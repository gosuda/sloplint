//! TypeScript contract facts: type visibility, switch enum counts, callee roots,
//! closure facts, and assertion silence.

use std::sync::LazyLock;

use sloplint_core::facts::{ArgShape, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::TypeScript,
        path,
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn type_of<'a>(a: &'a Analysis<'a>, name: &str) -> &'a sloplint_core::facts::TypeDef<'a> {
    a.facts
        .types
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("has type {name}"))
}

fn arg_shape_of<'a>(a: &'a Analysis<'a>, callee: &str) -> &'a ArgShape<'a> {
    &a.facts
        .calls
        .iter()
        .find(|c| c.callee == callee)
        .unwrap_or_else(|| panic!("has call {callee}"))
        .args
        .first()
        .unwrap_or_else(|| panic!("call has an arg"))
        .shape
}

fn root_of<'a>(a: &'a Analysis<'a>, callee: &str) -> Option<&'a str> {
    a.facts
        .calls
        .iter()
        .find(|c| c.callee == callee)
        .unwrap_or_else(|| panic!("has call {callee}"))
        .callee_root
}

#[test]
fn export_list_marks_the_type_visibility_exported() {
    let a = run(
        "render.ts",
        "interface Renderer { render(): void }\nexport { Renderer };\nclass Internal {}\n",
    );
    assert!(a.parse_error.is_none(), "parses");
    assert_eq!(
        type_of(&a, "Renderer").visibility,
        Visibility::Exported,
        "a type named in an export list is visible outside the module"
    );
    assert_eq!(
        type_of(&a, "Internal").visibility,
        Visibility::Private,
        "a type never exported stays private"
    );
}

#[test]
fn two_enum_member_cases_make_the_switch_enum_like() {
    let a = run(
        "colors.ts",
        "enum Color { Red, Blue }\nenum Tone { Warm, Cold }\nfunction pick(c: Color, t: Tone) {\n  switch (c) { case Color.Red: use(c); break; default: use(c); }\n  switch (t) { case Tone.Warm: use(t); break; case Tone.Cold: use(t); break; default: use(t); }\n}\n",
    );
    assert!(a.parse_error.is_none(), "parses");
    let arms = &a.facts.wildcard_arms;
    assert_eq!(arms.len(), 2, "one arm per default");
    assert!(
        !arms[0].enum_like,
        "one named member case is below the two-variant requirement"
    );
    assert_eq!(arms[0].variant_count, 1, "single member case counted");
    assert!(arms[1].enum_like, "two named member cases stay enum-like");
    assert_eq!(arms[1].variant_count, 2, "both member cases counted");
}

#[test]
fn constructor_calls_root_at_their_identifier() {
    let a = run(
        "new.ts",
        "const p = new Parser(opts);\nconst q = new ns.Parser(opts);\nconst r = new (factory())();\n",
    );
    assert!(a.parse_error.is_none(), "parses");
    assert_eq!(
        root_of(&a, "Parser"),
        Some("Parser"),
        "a bare constructor identifier is the callee root"
    );
    assert_eq!(
        root_of(&a, "ns.Parser"),
        Some("ns"),
        "a qualified constructor roots at the leftmost identifier"
    );
    assert_eq!(
        root_of(&a, "(factory())"),
        None,
        "a parenthesized factory call still has no path root"
    );
}

#[test]
fn type_wrapper_chains_root_at_their_identifier() {
    let a = run(
        "wrap.ts",
        "obj!.run(x);\n(req as Req).send(x);\nmake()!.call(x);\n",
    );
    assert!(a.parse_error.is_none(), "parses");
    assert_eq!(
        root_of(&a, "obj!.run"),
        Some("obj"),
        "a non-null assertion does not hide the callee root"
    );
    assert_eq!(
        root_of(&a, "(req as Req).send"),
        Some("req"),
        "an as-cast receiver keeps the callee root"
    );
    assert_eq!(
        root_of(&a, "call"),
        None,
        "a call receiver still has no path root"
    );
}

