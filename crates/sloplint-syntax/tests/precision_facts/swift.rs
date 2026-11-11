use std::sync::LazyLock;

use sloplint_core::facts::{AmbientKind, ArgShape, PrimitiveKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Swift,
        path,
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn fact_of<'a>(analysis: &'a Analysis<'a>, name: &str) -> &'a sloplint_core::facts::Function<'a> {
    analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("has function {name}"))
}

#[test]
fn visibility_maps_modifiers_to_three_states() {
    let analysis = run(
        "case.swift",
        "public func a() {}\nfunc b() {}\nprivate func c() {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        fact_of(&analysis, "a").visibility,
        Visibility::Exported,
        "public is callable from another module"
    );
    assert_eq!(
        fact_of(&analysis, "b").visibility,
        Visibility::Restricted,
        "the default internal access is module-restricted"
    );
    assert_eq!(
        fact_of(&analysis, "c").visibility,
        Visibility::Private,
        "private stays file-private"
    );
}

#[test]
fn override_marks_the_signature_forced() {
    let analysis = run(
        "case.swift",
        "class A {\n    func f() {}\n}\nclass B: A {\n    override func f() {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let mut fs = analysis.facts.functions.iter().filter(|f| f.name == "f");
    let plain = fs.next().expect("has the base method");
    let forced = fs.next().expect("has the override");
    assert!(
        !plain.signature_forced,
        "a plain method keeps an editable signature"
    );
    assert!(
        forced.signature_forced,
        "an override restates a forced signature"
    );
}

#[test]
fn primitive_param_kinds_follow_the_swift_type_table() {
    let analysis = run(
        "case.swift",
        "func f(a: String, b: Bool, c: Int, d: Double, e: [String]) {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let kinds: Vec<PrimitiveKind> = fact_of(&analysis, "f")
        .params
        .iter()
        .map(|p| p.primitive_kind)
        .collect();
    assert_eq!(
        kinds,
        [
            PrimitiveKind::Str,
            PrimitiveKind::Bool,
            PrimitiveKind::Int,
            PrimitiveKind::Float,
            PrimitiveKind::Other
        ],
        "String Bool Int Double map by name and an array type stays Other"
    );
}

#[test]
fn swift_assert_shapes_split_comparison_from_bare_name() {
    let analysis = run(
        "case.swift",
        "func check(x: Int, y: Int, ready: Bool) {\n    assert(x == y)\n    assert(ready)\n    assert(load())\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        analysis.facts.assertions.len(),
        3,
        "all asserts are recorded"
    );
    let first = &analysis.facts.assertions[0];
    let second = &analysis.facts.assertions[1];
    let third = &analysis.facts.assertions[2];
    assert!(first.comparison, "an equality condition is a comparison");
    assert_eq!(
        first.shape,
        sloplint_core::facts::AssertShape::TopEqNe,
        "an equality condition is top eq-ne"
    );
    assert!(!second.comparison, "a bare name is not a comparison");
    assert_eq!(
        second.shape,
        sloplint_core::facts::AssertShape::BareName,
        "a bare name is a bare name"
    );
    assert!(!third.comparison, "a call result is not a comparison");
    assert_eq!(
        third.shape,
        sloplint_core::facts::AssertShape::Other,
        "a call result keeps the default shape"
    );
    for assertion in &analysis.facts.assertions {
        assert!(!assertion.in_typecheck, "swift has no TYPE_CHECKING frame");
        assert!(
            !assertion.value_is_bool_literal,
            "bool-literal values are a go-only fact"
        );
    }
}

#[test]
fn swift_module_imports_carry_the_module_as_external_root() {
    let analysis = run("case.swift", "import Foundation\nfunc f() {}\n");
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(analysis.facts.imports.len(), 1, "the import is recorded");
    let import = &analysis.facts.imports[0];
    assert_eq!(
        import.external_root,
        Some("Foundation"),
        "a module import names its module as the external root"
    );
    assert!(!import.is_reexport, "an import is not a re-export");
    assert!(!import.in_test, "a library import is not test-scoped");
    let bare = run("case.swift", "func f() {}\n");
    assert!(bare.parse_error.is_none(), "parses");
    assert!(
        bare.facts.imports.is_empty(),
        "a file without imports records none"
    );
}

#[test]
fn in_function_write_to_top_level_var_is_one_global_write() {
    let analysis = run("case.swift", "var count = 0\nfunc bump() {\ncount = 1\n}\n");
    assert!(analysis.parse_error.is_none(), "parses");
    let mutables: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalMutable)
        .collect();
    assert_eq!(mutables.len(), 1, "one top-level var is a global mutable");
    assert_eq!(mutables[0].name, "count", "mutable keeps the declared name");
    let writes: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .collect();
    assert_eq!(writes.len(), 1, "in-function write joins the top-level var");
    assert_eq!(writes[0].name, "count", "write keeps the assigned name");
    assert!(
        writes[0].in_function.is_some(),
        "write is attributed inside the function"
    );
}

#[test]
fn init_declaration_keeps_a_function_fact() {
    let analysis = run(
        "case.swift",
        "public class C {\npublic init(verbose: Bool, quiet: Bool) {}\nfunc f() {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let init = fact_of(&analysis, "init");
    assert_eq!(
        init.params.len(),
        2,
        "the initializer keeps both parameters"
    );
    assert_eq!(
        init.visibility,
        Visibility::Exported,
        "a public initializer in a public class is exported"
    );
    let plain = fact_of(&analysis, "f");
    assert!(
        plain.params.is_empty(),
        "the plain method keeps no parameters"
    );
    let internal = run(
        "case.swift",
        "class C {\npublic init(verbose: Bool, quiet: Bool) {}\n}\n",
    );
    assert!(internal.parse_error.is_none(), "parses");
    assert_eq!(
        fact_of(&internal, "init").visibility,
        Visibility::Restricted,
        "a public member of an internal type stays restricted"
    );
}

#[test]
fn defaulted_params_mark_has_default() {
    let analysis = run(
        "case.swift",
        "func f(a: String, b: String = \"x\") {}\nfunc g(flag: Bool = true) {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let params = &fact_of(&analysis, "f").params;
    assert!(!params[0].has_default, "a plain parameter has no default");
    assert!(params[1].has_default, "an equals default marks has_default");
    assert!(
        !params[1].bool_default,
        "a string default is not a bool default"
    );
    let flag = &fact_of(&analysis, "g").params[0];
    assert!(flag.has_default, "a bool default marks has_default");
    assert!(flag.bool_default, "a true default marks bool_default");
}

#[test]
fn main_attribute_marks_the_entry_point() {
    let attributed = run(
        "case.swift",
        "@main struct App {\nstatic func main() {}\n}\n",
    );
    assert!(attributed.parse_error.is_none(), "parses");
    assert!(
        fact_of(&attributed, "main").entry_point,
        "the @main type entry is an entry point"
    );
    let bare = run("case.swift", "func main() {}\n");
    assert!(bare.parse_error.is_none(), "parses");
    assert!(
        !fact_of(&bare, "main").entry_point,
        "a free main without @main stays a plain function"
    );
}

#[test]
fn nested_main_inside_attributed_type_stays_plain() {
    let analysis = run(
        "case.swift",
        "@main struct App {\nstruct Helper {\nstatic func main() {}\n}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert!(
        !fact_of(&analysis, "main").entry_point,
        "only the immediate @main type entry counts"
    );
}

#[test]
fn receiver_keywords_are_callee_roots() {
    let analysis = run(
        "case.swift",
        "func f() {\nself.render(name)\nsuper.render(name)\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let roots: Vec<_> = analysis.facts.calls.iter().map(|c| c.callee_root).collect();
    assert!(roots.contains(&Some("self")), "self stays a receiver root");
    assert!(
        roots.contains(&Some("super")),
        "super stays a receiver root"
    );
}

#[test]
fn interpolated_strings_carry_the_static_run() {
    let analysis = run(
        "case.swift",
        "func f(a: String, b: String) {\nlog.info(\"Processing request \\(a)\")\nlog.info(\"\\(a) ok\")\nlog.info(\"\\(a)\\(b)\")\nlog.info(\"plain message here\")\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let mut infos = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee.contains("info"));
    let long = infos.next().expect("has the long-static call");
    let short = infos.next().expect("has the short-static call");
    let dynamic = infos.next().expect("has the all-dynamic call");
    let still = infos.next().expect("has the static call");
    assert!(
        matches!(long.args[0].shape, ArgShape::Concat { longest_static: 19 }),
        "a nineteen-char static run measures nineteen"
    );
    assert!(
        matches!(short.args[0].shape, ArgShape::Concat { longest_static: 3 }),
        "a three-char static run measures three"
    );
    assert!(
        matches!(
            dynamic.args[0].shape,
            ArgShape::Concat { longest_static: 0 }
        ),
        "two interpolations with no static text measure zero"
    );
    assert!(
        matches!(still.args[0].shape, ArgShape::Str { .. }),
        "a static literal stays Str"
    );
}

#[test]
fn keyed_literals_are_not_collections() {
    let analysis = run(
        "case.swift",
        "foo([\n1,\n2,\n3])\nfoo([\n\"k\": 1])\nfoo([\n{ $0 }])\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let mut calls = analysis.facts.calls.iter().filter(|c| c.callee == "foo");
    let list = calls.next().expect("has the list call");
    let keyed = calls.next().expect("has the keyed call");
    let closed = calls.next().expect("has the closure call");
    assert!(
        matches!(list.args[0].shape, ArgShape::Collection),
        "a multiline list stays Collection"
    );
    assert!(
        matches!(keyed.args[0].shape, ArgShape::Other),
        "a keyed literal is Other, not Collection"
    );
    assert!(
        matches!(closed.args[0].shape, ArgShape::CodeBody),
        "a list holding a closure is CodeBody"
    );
}

#[test]
fn chained_concat_measures_every_operand() {
    let analysis = run(
        "case.swift",
        "func f(path: String, prefix: String) {\nlog.info(\"failed to open the cache file \" + path + \" now\")\nlog.info(prefix + \" ok\")\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let mut infos = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee.contains("info"));
    let chained = infos.next().expect("has the chained call");
    let short = infos.next().expect("has the short chain call");
    assert!(
        matches!(
            chained.args[0].shape,
            ArgShape::Concat { longest_static: 30 }
        ),
        "the left-nested thirty-char literal sets the run"
    );
    assert!(
        matches!(short.args[0].shape, ArgShape::Concat { longest_static: 3 }),
        "a short trailing literal measures three"
    );
}

#[test]
fn labelled_argument_span_covers_the_label() {
    let analysis = run("case.swift", "foo(\nlabel:\n[1, 2, 3])\nfoo([1])\n");
    assert!(analysis.parse_error.is_none(), "parses");
    let mut calls = analysis.facts.calls.iter().filter(|c| c.callee == "foo");
    let split = calls.next().expect("has the split call");
    let single = calls.next().expect("has the single-line call");
    assert_eq!(split.args[0].start_line, 2, "the span starts at the label");
    assert_eq!(split.args[0].end_line, 3, "the span ends at the value");
    assert_eq!(
        single.args[0].start_line, single.args[0].end_line,
        "a single-line argument stays on one line"
    );
}

#[test]
fn dynamic_names_follow_interpolation() {
    let analysis = run(
        "case.swift",
        "func f(name: String) {\nNSSelectorFromString(\"get\\(name)\")\nNSSelectorFromString(\"get\")\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        analysis.facts.dynamic_symbols.len(),
        1,
        "only the interpolated name is dynamic"
    );
}

#[test]
fn a_raw_string_name_is_dynamic_only_when_it_interpolates() {
    let analysis = run(
        "case.swift",
        "func f(verb: String) {\nNSSelectorFromString(#\"\\#(verb)Action\"#)\nNSSelectorFromString(#\"doAction\"#)\nNSSelectorFromString(#\"\\(verb)Action\"#)\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        analysis.facts.dynamic_symbols.len(),
        1,
        "only the raw string with a pound interpolation is dynamic"
    );
}

#[test]
fn dotted_imports_share_the_first_segment() {
    let analysis = run(
        "case.swift",
        "import Foo.Bar\nimport Foo.Baz\nimport Foundation\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        analysis.facts.imports[0].external_root,
        Some("Foo"),
        "the first dotted import roots at Foo"
    );
    assert_eq!(
        analysis.facts.imports[1].external_root,
        Some("Foo"),
        "the second dotted import roots at Foo"
    );
    assert_eq!(
        analysis.facts.imports[2].external_root,
        Some("Foundation"),
        "a single-segment import keeps its module"
    );
}

#[test]
fn exported_import_marks_reexport() {
    let analysis = run("case.swift", "@_exported import Core\nimport Core\n");
    assert!(analysis.parse_error.is_none(), "parses");
    assert!(
        analysis.facts.imports[0].is_reexport,
        "the @_exported import is a re-export"
    );
    assert!(
        !analysis.facts.imports[1].is_reexport,
        "a plain import stays a first-party import"
    );
}

#[test]
fn wildcard_arms_count_siblings_and_nullable_ctors() {
    let analysis = run(
        "case.swift",
        "func f(opt: Int, c: Int, s: Int, e: Int) {\nswitch opt {\ncase .some(let x): a(x)\ncase .none: b()\ndefault: c()\n}\nswitch c {\ncase .red: a()\ndefault: b()\n}\nswitch s {\ncase .a, .b: a()\ndefault: b()\n}\nswitch e {\ncase .error(let e): f(e)\ncase .idle: g()\ndefault: h()\n}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        analysis.facts.wildcard_arms.len(),
        4,
        "each default arm is recorded"
    );
    let nullable = &analysis.facts.wildcard_arms[0];
    assert!(
        nullable.has_nullable_ctor,
        "a .some/.none sibling sets the flag"
    );
    assert_eq!(nullable.variant_count, 2, "two enum siblings count as two");
    assert!(nullable.enum_like, "two same-root siblings are enum-like");
    let single = &analysis.facts.wildcard_arms[1];
    assert_eq!(single.variant_count, 1, "one enum sibling counts as one");
    assert!(!single.enum_like, "a lone sibling is not enum-like");
    assert!(!single.has_nullable_ctor, "a plain case sets no flag");
    let multi = &analysis.facts.wildcard_arms[2];
    assert_eq!(
        multi.variant_count, 2,
        "a two-pattern case counts each pattern"
    );
    assert!(multi.enum_like, "two patterns of one switch stay enum-like");
    let prefixed = &analysis.facts.wildcard_arms[3];
    assert_eq!(prefixed.variant_count, 2, "two custom cases count as two");
    assert!(
        !prefixed.has_nullable_ctor,
        ".error shares a prefix with err but is not a nullable ctor"
    );
    assert!(
        prefixed.enum_like,
        "two same-root custom cases stay enum-like"
    );
}

#[test]
fn final_without_override_keeps_editable_signature() {
    let analysis = run(
        "case.swift",
        "class A {\nfunc f() {}\n}\nclass B: A {\noverride func f() {}\npublic final func g(a: Bool, b: Bool) {}\n}\npublic final class Doc: Encodable {\npublic func encode(to e: Int, pretty: Bool) {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let mut fs: Vec<_> = analysis.facts.functions.iter().collect();
    fs.sort_by(|a, b| a.name.cmp(b.name));
    let forced = fs.iter().find(|f| f.name == "f" && f.signature_forced);
    assert!(forced.is_some(), "an override restates a forced signature");
    for name in ["g", "encode"] {
        let f = fs.iter().find(|f| f.name == name).expect("has the method");
        assert!(
            !f.signature_forced,
            "ruling 18 leaves {name} without override editable"
        );
    }
}

#[test]
fn call_chain_reports_the_member_name_alone() {
    let analysis = run(
        "case.swift",
        "func f() {\nAppEnvironment.current().logger.info(\"ok\")\ncache.items[0].info(\"ok\")\nlog.info(\"ok\")\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let mut infos: Vec<_> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "info" || c.callee == "log.info")
        .collect();
    infos.sort_by_key(|a| a.callee.len());
    assert_eq!(infos.len(), 3, "all three info calls are recorded");
    for chained in &infos[0..2] {
        assert_eq!(
            chained.callee, "info",
            "a call or index in the chain keeps the member name"
        );
        assert_eq!(
            chained.callee_root, None,
            "a call or index in the chain has no root"
        );
        assert!(
            !chained.is_simple_path,
            "a call or index in the chain is not a simple path"
        );
        assert!(
            !chained.callee.contains('('),
            "a callee never carries argument text"
        );
        assert!(
            !chained.callee.contains('['),
            "a callee never carries index text"
        );
    }
    let plain = infos[2];
    assert_eq!(
        plain.callee, "log.info",
        "a plain receiver chain keeps its dots"
    );
    assert_eq!(
        plain.callee_root,
        Some("log"),
        "a plain receiver keeps its root"
    );
    assert!(
        plain.is_simple_path,
        "a plain receiver chain is a simple path"
    );
}

#[test]
fn try_and_await_calls_still_initialize_their_binding() {
    let analysis = run(
        "case.swift",
        "func f() async throws {\nlet a = try decode(x)\nlet b = await fetch(x)\nlet c = try await load(x)\ndecode(y)\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let bound: Vec<_> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| matches!(c.callee, "decode" | "fetch" | "load"))
        .map(|c| (c.callee, c.initializes_binding))
        .collect();
    assert_eq!(
        bound.len(),
        4,
        "four initializer-position calls are recorded"
    );
    assert!(bound[0].1, "a try call initializes its binding");
    assert!(bound[1].1, "an await call initializes its binding");
    assert!(bound[2].1, "a try await call initializes its binding");
    assert!(!bound[3].1, "a bare statement call initializes no binding");
}
