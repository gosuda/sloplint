//! Shared setup: analyze one deep source through the public API on a default-sized thread stack.

use std::time::{Duration, Instant};

use sloplint_core::facts::{FileFacts, Function, MutBinding};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

/// Nesting depth for every case: deep enough to overflow a 2 MiB stack under recursion.
pub const DEPTH: usize = 20_000;

/// Wall-clock cap for a full-depth analysis when the quarter-depth sample is too fast to scale.
///
/// Linear analysis takes well under a second in a debug build. A pass that rescans the nodes
/// below every node costs minutes at this depth, so the cap still separates the two shapes with
/// a wide margin on a slow machine.
pub const LINEAR_BUDGET: Duration = Duration::from_secs(30);

/// Scaling bound: time at [`DEPTH`] must stay within this multiple of time at `DEPTH / 4`.
///
/// Linear work grows about 4x across that step and quadratic work about 16x, so 10 separates
/// the two shapes with margin on either side while load swings shared by both samples cancel.
pub const LINEAR_RATIO: u32 = 10;

/// Minimum quarter-depth time worth scaling: below this timer noise dominates the ratio,
/// so the check falls back to [`LINEAR_BUDGET`].
pub const MIN_SAMPLE: Duration = Duration::from_millis(5);

/// Samples per depth: each depth runs this many times and keeps the fastest time.
const SAMPLES: usize = 3;

/// The macOS Intel runner measured 10.4x to 11.3x on linear analysis, so there the ratio cannot
/// separate linear from quadratic work and only [`LINEAR_BUDGET`] applies.
const RATIO_SEPARATES: bool = !cfg!(all(target_os = "macos", target_arch = "x86_64"));

/// Requires linear scaling: runs `check` at `DEPTH / 4` and at [`DEPTH`], keeps the fastest
/// time at each depth, and requires the full depth to cost at most [`LINEAR_RATIO`] times the
/// quarter depth. When the quarter depth is faster than [`MIN_SAMPLE`], the ratio is noise, so
/// the full depth must instead finish within [`LINEAR_BUDGET`].
pub fn within_linear_budget<T>(mut check: impl FnMut(usize) -> T) -> T {
    let mut small = Duration::MAX;
    for _ in 0..SAMPLES {
        let started = Instant::now();
        check(DEPTH / 4);
        small = small.min(started.elapsed());
    }
    let started = Instant::now();
    let mut value = check(DEPTH);
    let mut full = started.elapsed();
    for _ in 1..SAMPLES {
        let started = Instant::now();
        value = check(DEPTH);
        full = full.min(started.elapsed());
    }
    if small < MIN_SAMPLE || !RATIO_SEPARATES {
        assert!(
            full < LINEAR_BUDGET,
            "analysis took {full:?} at depth {DEPTH}, so it is no longer linear in depth"
        );
    } else {
        assert!(
            full <= small * LINEAR_RATIO,
            "analysis took {full:?} at depth {DEPTH} versus {small:?} at depth {}, above {LINEAR_RATIO}x, so it is no longer linear in depth",
            DEPTH / 4
        );
    }
    value
}

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
        role: CodeRole::Library,
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