#[test]
fn concat_carries_the_longest_static_run() {
    let a = run(
        "concat.ts",
        "logger.error('Failed to open ' + path);\nlogger.info(`err ${code}`);\nlogger.debug(`${a}${b}`);\nlogger.warn('plain note');\n",
    );
    assert!(a.parse_error.is_none(), "parses");
    assert_eq!(
        arg_shape_of(&a, "logger.error"),
        &ArgShape::Concat { longest_static: 15 },
        "a + chain measures its longest static operand"
    );
    assert_eq!(
        arg_shape_of(&a, "logger.info"),
        &ArgShape::Concat { longest_static: 4 },
        "a template measures its longest static chunk"
    );
    assert_eq!(
        arg_shape_of(&a, "logger.debug"),
        &ArgShape::Concat { longest_static: 0 },
        "an all-dynamic template measures zero"
    );
    assert!(
        matches!(arg_shape_of(&a, "logger.warn"), ArgShape::Str { .. }),
        "a plain string stays Str"
    );
}

#[test]
fn keyed_literals_are_not_collections() {
    let a = run(
        "shapes.ts",
        "plain({ retries: 3 });\nlisted([1, 2]);\nclosed([() => { work(); }]);\n",
    );
    assert!(a.parse_error.is_none(), "parses");
    assert_eq!(
        arg_shape_of(&a, "plain"),
        &ArgShape::Other,
        "a keyed literal is not a Collection"
    );
    assert_eq!(
        arg_shape_of(&a, "listed"),
        &ArgShape::Collection,
        "a plain array stays a Collection"
    );
    assert_eq!(
        arg_shape_of(&a, "closed"),
        &ArgShape::CodeBody,
        "an array holding a closure is a body"
    );
}

#[test]
fn receiver_call_callee_drops_to_member() {
    let a = run("chain.ts", "ctx.log().error('x');\na.b.c('y');\n");
    assert!(a.parse_error.is_none(), "parses");
    let collapsed = a
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "error")
        .expect("has the collapsed call");
    assert!(
        !collapsed.is_simple_path,
        "a call receiver is not a simple path"
    );
    assert!(
        collapsed.callee_root.is_none(),
        "a collapsed chain has no root, got {:?}",
        collapsed.callee_root
    );
    let plain = a
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "a.b.c")
        .expect("has the plain call");
    assert!(
        plain.is_simple_path,
        "a plain member chain stays a simple path"
    );
    assert_eq!(plain.callee_root, Some("a"), "a plain chain keeps its root");
}

#[test]
fn block_local_callback_is_its_own_function() {
    let a = run(
        "pick.ts",
        "export function run(items: Item[]) {\n  const pick = (x: Item) => {\n    if (x.a) {\n      for (const y of x.b) {\n        if (y.c) {\n          return y;\n        }\n      }\n    }\n    return null;\n  };\n  return items.map(pick);\n}\n",
    );
    assert!(a.parse_error.is_none(), "parses");
    let pick = a
        .facts
        .functions
        .iter()
        .find(|f| f.name == "pick")
        .unwrap_or_else(|| panic!("block-local callback has a Function fact"));
    assert_eq!(
        pick.visibility,
        Visibility::Private,
        "a block-local binding stays private"
    );
    assert!(
        a.facts.functions.iter().any(|f| f.name == "run"),
        "the enclosing function keeps its fact"
    );
}

#[test]
fn default_exported_closure_is_exported() {
    let a = run(
        "def.ts",
        "export default function (copy: boolean, verbose: boolean) { return copy && verbose; }\nexport function run(items: number[]) { return items.map((x) => x * 2); }\n",
    );
    assert!(a.parse_error.is_none(), "parses");
    assert!(
        a.facts
            .functions
            .iter()
            .any(|f| f.name.is_empty() && f.visibility == Visibility::Exported),
        "a default-exported function is actionable"
    );
    assert!(
        a.facts
            .functions
            .iter()
            .any(|f| f.name.is_empty() && f.visibility == Visibility::Private),
        "an argument callback stays private"
    );
}

#[test]
fn assert_calls_emit_an_assertion_fact() {
    let a = run(
        "check.ts",
        "import assert from 'node:assert';\nexport function check(ok: boolean): void {\n  assert(ok);\n}\n",
    );
    assert!(a.parse_error.is_none(), "parses");
    assert!(
        a.facts.calls.iter().any(|c| c.callee == "assert"),
        "the call itself is still recorded"
    );
    assert_eq!(
        a.facts.assertions.len(),
        1,
        "a bare-identifier assert emits one Assertion fact"
    );
    assert!(
        matches!(
            a.facts.assertions[0].shape,
            sloplint_core::facts::AssertShape::BareName
        ),
        "an identifier condition is a bare-name assertion"
    );
}
