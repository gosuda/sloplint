//! Kotlin test markers: one child module per mechanism, so each fix lands with
//! its own case module while the crate links a single integration-test executable.
//!
//! A function opens a Test region when its own annotation list names a test
//! annotation; a `Benchmark` annotation opens a Bench region instead.

use crate::support::with_facts;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;

const ATTRIBUTED: &str = "class Widget {\n    @Test\n    fun flag() { }\n    @ParameterizedTest\n    fun check() { }\n    @RepeatedTest\n    fun repeated() { }\n    @TestFactory\n    fun factory() { }\n    @TestTemplate\n    fun templated() { }\n    @org.junit.jupiter.api.Test\n    fun qualified() { }\n    @Test(timeout = 1000)\n    fun withArgs() { }\n    @Deprecated(\"use flag\")\n    fun deprecated_entry() { }\n    fun unannotated() { }\n}\n";

const BENCHED: &str = "class Widget {\n    @Benchmark\n    fun speed() { }\n}\n";

fn flags(source: &str) -> Vec<(String, bool)> {
    with_facts(Lang::Kotlin, "case.kt", source, |facts| {
        facts
            .functions
            .iter()
            .map(|f| {
                let marked = facts.regions.iter().any(|region| {
                    region.role == CodeRole::Test
                        && region.start_line <= f.pos.line
                        && f.pos.line <= region.end_line
                });
                (f.name.to_owned(), marked)
            })
            .collect()
    })
}

#[test]
fn every_test_annotation_marks_only_its_function() {
    assert_eq!(
        flags(ATTRIBUTED),
        [
            ("flag", true),
            ("check", true),
            ("repeated", true),
            ("factory", true),
            ("templated", true),
            ("qualified", true),
            ("withArgs", true),
            ("deprecated_entry", false),
            ("unannotated", false),
        ]
        .into_iter()
        .map(|(name, marked)| (name.to_owned(), marked))
        .collect::<Vec<_>>(),
    );
}

#[test]
fn benchmark_annotation_opens_a_bench_region() {
    with_facts(Lang::Kotlin, "case.kt", BENCHED, |facts| {
        assert!(
            facts
                .regions
                .iter()
                .any(|region| region.role == CodeRole::Bench),
            "a @Benchmark method opens a Bench region, got {:?}",
            facts.regions
        );
    });
}
