//! Shared setup: analyze one deep source through the public API on a default-sized thread stack.

use sloplint_core::facts::{FileFacts, Function, MutBinding};
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

/// Nesting depth for every case: deep enough to overflow a 2 MiB stack under recursion.
pub const DEPTH: usize = 20_000;

/// Runs `check` on a thread with the default 2 MiB stack, so an overflowing helper aborts the test
/// binary (a failure) instead of hanging the harness.
pub fn on_default_stack<T: Send + 'static>(check: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(check)
        .expect("spawn default-stack thread")
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// Analyzes `source` as `lang` at `path`, requires a clean parse, and reads owned values from its facts.
pub fn with_facts<R>(
    lang: Lang,
    path: &str,
    source: &str,
    read: impl FnOnce(&FileFacts<'_>) -> R,
) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang,
        path,
        source,
        is_test_file: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "{path} parses cleanly");
    read(&analysis.facts)
}

/// The first function named `name`; panics when the source has none.
pub fn function<'a, 's>(facts: &'a FileFacts<'s>, name: &str) -> &'a Function<'s> {
    facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("a function named {name}"))
}

/// The first binding named `name`; panics when the source declares none.
pub fn binding<'a, 's>(facts: &'a FileFacts<'s>, name: &str) -> &'a MutBinding<'s> {
    facts
        .bindings
        .iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("a binding named {name}"))
}

/// Names of every binding in declaration order.
pub fn binding_names(facts: &FileFacts<'_>) -> Vec<String> {
    facts.bindings.iter().map(|b| b.name.to_owned()).collect()
}

/// A tuple pattern nested `depth` levels whose leaves are `v0` through `v{depth}`, in order.
pub fn nested_tuple(depth: usize) -> String {
    let mut out = String::new();
    for level in 0..depth {
        out.push_str(&format!("(v{level}, "));
    }
    out.push_str(&format!("v{depth}"));
    out.push_str(&")".repeat(depth));
    out
}

/// The leaf names `nested_tuple(depth)` declares, in order.
pub fn tuple_leaves(depth: usize) -> Vec<String> {
    (0..=depth).map(|level| format!("v{level}")).collect()
}
