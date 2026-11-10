//! First-at-nest cognitive complexity, observed through `analyze` only.
//!
//! Every expectation follows one formula. An `if`, loop, or `catch` at nest 0
//! costs nothing; the first scored control at nest `n` costs `1 + n` and each
//! further control at the same nest costs 1. Match, switch, ternary, and R
//! `switch()` calls cost nothing and open no cognitive nest but still open one
//! control-depth level; `else if` costs nothing and opens none; a `with` block
//! opens nothing; a nested function restarts at nest 0; a closure bound by a
//! declarator is its own function while a headless closure shell opens no
//! nesting level, and the bound closure's inner controls never count above it.

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

/// `(cognitive, max_depth)` of the function called `name` in `source`.
fn metrics(lang: Lang, path: &str, source: &str, name: &str) -> (u32, u32) {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang,
        path,
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "{path} parses");
    let function = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{path} has function {name}"));
    (function.cognitive, function.max_depth)
}

fn rust(source: &str, name: &str) -> (u32, u32) {
    metrics(Lang::Rust, "case.rs", source, name)
}

fn ts(source: &str, name: &str) -> (u32, u32) {
    metrics(Lang::TypeScript, "case.ts", source, name)
}

fn py(source: &str, name: &str) -> (u32, u32) {
    metrics(Lang::Python, "case.py", source, name)
}

#[test]
fn sequential_top_level_ifs_score_zero() {
    let flat = "fn flat(n: u8) {\n    if n > 0 {}\n    if n > 1 {}\n    if n > 2 {}\n}\n";
    assert_eq!(rust(flat, "flat"), (0, 1), "three nest-0 ifs cost nothing");
    let after =
        "fn after(n: u8) {\n    if n > 0 {\n        if n > 1 {}\n    }\n    if n > 2 {}\n}\n";
    assert_eq!(
        rust(after, "after"),
        (2, 2),
        "nested if is first at 1, trailing if is nest 0"
    );
}

#[test]
fn nested_ifs_pay_first_at_nest_then_siblings_pay_one() {
    let nested = "fn nested(n: u8) {\n    if n > 0 {\n        if n > 1 {\n            if n > 2 {}\n        }\n    }\n}\n";
    assert_eq!(rust(nested, "nested"), (5, 3), "0 + 2 + 3");
    let siblings = "fn siblings(a: u8, b: u8) {\n    if a > 0 {\n        if a > 1 {}\n        if b > 1 {}\n    }\n}\n";
    assert_eq!(
        rust(siblings, "siblings"),
        (3, 2),
        "first at nest 1 costs 2, the sibling costs 1"
    );
}

#[test]
fn matches_ternaries_and_switches_open_depth_without_scoring() {
    let mixed = "fn mixed(xs: &[u32], k: u32) {\n    for x in xs {\n        if *x > k {\n            match x {\n                _ => {}\n            }\n        }\n    }\n}\n";
    assert_eq!(
        rust(mixed, "mixed"),
        (2, 3),
        "for 0 + if 2, match opens depth only"
    );
    let ternary = "function f(a: boolean, b: boolean) {\n  return a ? (b ? 1 : 2) : 3;\n}\n";
    assert_eq!(
        ts(ternary, "f"),
        (0, 2),
        "ternaries score nothing but open depth"
    );
    let switch = "function h(xs: number[]) {\n  for (const x of xs) {\n    switch (x) {\n      case 1: break;\n    }\n  }\n}\n";
    assert_eq!(ts(switch, "h"), (0, 2), "for 0, switch opens depth only");
}

#[test]
fn else_if_adds_nothing_and_opens_nothing() {
    let chain = "fn chain(n: u8) {\n    if n > 0 {} else if n > 1 {} else if n > 2 {} else {}\n}\n";
    assert_eq!(rust(chain, "chain"), (0, 1), "if 0 plus else-if arms at 0");
    let inner =
        "fn inner(n: u8) {\n    if n > 0 {\n        if n > 1 {} else if n > 2 {}\n    }\n}\n";
    assert_eq!(rust(inner, "inner"), (2, 2), "nested if 2 plus else-if 0");
    let elif =
        "def f(a, b, c):\n    if a:\n        pass\n    elif b:\n        if c:\n            pass\n";
    assert_eq!(
        py(elif, "f"),
        (2, 2),
        "elif 0, if inside elif is first at 1"
    );
    let ladder = "def g(a, b, c):\n    if a:\n        pass\n    elif b:\n        pass\n    elif c:\n        pass\n    else:\n        pass\n";
    assert_eq!(py(ladder, "g"), (0, 1), "a flat ladder scores nothing");
}

