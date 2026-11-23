use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;

use crate::support::run_clean;

fn sleep_lines(source: &str) -> Vec<u32> {
    run_clean(Lang::Rust, "src/lib.rs", source)
        .facts
        .blocking_sleeps
        .into_iter()
        .map(|pos| pos.line)
        .collect()
}

#[test]
fn canonical_paths_are_recorded_in_production_and_inline_tests() {
    let source = r#"fn backoff(delay: std::time::Duration) {
    std::thread::sleep(delay);
    (::std::thread::sleep)(delay);
}
#[test]
fn wait_for_index() {
    std::thread::sleep(std::time::Duration::from_millis(1));
}
#[cfg(test)]
mod checks {
    #[test]
    fn wait_for_index() {
        ::std::thread::sleep(::std::time::Duration::from_millis(1));
    }
}
"#;
    let facts = run_clean(Lang::Rust, "src/lib.rs", source).facts;
    assert_eq!(
        facts
            .blocking_sleeps
            .iter()
            .map(|pos| pos.line)
            .collect::<Vec<_>>(),
        [2, 3, 7, 13]
    );
    assert_eq!(facts.role_at(2), CodeRole::Library);
    assert_eq!(facts.role_at(7), CodeRole::Test);
    assert_eq!(facts.role_at(13), CodeRole::Test);
}

#[test]
fn grouped_root_imports_and_raw_aliases_prove_sleep_identity() {
    let source = r#"use std::{thread::{self, sleep as pause}, time::Duration};
use ::std::thread as worker;
use std::thread::sleep as r#yield;
fn wait_for_index(delay: Duration) {
    thread::sleep(delay);
    pause(delay);
    worker::sleep(delay);
    r#yield(delay);
}
"#;
    assert_eq!(sleep_lines(source), [5, 6, 7, 8]);
}

#[test]
fn direct_root_imports_and_parenthesized_calls_are_supported() {
    let source = r#"use std::thread::sleep;
fn wait_for_index(delay: std::time::Duration) {
    (sleep)(delay);
}
"#;
    assert_eq!(sleep_lines(source), [3]);
}

#[test]
fn known_duration_zero_constants_and_constructors_are_exempt() {
    let source = r#"use std::thread::sleep;
use std::time::Duration as Delay;
fn wait_for_index() {
    sleep(std::time::Duration::ZERO);
    sleep(::std::time::Duration::ZERO);
    sleep(Delay::ZERO);
    sleep(Delay::new(0, 0));
    sleep(Delay::from_secs(0_u64));
    sleep(Delay::from_millis(0x00));
    sleep(Delay::from_micros(0o0));
    sleep(Delay::from_nanos(0b00));
    sleep(Delay::from_secs_f32(0.0_f32));
    sleep(Delay::from_secs_f64(0e3));
    sleep((std::time::Duration::from_secs((0))));
    sleep(Delay::from_secs(1));
    sleep(Delay::from_millis(0xf32));
    sleep(Delay::from_nanos(0xf64));
}
"#;
    assert_eq!(sleep_lines(source), [15, 16, 17]);
}

#[test]
fn negative_zero_float_durations_are_exempt_without_exempting_other_negatives() {
    let source = r#"use std::time::Duration;
fn wait_for_index() {
    std::thread::sleep(Duration::from_secs_f32(-0.0_f32));
    std::thread::sleep(Duration::from_secs_f64(-((0.0_f64))));
    std::thread::sleep(Duration::from_secs_f64(-1.0));
}
"#;
    assert_eq!(sleep_lines(source), [5]);
}

#[test]
fn unknown_constants_and_custom_duration_types_need_review() {
    let source = r#"struct Duration;
impl Duration {
    const ZERO: std::time::Duration = std::time::Duration::from_secs(1);
    fn from_secs(_: u64) -> std::time::Duration { Self::ZERO }
}
const DELAY: std::time::Duration = std::time::Duration::ZERO;
fn wait_for_index() {
    std::thread::sleep(Duration::ZERO);
    std::thread::sleep(Duration::from_secs(0));
    std::thread::sleep(DELAY);
}
"#;
    assert_eq!(sleep_lines(source), [8, 9, 10]);
}

#[test]
fn module_and_local_imports_do_not_leak_into_other_scopes() {
    let source = r#"use std::thread as worker;
mod clock {
    fn wait_for_index(delay: std::time::Duration) {
        worker::sleep(delay);
        std::thread::sleep(delay);
    }
}
fn local(delay: std::time::Duration) {
    use std::thread::sleep;
    sleep(delay);
}
"#;
    assert_eq!(sleep_lines(source), [5]);
}

#[test]
fn shadowed_standard_crate_paths_are_not_classified() {
    for source in [
        "mod std { pub mod thread { pub fn sleep(_: u64) {} } } fn wait() { std::thread::sleep(1); }",
        "mod r#std { pub mod thread { pub fn sleep(_: u64) {} } } fn wait() { std::thread::sleep(1); }",
        "extern crate simulated as std; fn wait() { std::thread::sleep(1); }",
        "use simulated as std; fn wait() { std::thread::sleep(1); }",
    ] {
        assert!(sleep_lines(source).is_empty(), "{source}");
    }
}

#[test]
fn the_standard_extern_crate_declaration_preserves_identity() {
    let source =
        "extern crate std; fn wait(delay: std::time::Duration) { std::thread::sleep(delay); }";
    assert_eq!(sleep_lines(source), [1]);
}

