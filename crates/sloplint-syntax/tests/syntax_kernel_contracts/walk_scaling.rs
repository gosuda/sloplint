//! Walk exit, Ruby method visibility, and C++ member access stay linear in file size.
//!
//! Each case analyzes a generated file on a thread with the default 2 MiB stack and
//! waits at most `CAP` for the result. The quadratic forms take tens of seconds on
//! these inputs, while the linear forms finish well inside the cap in debug builds,
//! so the margin holds on a loaded host. A stack overflow aborts the test binary and
//! a hang fails at the cap, so neither can stall the harness.

use std::sync::mpsc::{RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use sloplint_core::lang::Lang;
use sloplint_syntax::Analysis;

use crate::support::run;

/// The wall-clock bound each generated input must meet.
const CAP: Duration = Duration::from_secs(10);

/// Runs `read` over the analysis of `source` on a 2 MiB thread, failing past `CAP`.
fn analyze_within<T: Send + 'static>(
    label: &str,
    lang: Lang,
    path: &'static str,
    source: String,
    read: fn(&Analysis<'_>) -> T,
) -> T {
    let (sender, receiver) = channel();
    thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            let analysis = run(lang, path, &source);
            let clean = analysis.parse_error.is_none();
            assert!(clean, "{path} parses cleanly");
            let _ = sender.send(read(&analysis));
        })
        .expect("spawn analysis thread");
    match receiver.recv_timeout(CAP) {
        Ok(value) => value,
        Err(RecvTimeoutError::Timeout) => panic!("{label} did not finish within {CAP:?}"),
        Err(RecvTimeoutError::Disconnected) => panic!("{label} analysis thread failed"),
    }
}

/// C with `depth` nested `if` blocks around one `return`, inside one function.
fn nested_ifs(depth: usize) -> String {
    let mut source = String::from("int deep(int x) {\n");
    for _ in 0..depth {
        source.push_str("if (x) {\n");
    }
    source.push_str("return x;\n");
    for _ in 0..depth {
        source.push_str("}\n");
    }
    source.push_str("return 0;\n}\n");
    source
}

/// JavaScript with `depth` nested arrow closures, each returning the next.
fn nested_closures(depth: usize) -> String {
    let mut source = String::from("function top() {\nlet f = ");
    for _ in 0..depth {
        source.push_str("() => ");
    }
    source.push_str("0;\nreturn f;\n}\n");
    source
}

/// Ruby with `count` methods, the second half after a `private` marker.
fn ruby_methods(count: usize) -> String {
    let mut source = String::from("class Gate\n");
    for index in 0..count {
        if index == count / 2 {
            source.push_str("  private\n\n");
        }
        source.push_str(&format!("  def m{index}\n  end\n\n"));
    }
    source.push_str("end\n");
    source
}

/// C++ with `count` members, the second half after a `private:` label.
fn cpp_members(count: usize) -> String {
    let mut source = String::from("class Gate {\npublic:\n");
    for index in 0..count {
        if index == count / 2 {
            source.push_str("private:\n");
        }
        source.push_str(&format!("    void m{index}(bool a) {{ (void)a; }}\n"));
    }
    source.push_str("};\n");
    source
}

/// `(deepest nesting, function count)` of the first function in the analysis.
fn deepest_and_count(analysis: &Analysis<'_>) -> (u32, usize) {
    let max_depth = analysis
        .facts
        .functions
        .iter()
        .map(|f| f.max_depth)
        .max()
        .unwrap_or(0);
    (max_depth, analysis.facts.functions.len())
}

/// `(function count, public count)` over the analysis.
fn counts_by_publicity(analysis: &Analysis<'_>) -> (usize, usize) {
    let total = analysis.facts.functions.len();
    let public = analysis.facts.functions.iter().filter(|f| f.public).count();
    (total, public)
}

#[test]
fn nested_control_flow_exit_stays_linear_in_depth() {
    let depth = 8000;
    let (max_depth, functions) = analyze_within(
        "8000 nested C ifs",
        Lang::C,
        "case.c",
        nested_ifs(depth),
        deepest_and_count,
    );
    assert_eq!(functions, 1, "one function");
    assert_eq!(
        max_depth,
        u32::try_from(depth).expect("depth fits u32"),
        "every nested if opens one nesting level"
    );
}

#[test]
fn nested_closure_exit_stays_linear_in_depth() {
    let depth = 16000;
    let (max_depth, functions) = analyze_within(
        "16000 nested JavaScript closures",
        Lang::JavaScript,
        "case.js",
        nested_closures(depth),
        deepest_and_count,
    );
    assert_eq!(functions, 1, "one function, `top`");
    assert_eq!(
        max_depth,
        u32::try_from(depth).expect("depth fits u32"),
        "every nested closure opens one nesting level"
    );
}

#[test]
fn ruby_method_visibility_stays_linear_in_methods() {
    let count = 4000;
    let (total, public) = analyze_within(
        "4000 Ruby methods",
        Lang::Ruby,
        "case.rb",
        ruby_methods(count),
        counts_by_publicity,
    );
    assert_eq!(total, count, "every method is a function fact");
    assert_eq!(public, count / 2, "only the first half precedes `private`");
}

#[test]
fn cpp_member_access_stays_linear_in_members() {
    let count = 4000;
    let (total, public) = analyze_within(
        "4000 C++ members",
        Lang::Cpp,
        "case.cpp",
        cpp_members(count),
        counts_by_publicity,
    );
    assert_eq!(total, count, "every member is a function fact");
    assert_eq!(public, count / 2, "only the first half follows `public:`");
}
