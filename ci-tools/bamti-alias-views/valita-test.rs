
#[test]
fn isolated_valita_alias_probe() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus = load_corpus(&root).expect("the pinned corpus must parse and validate");
    let case = corpus.case("valita").expect("valita is pinned");
    let expected = NodeOracle::discover(&root)
        .expect("the pinned Node oracle must be available")
        .run_case(case);
    let actual = BamtsRunner::new(&root).run_case(case, ExecutionMode::Jit);
    let mut failures = Vec::new();
    compare_case("valita", ExecutionMode::Jit, &expected, &actual, &mut failures);
    assert!(
        failures.is_empty(),
        "isolated Valita differential failures:\n{}",
        failures.join("\n\n")
    );
}
