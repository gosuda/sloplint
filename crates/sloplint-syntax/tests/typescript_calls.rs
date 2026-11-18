use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

fn analyze_source<'s>(
    parsers: &mut Parsers<'_>,
    custom: &'s CustomRules,
    lang: Lang,
    path: &'s str,
    source: &'s str,
) -> Analysis<'s> {
    let input = FileInput {
        lang,
        path,
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(parsers, &input, custom);
    assert!(
        analysis.parse_error.is_none(),
        "{lang:?} source must parse cleanly"
    );
    analysis
}

fn with_harness(lang: Lang, path: &str, source: &str, check: impl FnOnce(Analysis<'_>)) {
    let mut parsers = Parsers::new();
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_source(&mut parsers, &custom, lang, path, source);
    check(analysis);
}

#[test]
fn generic_tagged_template_reports_plain_callee() {
    let source = "logger.info<string>`hello`;\nlogger.info<string>(\"hi\");\n";
    for lang in [Lang::TypeScript, Lang::Tsx] {
        let path = if lang == Lang::Tsx { "a.tsx" } else { "a.ts" };
        with_harness(lang, path, source, |analysis| {
            assert_eq!(analysis.facts.calls.len(), 2, "{lang:?}");
            assert_eq!(
                analysis.facts.calls[0].callee, "logger.info",
                "{lang:?}: generic tag must unwrap the instantiation"
            );
            assert_eq!(analysis.facts.calls[0].callee_root, Some("logger"));
        });
    }
}
