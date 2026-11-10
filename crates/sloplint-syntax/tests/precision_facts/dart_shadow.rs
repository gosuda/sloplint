use std::sync::LazyLock;

use sloplint_core::facts::{AmbientKind, ArgShape};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Dart,
        path,
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

#[test]
fn repeated_later_prefixes_are_enum_like() {
    let analysis = run(
        "case.dart",
        "void f(Object x) {\nswitch (x) {\ncase A.a:\nbreak;\ncase B.b:\nbreak;\ncase B.c:\nbreak;\ndefault:\nbreak;\n}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(analysis.facts.wildcard_arms.len(), 1, "one wildcard arm");
    let arm = &analysis.facts.wildcard_arms[0];
    assert_eq!(arm.variant_count, 3, "three sibling variants");
    assert!(arm.enum_like, "a repeated later prefix is enum-like");
}

#[test]
fn additive_chains_measure_the_whole_spine() {
    let analysis = run(
        "case.dart",
        "void f(String prefix) {\nlog.info(\"failed to open the cache file \" + prefix + \" now\");\nlog.info(prefix + \" ok\");\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let calls: Vec<_> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "log.info")
        .collect();
    assert_eq!(calls.len(), 2, "both calls are recorded");
    for (index, want) in [(0usize, 30u32), (1, 3)] {
        let ArgShape::Concat { longest_static } = calls[index].args[0].shape else {
            panic!("call {index} is not a concatenation");
        };
        assert_eq!(longest_static, want, "chain static runs are hand-checked");
    }
}

#[test]
fn chained_receiver_calls_use_the_member_name() {
    let analysis = run(
        "case.dart",
        "void f() {\nbuilder(1).configure([1, 2]);\nsvc.render(1);\nlist.map<String>(identity).join(',');\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    for call in &analysis.facts.calls {
        assert!(
            !call.callee.contains('('),
            "callee never carries argument text"
        );
    }
    let configure = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "configure")
        .expect("has the configured call");
    assert!(!configure.is_simple_path, "a call receiver is not a path");
    assert_eq!(configure.callee_root, None, "a call receiver has no root");
    let render = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "svc.render")
        .expect("has the plain call");
    assert!(render.is_simple_path, "a plain chain stays a path");
    let join = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "join")
        .expect("has the join call");
    assert!(
        !join.is_simple_path,
        "a generic call receiver is not a path"
    );
}

#[test]
fn null_aware_calls_are_simple_paths() {
    let analysis = run(
        "case.dart",
        "String greet(String n) => svc?.render(n);\nString greet2(String n) => svc.render(n);\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let null_aware = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "svc?.render")
        .expect("has the null-aware call");
    assert!(null_aware.is_simple_path, "a null-aware chain stays a path");
    assert_eq!(
        null_aware.callee_root,
        Some("svc"),
        "the receiver identifier stays the root"
    );
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "svc.render")
        .expect("has the plain call");
    assert!(plain.is_simple_path, "a plain chain stays a path");
}

#[test]
fn parameter_writes_are_not_global_writes() {
    let analysis = run(
        "case.dart",
        "var count = 0;\nvoid reset(int count) {\ncount = 0;\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let writes: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .collect();
    assert!(writes.is_empty(), "a parameter write is not a global write");
    let mutables: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalMutable)
        .collect();
    assert_eq!(mutables.len(), 1, "the top-level var stays a mutable");
}

#[test]
fn closure_params_shadow_globals() {
    let analysis = run(
        "case.dart",
        "var count = 0;\nvoid setup() {\nvar f = (int count) {\ncount = 0;\n};\nf(1);\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let writes: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .collect();
    assert!(
        writes.is_empty(),
        "a closure parameter write is not a global write"
    );
    let exposed = run(
        "case.dart",
        "var count = 0;\nvoid setup() {\nvar f = (int other) {\ncount = 0;\n};\nf(1);\n}\n",
    );
    assert!(exposed.parse_error.is_none(), "parses");
    let exposed_writes: Vec<_> = exposed
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .collect();
    assert_eq!(
        exposed_writes.len(),
        1,
        "a real global write still fires with no same-named parameter"
    );
}

#[test]
fn every_top_level_declarator_is_a_mutable() {
    let analysis = run(
        "case.dart",
        "var hits = 0, misses = 0;\nvoid record(bool ok) {\nif (ok) { hits++; } else { misses++; }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let mut names: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalMutable)
        .map(|a| a.name)
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["hits", "misses"],
        "both declarators are mutables"
    );
    let writes: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .collect();
    assert_eq!(writes.len(), 2, "writes to declared globals still fire");
}

#[test]
fn later_local_declarators_shadow_globals() {
    let analysis = run(
        "case.dart",
        "var b = 0;\nvoid f() {\nvar a = 1, b = 2;\nb = 3;\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let writes: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .collect();
    assert!(
        writes.is_empty(),
        "a later local declarator shadows the global"
    );
    let exposed = run(
        "case.dart",
        "var b = 0;\nvoid f() {\nvar a = 1;\nb = 3;\n}\n",
    );
    assert!(exposed.parse_error.is_none(), "parses");
    let exposed_writes: Vec<_> = exposed
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .collect();
    assert_eq!(
        exposed_writes.len(),
        1,
        "a real global write still fires with no same-named local"
    );
}
