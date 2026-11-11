use std::sync::LazyLock;

use sloplint_core::facts::{ArgShape, FileFacts, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn facts_of<'s>(source: &'s str, path: &'s str) -> FileFacts<'s> {
    let input = FileInput {
        lang: Lang::Python,
        path,
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &NO_CUSTOM);
    assert!(analysis.parse_error.is_none(), "case.py parses");
    analysis.facts
}

#[test]
fn concat_carries_longest_static_run() {
    let facts = facts_of(
        "log.error('Failed to open ' + path + ' for reading')\nlog.error(f'oops {err}')\nlog.error(f'{err}')\nlog.error(path + code)\n",
        "case.py",
    );
    let shapes: Vec<_> = facts.calls.iter().map(|c| &c.args[0].shape).collect();
    assert!(
        matches!(shapes[0], ArgShape::Concat { longest_static: 15 }),
        "long chain reports 15"
    );
    assert!(
        matches!(shapes[1], ArgShape::Concat { longest_static: 5 }),
        "short f-string reports 5"
    );
    assert!(
        matches!(shapes[2], ArgShape::Concat { longest_static: 0 }),
        "bare hole reports 0"
    );
    assert!(
        matches!(shapes[3], ArgShape::Other),
        "string-free chain stays other"
    );
}

#[test]
fn implicit_concat_operand_counts_as_static() {
    let facts = facts_of(
        "log.error(\"Failed to open \" \"the file: \" + path)\nlog.error(path + \"Failed to open \" \"file\")\n",
        "case.py",
    );
    assert!(
        matches!(
            facts.calls[0].args[0].shape,
            ArgShape::Concat { longest_static: 15 }
        ),
        "left implicit join reports 15"
    );
    assert!(
        matches!(
            facts.calls[1].args[0].shape,
            ArgShape::Concat { longest_static: 15 }
        ),
        "right implicit join reports 15"
    );
}

#[test]
fn keyed_literal_is_other_and_closure_collection_is_codebody() {
    let facts = facts_of(
        "build({'a': 1, 'b': 2})\nrun([lambda x: x])\nmake([1, 2])\n",
        "case.py",
    );
    assert!(
        matches!(facts.calls[0].args[0].shape, ArgShape::Other),
        "keyed literal stays other"
    );
    assert!(
        matches!(facts.calls[1].args[0].shape, ArgShape::CodeBody),
        "closure inside list is codebody"
    );
    assert!(
        matches!(facts.calls[2].args[0].shape, ArgShape::Collection),
        "plain list stays a collection"
    );
}

#[test]
fn receiver_call_callee_carries_no_arguments() {
    let facts = facts_of(
        "import logging\nlogging.getLogger(__name__).error('upload failed: %s', exc)\nlogger.error('oops')\n",
        "case.py",
    );
    let outer = facts
        .calls
        .iter()
        .find(|c| c.callee == "error")
        .expect("outer receiver call collapses to member");
    assert!(
        !outer.callee.contains('('),
        "callee never contains argument text"
    );
    assert!(
        outer.callee_root.is_none(),
        "chain bottoming at a call has no root"
    );
    assert!(
        !outer.is_simple_path,
        "chain through a call is not a simple path"
    );
    let nested = facts
        .calls
        .iter()
        .find(|c| c.callee == "logging.getLogger")
        .expect("nested call keeps its own path");
    assert_eq!(
        nested.callee_root,
        Some("logging"),
        "nested chain keeps its root"
    );
    let plain = facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.error")
        .expect("plain member chain keeps its path");
    assert!(
        plain.is_simple_path,
        "identifiers joined by members are a simple path"
    );
}

#[test]
fn subscript_receiver_has_no_root() {
    let facts = facts_of("items[0].strip()\nstore.get(key)\n", "case.py");
    let strip = facts
        .calls
        .iter()
        .find(|c| c.callee == "strip")
        .expect("has strip");
    assert!(
        strip.callee_root.is_none(),
        "chain bottoming at a subscript has no root"
    );
    assert!(
        !strip.is_simple_path,
        "chain through a subscript is not a simple path"
    );
    let plain = facts
        .calls
        .iter()
        .find(|c| c.callee == "store.get")
        .expect("has store.get");
    assert_eq!(
        plain.callee_root,
        Some("store"),
        "member chain keeps its root"
    );
    assert!(
        plain.is_simple_path,
        "two-level member chain is a simple path"
    );
}

#[test]
fn multi_level_member_chain_is_a_simple_path() {
    let facts = facts_of(
        "return helpers.render(name)\nreturn pkg.mod.render(name)\n",
        "case.py",
    );
    for call in &facts.calls {
        assert!(
            call.is_simple_path,
            "member-joined identifiers are a simple path"
        );
    }
    assert_eq!(facts.calls.len(), 2, "both dotted calls are recorded");
}

#[test]
fn comment_only_handler_body_is_empty() {
    let facts = facts_of(
        "def f(p):\n    try:\n        os.remove(p)\n    except FileNotFoundError:\n        # already gone\n        pass\ndef g(p):\n    try:\n        os.remove(p)\n    except FileNotFoundError:\n        log(p)\n",
        "case.py",
    );
    assert!(
        facts.handlers[0].stmts.is_empty(),
        "comment plus pass is empty"
    );
    assert_eq!(
        facts.handlers[1].stmts.len(),
        1,
        "real call stays a statement"
    );
}

#[test]
fn commented_import_only_try_stays_imports_only() {
    let facts = facts_of(
        "def f():\n    try:\n        # optional accelerator\n        import ujson\n    except ImportError:\n        pass\ndef g():\n    try:\n        # note\n        open('x').read()\n    except OSError:\n        pass\n",
        "case.py",
    );
    assert!(
        facts.handlers[0].try_imports_only,
        "comment does not break the import test"
    );
    assert!(
        !facts.handlers[1].try_imports_only,
        "non-import body stays false"
    );
}

#[test]
fn commented_struct_literal_keeps_source() {
    let facts = facts_of(
        "p = Point(\n    # coordinates\n    x=s.x, y=s.y, z=s.z, w=s.w, v=s.v, u=s.u)\nq = dict(a=b.a, c=b.c)\n",
        "case.py",
    );
    let hit = facts
        .field_copies
        .iter()
        .find(|c| c.source == Some("s"))
        .expect("commented copy keeps its source");
    assert_eq!(hit.fields, 6, "comment is not counted as a field");
    assert!(
        facts.field_copies.iter().all(|c| c.source != Some("b")),
        "mapping constructor stays silent"
    );
}

#[test]
fn comment_in_arguments_is_not_an_argument() {
    let facts = facts_of("x = render(  # forward\n    name)\n", "case.py");
    let call = facts
        .calls
        .iter()
        .find(|c| c.callee == "render")
        .expect("has render");
    assert_eq!(call.args.len(), 1, "comment is not an argument");
    assert!(
        matches!(call.args[0].shape, ArgShape::Ident("name")),
        "real argument is kept"
    );
}

#[test]
fn sole_generator_is_one_argument() {
    let facts = facts_of("total = any(x for x in items)\n", "case.py");
    let call = facts
        .calls
        .iter()
        .find(|c| c.callee == "any")
        .expect("has any");
    assert_eq!(call.args.len(), 1, "generator is a single argument");
    assert!(
        matches!(call.args[0].shape, ArgShape::Other),
        "generator is not split"
    );
}

#[test]
fn comprehension_argument_is_not_a_collection() {
    let facts = facts_of(
        "build(\n    [row.name for row in rows if row.ok]\n)\nbuild([\n    'red',\n    'green',\n])\n",
        "case.py",
    );
    assert!(
        matches!(facts.calls[0].args[0].shape, ArgShape::Other),
        "computed comprehension stays other"
    );
    assert!(
        matches!(facts.calls[1].args[0].shape, ArgShape::Collection),
        "literal list stays a collection"
    );
}

#[test]
fn parenthesised_ctors_mark_nullable_sibling() {
    let facts = facts_of(
        "def f(result):\n    match result:\n        case Ok(v):\n            return 1\n        case Err(e):\n            return 2\n        case _:\n            return 0\ndef g(result):\n    match result:\n        case 1:\n            return 1\n        case _:\n            return 0\n",
        "case.py",
    );
    assert!(
        facts.wildcard_arms[0].has_nullable_ctor,
        "Ok/Err silences the wildcard"
    );
    assert!(
        !facts.wildcard_arms[1].has_nullable_ctor,
        "literal sibling stays unset"
    );
}

#[test]
fn single_variant_match_is_not_enum_like() {
    let facts = facts_of(
        "def f(color):\n    match color:\n        case Color.RED:\n            return 1\n        case _:\n            return 0\ndef g(color):\n    match color:\n        case Color.RED:\n            return 1\n        case Color.GREEN:\n            return 2\n        case _:\n            return 0\n",
        "case.py",
    );
    let single = &facts.wildcard_arms[0];
    assert_eq!(single.variant_count, 1, "one sibling is counted");
    assert!(!single.enum_like, "one sibling is not enum-like");
    let pair = &facts.wildcard_arms[1];
    assert_eq!(pair.variant_count, 2, "two siblings are counted");
    assert!(pair.enum_like, "two siblings are enum-like");
}

#[test]
fn typed_star_args_make_followers_keyword_only() {
    let facts = facts_of(
        "def confirm(*args: str, abort: bool = False, err: bool = False):\n    return abort\ndef plain(a, b):\n    return a\n",
        "case.py",
    );
    let confirm = facts
        .functions
        .iter()
        .find(|f| f.name == "confirm")
        .expect("has confirm");
    for param in confirm
        .params
        .iter()
        .filter(|p| p.name == "abort" || p.name == "err")
    {
        assert!(
            param.keyword_only,
            "follower of typed star-args is keyword-only"
        );
    }
    let plain = facts
        .functions
        .iter()
        .find(|f| f.name == "plain")
        .expect("has plain");
    assert!(
        plain.params.iter().all(|p| !p.keyword_only),
        "plain params stay positional"
    );
}

#[test]
fn helper_in_test_file_is_in_test() {
    let source = "def make_client(retry: bool = False, verify: bool = True):\n    return retry\n";
    let in_test = facts_of(source, "tests/test_api.py");
    assert!(in_test.functions[0].in_test, "whole test file is in test");
    let in_lib = facts_of(source, "case.py");
    assert!(
        !in_lib.functions[0].in_test,
        "same helper outside tests is library"
    );
}

#[test]
fn dunder_of_private_class_is_private() {
    let facts = facts_of(
        "class _Impl:\n    def __call__(self, x: int) -> int:\n        return x\nclass Service:\n    def __len__(self):\n        return 0\n",
        "case.py",
    );
    let private = facts
        .functions
        .iter()
        .find(|f| f.name == "__call__")
        .expect("has call");
    assert_eq!(
        private.visibility,
        Visibility::Private,
        "dunder of private class is private"
    );
    let public = facts
        .functions
        .iter()
        .find(|f| f.name == "__len__")
        .expect("has len");
    assert_eq!(
        public.visibility,
        Visibility::Exported,
        "dunder of public class stays exported"
    );
}

#[test]
fn same_name_alias_is_not_a_rename() {
    let facts = facts_of(
        "from os import path as path\nfrom os import path as p\n",
        "case.py",
    );
    let same = facts
        .imports
        .iter()
        .find(|i| i.alias.is_none())
        .expect("has plain");
    assert_eq!(same.name, "path", "same-name alias records the name");
    let renamed = facts
        .imports
        .iter()
        .find(|i| i.alias == Some("p"))
        .expect("has rename");
    assert_eq!(renamed.name, "path", "real rename keeps its alias");
}

#[test]
fn sole_call_survives_a_leading_comment() {
    let facts = facts_of(
        "def greet(name):\n    # forwards to the renderer\n    return render_greeting(name)\ndef two(name):\n    log(name)\n    return render_greeting(name)\n",
        "case.py",
    );
    let greet = facts
        .functions
        .iter()
        .find(|f| f.name == "greet")
        .expect("has greet");
    assert!(
        greet.sole_call.is_some(),
        "comment does not hide the sole call"
    );
    let two = facts
        .functions
        .iter()
        .find(|f| f.name == "two")
        .expect("has two");
    assert!(
        two.sole_call.is_none(),
        "two statements stay without a sole call"
    );
}

#[test]
fn late_literal_loop_still_suppresses_dynamic() {
    let mut source = String::from("def f(mod):\n");
    for _ in 0..800 {
        source.push_str("    x = 1\n");
    }
    source.push_str("    for name in ('a', 'b'):\n        getattr(mod, name)\n");
    let facts = facts_of(&source, "case.py");
    assert!(
        facts.dynamic_symbols.is_empty(),
        "loop past the old budget still suppresses"
    );
    let loud = facts_of(
        "def f(mod, other):\n    for name in other:\n        getattr(mod, name)\n",
        "case.py",
    );
    assert_eq!(
        loud.dynamic_symbols.len(),
        1,
        "non-literal loop keeps the dynamic fact"
    );
}

#[test]
fn handler_call_callee_matches_call_fact() {
    let facts = facts_of(
        "def f(p):\n    try:\n        x(p)\n    except OSError:\n        logging.getLogger(__name__).error('x')\ndef g(p):\n    try:\n        x(p)\n    except OSError:\n        log('x')\n",
        "case.py",
    );
    let first = &facts.handlers[0].stmts;
    assert!(
        matches!(
            first[0],
            sloplint_core::facts::Stmt::Call {
                callee: "error",
                ..
            }
        ),
        "handler collapses the receiver call"
    );
    let second = &facts.handlers[1].stmts;
    assert!(
        matches!(
            second[0],
            sloplint_core::facts::Stmt::Call { callee: "log", .. }
        ),
        "plain handler call keeps its name"
    );
}

#[test]
fn top_level_tests_dir_is_in_test() {
    let source = "def make_flags(retry: bool = False, verify: bool = True):\n    return retry\n";
    let in_test = facts_of(source, "tests/helpers.py");
    assert!(
        in_test.functions[0].in_test,
        "root-relative tests dir is in test"
    );
    let in_lib = facts_of(source, "src/helpers.py");
    assert!(
        !in_lib.functions[0].in_test,
        "same helper outside tests is library"
    );
}

#[test]
fn backslash_continuation_keeps_full_callee_path() {
    let facts = facts_of(
        "x = log \\\n    .error('x')\ny = log.error('x')\n",
        "case.py",
    );
    let broken = facts
        .calls
        .iter()
        .find(|c| c.callee == "log \\\n    .error")
        .expect("has line-broken path");
    assert!(
        broken.is_simple_path,
        "identifier-only chain stays a simple path"
    );
    let clean = facts
        .calls
        .iter()
        .find(|c| c.callee == "log.error")
        .expect("has clean path");
    assert!(
        clean.is_simple_path,
        "clean member chain stays a simple path"
    );
}

#[test]
fn many_sites_share_one_literal_walk() {
    let mut source = String::from("def f(mod):\n    for name in ('a', 'b'):\n");
    for i in 0..300 {
        source.push_str(&format!("        v{i} = getattr(mod, name)\n"));
    }
    let facts = facts_of(&source, "case.py");
    assert!(
        facts.dynamic_symbols.is_empty(),
        "300 sites share the loop targets"
    );
}