#[test]
fn catch_scores_at_its_nest_and_opens_one() {
    let ts_catch = "function g(a: boolean) {\n  try {\n    if (a) { run(); }\n  } catch (e) {\n    if (a) { recover(); }\n  }\n}\n";
    assert_eq!(ts(ts_catch, "g"), (2, 2), "catch 0 + if in catch 2");
    let py_except = "def e(a):\n    try:\n        pass\n    except ValueError:\n        if a:\n            pass\n";
    assert_eq!(py(py_except, "e"), (2, 2), "except 0 + if in except 2");
}

#[test]
fn with_blocks_open_no_nest_and_cost_nothing() {
    let alone = "def f(p):\n    with open(p) as h:\n        pass\n";
    assert_eq!(py(alone, "f"), (0, 0), "a with scores 0 and opens no level");
    let held = "def g(a, p):\n    with open(p) as h:\n        if a:\n            pass\n";
    assert_eq!(py(held, "g"), (0, 1), "if inside with is still nest 0");
    let twice = "def k(a, p, q):\n    with open(p) as h:\n        with open(q) as i:\n            if a:\n                pass\n";
    assert_eq!(py(twice, "k"), (0, 1), "two withs change nothing");
}

#[test]
fn nested_functions_restart_nesting() {
    let defs = "def outer(a, b):\n    if a:\n        pass\n    def inner(c):\n        if c:\n            if b:\n                pass\n    if b:\n        pass\n";
    assert_eq!(
        py(defs, "outer"),
        (0, 1),
        "inner's ifs are not charged to outer"
    );
    assert_eq!(py(defs, "inner"), (2, 2), "inner restarts at nest 0");
}

#[test]
fn headed_closures_are_their_own_function() {
    let source = "function host(n: number) {\n  if (n) { run(n); }\n}\nconst worker = (m: number) => {\n  if (m) {\n    if (n) { run(m); }\n  }\n};\n";
    assert_eq!(
        ts(source, "host"),
        (0, 1),
        "the closure's ifs are not charged to host"
    );
    assert_eq!(
        ts(source, "worker"),
        (2, 2),
        "the headed closure restarts at nest 0"
    );
}

#[test]
fn bound_closure_scores_on_its_own_frame() {
    let source = "function host(n: number) {\n  const f = (m: number) => {\n    if (m) {\n      if (n) { run(m); }\n    }\n  };\n  f(n);\n}\n";
    assert_eq!(
        ts(source, "host"),
        (0, 0),
        "the bound closure's ifs no longer count on the enclosing frame"
    );
    assert_eq!(
        ts(source, "f"),
        (2, 2),
        "the bound closure restarts at nest 0"
    );
}

#[test]
fn r_switch_call_opens_depth_without_scoring() {
    let flat = "g <- function(x) {\n  switch(x, 1, 2)\n}\n";
    assert_eq!(
        metrics(Lang::R, "case.R", flat, "g"),
        (0, 1),
        "bare switch: depth without score"
    );
    let held = "f <- function(x) {\n  if (x) {\n    switch(x, 1, 2)\n  }\n}\n";
    let got = metrics(Lang::R, "case.R", held, "f");
    assert_eq!(got, (0, 2), "if 0 plus switch depth, never doubled");
}

#[test]
fn ternary_alternate_chain_shares_one_level() {
    let chain = "function f(a, b, c, d, e, g) {\n  return a ? 1 : b ? 2 : c ? 3 : d ? 4 : e ? 5 : g ? 6 : 7;\n}\n";
    assert_eq!(
        metrics(Lang::JavaScript, "case.js", chain, "f"),
        (0, 1),
        "six alternate links share the first level"
    );
    assert_eq!(
        ts(chain, "f"),
        (0, 1),
        "the TypeScript hook matches the JavaScript one"
    );
    assert_eq!(
        metrics(Lang::Tsx, "case.tsx", chain, "f"),
        (0, 1),
        "TSX shares the TypeScript hook and field"
    );
    let consequence = "function f(a, b) {\n  return a ? (b ? 1 : 2) : 3;\n}\n";
    assert_eq!(
        metrics(Lang::JavaScript, "case.js", consequence, "f"),
        (0, 2),
        "a ternary in the consequence arm still nests"
    );
    let under_if =
        "function f(a, b, c) {\n  if (a) {\n    return b ? 1 : c ? 2 : 3;\n  }\n  return 0;\n}\n";
    assert_eq!(
        metrics(Lang::JavaScript, "case.js", under_if, "f"),
        (0, 2),
        "if opens one and the chain under it adds one"
    );
    let if_in_arm =
        "function f(a, b) {\n  return a ? (() => { if (b) { return 1; } return 0; })() : 3;\n}\n";
    assert_eq!(
        metrics(Lang::JavaScript, "case.js", if_in_arm, "f"),
        (0, 1),
        "the arm closure is its own function, the chain adds one"
    );
    assert_eq!(
        metrics(Lang::JavaScript, "case.js", if_in_arm, ""),
        (0, 1),
        "an if at nest 0 in the arm closure is unchanged"
    );
}
