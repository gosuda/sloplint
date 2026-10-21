//! A tagged template call carries its template as one argument, so the literal run in front
//! of the first substitution is visible to the log-message rule. TypeScript matches JavaScript.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn shapes(lang: Lang, path: &str) -> Vec<(String, Vec<String>)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang,
        path,
        source: SOURCE,
        is_test_file: false,
        has_interface: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "{path} parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .calls
        .iter()
        .map(|c| {
            let args = c.args.iter().map(|a| format!("{:?}", a.shape)).collect();
            (c.callee.to_string(), args)
        })
        .collect()
}

const SOURCE: &str = "logger.info`starting ${name}`;
logger.info(\"starting \" + name);
logger.info(`starting ${name}`, name);
";

fn expected() -> Vec<(String, Vec<String>)> {
    let template = "Str { longest_static: 9 }".to_string();
    vec![
        ("logger.info".to_string(), vec![template.clone()]),
        ("logger.info".to_string(), vec!["Concat".to_string()]),
        (
            "logger.info".to_string(),
            vec![template, "Ident(\"name\")".to_string()],
        ),
    ]
}

#[test]
fn typescript_tagged_template_is_one_string_argument() {
    assert_eq!(shapes(Lang::TypeScript, "log.ts"), expected());
}

#[test]
fn javascript_tagged_template_is_one_string_argument() {
    assert_eq!(shapes(Lang::JavaScript, "log.js"), expected());
}