#[test]
fn absolute_standard_paths_survive_nested_globs() {
    let source = r#"#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn waits() {
        ::std::thread::sleep(::std::time::Duration::from_millis(1));
        ::std::thread::sleep(::std::time::Duration::ZERO);
        ::std::thread::sleep(::std::time::Duration::from_millis(0));
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
"#;
    let facts = run_clean(Lang::Rust, "src/lib.rs", source).facts;
    assert_eq!(
        facts
            .blocking_sleeps
            .iter()
            .map(|pos| pos.line)
            .collect::<Vec<_>>(),
        [6]
    );
    assert_eq!(facts.role_at(6), CodeRole::Test);
}

#[test]
fn mixed_canonical_paths_survive_unrelated_nested_globs() {
    for unrelated in [
        "mod other { use simulated::*; }",
        "mod other { use simulated::{nested::{*}}; }",
        "fn other() { use simulated::*; }",
    ] {
        for function in ["std::thread::sleep", "::std::thread::sleep"] {
            for duration in ["std::time::Duration", "::std::time::Duration"] {
                let source = format!(
                    "{unrelated}\nfn wait() {{\n    {function}({duration}::ZERO);\n    {function}({duration}::from_millis(0));\n    {function}({duration}::from_millis(1));\n}}\n"
                );
                assert_eq!(sleep_lines(&source), [5], "{source}");
            }
        }
    }
}

#[test]
fn canonical_paths_ignore_globs_in_separate_blocks_and_outer_modules() {
    let source = r#"fn wait() {
    { use simulated::*; }
    ::std::thread::sleep(std::time::Duration::ZERO);
    std::thread::sleep(::std::time::Duration::from_millis(1));
}
mod outer {
    use simulated::*;
    mod inner {
        fn wait() {
            ::std::thread::sleep(std::time::Duration::ZERO);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}
"#;
    assert_eq!(sleep_lines(source), [4, 11]);
}

#[test]
fn enclosing_globs_keep_relative_canonical_paths_ambiguous() {
    let source = r#"mod checks {
    use simulated::{nested::{*}};
    fn wait() {
        ::std::thread::sleep(std::time::Duration::ZERO);
        ::std::thread::sleep(::std::time::Duration::ZERO);
        std::thread::sleep(::std::time::Duration::from_millis(1));
    }
}
fn wait() {
    ::std::thread::sleep(std::time::Duration::ZERO);
    ::std::thread::sleep(::std::time::Duration::ZERO);
    use simulated::*;
    std::thread::sleep(::std::time::Duration::from_millis(1));
}
"#;
    assert_eq!(sleep_lines(source), [4, 10]);
}

#[test]
fn absolute_paths_do_not_resolve_through_local_import_aliases() {
    let source = r#"use std::thread as worker;
use std::thread::sleep as pause;
use std::time::Duration as Delay;
fn wait(delay: std::time::Duration) {
    ::worker::sleep(delay);
    (::pause)(delay);
    ::std::thread::sleep(::Delay::ZERO);
}
"#;
    assert_eq!(sleep_lines(source), [7]);
}

#[test]
fn absolute_standard_paths_keep_visible_root_ambiguity() {
    for prefix in [
        "use simulated::*;",
        "use simulated::{nested::{*}};",
        "extern crate simulated as std;",
        "use simulated as std;",
        "mod std { pub mod thread { pub fn sleep(_: u64) {} } }",
        "mod other { use simulated as std; }",
    ] {
        let source = format!("{prefix} fn wait() {{ ::std::thread::sleep(1); }}");
        assert!(sleep_lines(&source).is_empty(), "{source}");
    }
}

#[test]
fn imported_function_shadowing_is_vetoed_throughout_the_file() {
    for shadow in [
        "fn shadow(sleep: fn(std::time::Duration)) {}",
        "fn shadow() { let sleep = |_| {}; }",
        "fn shadow() { let _closure = |sleep| {}; }",
        "fn shadow() { if let Some(sleep) = None::<u8> {} }",
        "fn shadow() { for sleep in 0..1 {} }",
        "fn shadow() { match Some(1) { Some(sleep) => {}, None => {} } }",
        "mod local { pub fn sleep(_: std::time::Duration) {} }",
    ] {
        let source = format!(
            "use std::thread::sleep; fn wait(delay: std::time::Duration) {{ sleep(delay); }} {shadow}"
        );
        assert!(sleep_lines(&source).is_empty(), "{shadow}");
    }
}

#[test]
fn unrelated_sleep_apis_and_wildcards_are_not_resolved() {
    for source in [
        "fn wait(delay: u64) { worker::sleep(delay); }",
        "fn sleep(_: u64) {} fn wait() { sleep(1); }",
        "fn wait() { tokio::time::sleep(tokio::time::Duration::from_secs(1)); }",
        "fn wait() { async_std::task::sleep(std::time::Duration::from_secs(1)); }",
        "fn wait() { std::thread::sleep_ms(1); }",
        "use simulated::thread; fn wait() { thread::sleep(1); }",
        "use std::thread::*; fn wait() { sleep(std::time::Duration::from_secs(1)); }",
        "use simulated::*; fn wait() { std::thread::sleep(std::time::Duration::from_secs(1)); }",
    ] {
        assert!(sleep_lines(source).is_empty(), "{source}");
    }
}
