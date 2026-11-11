use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn facts_of(source: &str) -> sloplint_syntax::Analysis<'_> {
    let input = FileInput {
        lang: Lang::Go,
        path: "pkg/sample.go",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

#[test]
fn index_receiver_callee_is_member_alone() {
    let analysis = facts_of(
        "package sample\nfunc f(h H, x int) { handlers[pick(x)].Run(h) }\nfunc g(m string) { slog.Info(m) }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let indexed = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "Run")
        .expect("has the indexed call");
    assert!(
        !indexed.is_simple_path,
        "a call through an index is not a simple path"
    );
    assert!(
        indexed.callee_root.is_none(),
        "an indexed chain has no root, got {:?}",
        indexed.callee_root
    );
    assert!(
        analysis.facts.calls.iter().any(|c| c.callee == "slog.Info"),
        "a clean chain keeps its dotted text"
    );
}

#[test]
fn line_broken_chain_keeps_text() {
    let analysis = facts_of("package sample\nfunc f() { s.\nlogger.Info(\"starting\") }\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert!(
        analysis
            .facts
            .calls
            .iter()
            .any(|c| c.callee == "s.\nlogger.Info"),
        "a pure chain keeps its text, got {:?}",
        analysis
            .facts
            .calls
            .iter()
            .map(|c| c.callee)
            .collect::<Vec<_>>()
    );
}
