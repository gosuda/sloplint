//! Else-if chains cost one per arm and open no nest, observed through `analyze` only.
//!
//! Go `else if`, Java `else if`, and Bash `elif` share the walker rule with
//! Rust and Python: the chained arm adds one to cognitive complexity without
//! opening a nesting level. Each language pairs a flat chain with a nested
//! control that shows the chained arm still costs one inside a nest.

use sloplint_core::lang::Lang;

use crate::support::run_clean;

/// `(cognitive, max_depth)` of the function called `name` in `source`.
fn metrics(lang: Lang, path: &str, source: &str, name: &str) -> (u32, u32) {
    let analysis = run_clean(lang, path, source);
    let function = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{path} has function {name}"));
    (function.cognitive, function.max_depth)
}

const GO_CHAIN: &str = "package probe\nfunc Chain(n int) string {\n\tif n == 0 {\n\t\treturn \"zero\"\n\t} else if n == 1 {\n\t\treturn \"one\"\n\t} else if n == 2 {\n\t\treturn \"two\"\n\t}\n\treturn \"other\"\n}\n";

const GO_INNER: &str = "package probe\nfunc Inner(a bool, b bool, c bool) string {\n\tif a {\n\t\tif b {\n\t\t} else if c {\n\t\t}\n\t}\n\treturn \"done\"\n}\n";

const JAVA_CHAIN: &str = "public class Chain {\n\tpublic String chain(int n) {\n\t\tif (n == 0) {\n\t\t\treturn \"zero\";\n\t\t} else if (n == 1) {\n\t\t\treturn \"one\";\n\t\t} else if (n == 2) {\n\t\t\treturn \"two\";\n\t\t}\n\t\treturn \"other\";\n\t}\n}\n";

const JAVA_INNER: &str = "public class Chain {\n\tpublic String inner(boolean a, boolean b, boolean c) {\n\t\tif (a) {\n\t\t\tif (b) {\n\t\t\t} else if (c) {\n\t\t\t}\n\t\t}\n\t\treturn \"done\";\n\t}\n}\n";

const BASH_CHAIN: &str = "classify() {\n\tif [ \"$1\" = a ]; then\n\t\techo a\n\telif [ \"$1\" = b ]; then\n\t\techo b\n\telif [ \"$1\" = c ]; then\n\t\techo c\n\telse\n\t\techo other\n\tfi\n}\n";

const BASH_INNER: &str = "outer() {\n\tif true; then\n\t\tif true; then\n\t\t\techo a\n\t\telif true; then\n\t\t\techo b\n\t\tfi\n\tfi\n}\n";

#[test]
fn go_else_if_chain_costs_one_per_arm() {
    assert_eq!(
        metrics(Lang::Go, "chain.go", GO_CHAIN, "Chain"),
        (3, 1),
        "if 1 plus two else-if arms at 1"
    );
    assert_eq!(
        metrics(Lang::Go, "chain.go", GO_INNER, "Inner"),
        (4, 2),
        "outer if 1 plus nested if 2 plus else-if 1"
    );
}

#[test]
fn java_else_if_chain_costs_one_per_arm() {
    assert_eq!(
        metrics(Lang::Java, "Chain.java", JAVA_CHAIN, "chain"),
        (3, 1),
        "if 1 plus two else-if arms at 1"
    );
    assert_eq!(
        metrics(Lang::Java, "Chain.java", JAVA_INNER, "inner"),
        (4, 2),
        "outer if 1 plus nested if 2 plus else-if 1"
    );
}

#[test]
fn bash_elif_chain_costs_one_per_arm() {
    assert_eq!(
        metrics(Lang::Bash, "chain.sh", BASH_CHAIN, "classify"),
        (3, 1),
        "if 1 plus two elif arms at 1"
    );
    assert_eq!(
        metrics(Lang::Bash, "chain.sh", BASH_INNER, "outer"),
        (4, 2),
        "outer if 1 plus nested if 2 plus elif 1"
    );
}
