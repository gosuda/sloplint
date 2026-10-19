//! Nesting-weighted cognitive complexity, observed through `analyze` only.
//!
//! Every expectation follows one formula. An `if`, loop, match, ternary, or
//! catch costs `1 + nest` and then opens a nest level; `else if` costs `1` and
//! opens none; a `with` block opens a level and costs nothing; a nested function
//! restarts at nest `0`.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

/// `(cognitive, max_depth)` of the function called `name` in `source`.
fn metrics(lang: Lang, path: &str, source: &str, name: &str) -> (u32, u32) {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang,
        path,
        source,
        is_test_file: false,
        has_interface: false,
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
fn nested_controls_cost_more_than_flat_ones() {
    let flat = "fn flat(n: u8) {\n    if n > 0 {}\n    if n > 1 {}\n    if n > 2 {}\n}\n";
    assert_eq!(rust(flat, "flat"), (3, 1), "three siblings: 1 + 1 + 1");
    let nested = "fn nested(n: u8) {\n    if n > 0 {\n        if n > 1 {\n            if n > 2 {}\n        }\n    }\n}\n";
    assert_eq!(rust(nested, "nested"), (6, 3), "1 + 2 + 3");
    let after =
        "fn after(n: u8) {\n    if n > 0 {\n        if n > 1 {}\n    }\n    if n > 2 {}\n}\n";
    assert_eq!(rust(after, "after"), (4, 2), "nest closes: 1 + 2, then 1");
}

#[test]
fn loops_matches_ternaries_and_ifs_share_one_nest_counter() {
    let mixed = "fn mixed(xs: &[u32], k: u32) {\n    for x in xs {\n        if *x > k {\n            match x {\n                _ => {}\n            }\n        }\n    }\n}\n";
    assert_eq!(rust(mixed, "mixed"), (6, 3), "for 1 + if 2 + match 3");
    let spin = "fn spin() {\n    loop {\n        while true {\n            for _ in 0..3 {}\n        }\n    }\n}\n";
    assert_eq!(rust(spin, "spin"), (6, 3), "loop 1 + while 2 + for 3");
    let ternary = "function f(a: boolean, b: boolean) {\n  return a ? (b ? 1 : 2) : 3;\n}\n";
    assert_eq!(
        ts(ternary, "f"),
        (3, 2),
        "outer ternary 1 + inner ternary 2"
    );
    let switch = "function h(xs: number[]) {\n  for (const x of xs) {\n    switch (x) {\n      case 1: break;\n    }\n  }\n}\n";
    assert_eq!(ts(switch, "h"), (3, 2), "for 1 + switch 2");
}

#[test]
fn else_if_adds_one_without_opening_a_nest() {
    let chain = "fn chain(n: u8) {\n    if n > 0 {} else if n > 1 {} else if n > 2 {} else {}\n}\n";
    assert_eq!(rust(chain, "chain"), (3, 1), "if 1 + two else-if at 1");
    let inner =
        "fn inner(n: u8) {\n    if n > 0 {\n        if n > 1 {} else if n > 2 {}\n    }\n}\n";
    assert_eq!(
        rust(inner, "inner"),
        (4, 2),
        "if 1 + nested if 2 + else-if 1"
    );
    let elif =
        "def f(a, b, c):\n    if a:\n        pass\n    elif b:\n        if c:\n            pass\n";
    assert_eq!(py(elif, "f"), (4, 2), "if 1 + elif 1 + if inside elif 2");
    let ladder = "def g(a, b, c):\n    if a:\n        pass\n    elif b:\n        pass\n    elif c:\n        pass\n    else:\n        pass\n";
    assert_eq!(py(ladder, "g"), (3, 1), "if 1 + two elif at 1");
}

#[test]
fn catch_pays_its_nest_and_opens_one() {
    let ts_catch = "function g(a: boolean) {\n  try {\n    if (a) { run(); }\n  } catch (e) {\n    if (a) { recover(); }\n  }\n}\n";
    assert_eq!(ts(ts_catch, "g"), (4, 2), "if 1 + catch 1 + if in catch 2");
    let py_except = "def e(a):\n    try:\n        pass\n    except ValueError:\n        if a:\n            pass\n";
    assert_eq!(py(py_except, "e"), (3, 2), "except 1 + if in except 2");
}

#[test]
fn with_blocks_open_a_nest_and_cost_nothing() {
    let alone = "def f(p):\n    with open(p) as h:\n        pass\n";
    assert_eq!(
        py(alone, "f"),
        (0, 1),
        "a with scores 0 and opens one level"
    );
    let held = "def g(a, p):\n    with open(p) as h:\n        if a:\n            pass\n";
    assert_eq!(py(held, "g"), (2, 2), "if inside with: 1 + 1");
    let twice = "def k(a, p, q):\n    with open(p) as h:\n        with open(q) as i:\n            if a:\n                pass\n";
    assert_eq!(
        py(twice, "k"),
        (3, 3),
        "two withs cost 0, if at nest 2 costs 3"
    );
}

#[test]
fn function_scopes_restart_nesting_while_closures_carry_it() {
    let defs = "def outer(a, b):\n    if a:\n        def inner(c):\n            if c:\n                pass\n        if b:\n            pass\n";
    assert_eq!(
        py(defs, "outer"),
        (3, 2),
        "inner's if is not charged to outer"
    );
    assert_eq!(py(defs, "inner"), (1, 1), "inner restarts at nest 0");
    let closure =
        "fn host(n: u8) {\n    let f = |m: u8| {\n        if m > 0 {}\n    };\n    f(n);\n}\n";
    assert_eq!(
        rust(closure, "host"),
        (2, 2),
        "closure opens a level for free"
    );
}

#[test]
fn r_switch_call_is_scored_once_at_its_nest() {
    let flat = "g <- function(x) {\n  switch(x, 1, 2)\n}\n";
    assert_eq!(
        metrics(Lang::R, "case.R", flat, "g"),
        (1, 1),
        "bare switch: 1"
    );
    let held = "f <- function(x) {\n  if (x) {\n    switch(x, 1, 2)\n  }\n}\n";
    let got = metrics(Lang::R, "case.R", held, "f");
    assert_eq!(got, (3, 2), "if 1 + switch inside if 2, never doubled");
}
