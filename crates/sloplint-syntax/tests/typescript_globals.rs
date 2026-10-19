use sloplint_core::facts::AmbientKind;
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn globals(lang: Lang, source: &str) -> (Vec<String>, bool, bool) {
    let mut parsers = Parsers::new();
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang,
        path: "a.ts",
        source,
        is_test_file: false,
        has_interface: false,
    };
    let analysis = analyze(&mut parsers, &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "wave-self-lint: {lang:?} globals source must parse cleanly"
    );
    let names = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalMutable)
        .map(|a| a.name.to_string())
        .collect();
    let env = analysis
        .facts
        .ambient
        .iter()
        .any(|a| a.kind == AmbientKind::EnvRead);
    let const_global = analysis
        .facts
        .ambient
        .iter()
        .any(|a| a.kind == AmbientKind::GlobalMutable && a.name == "app");
    (names, env, const_global)
}

const SOURCE: &str = "let top_let = 1;\nvar top_var = 2;\nexport let exported_let = 3;\nconst app = express();\napp.get('/', (req, res) => {\n  let hits = 0;\n  res.send(String(++hits));\n});\nfor (let i = 0; i < 3; i++) {\n  console.log(i);\n}\n{\n  let block_let = 0;\n}\nconst mode = process.env.MODE;\n";

#[test]
fn typescript_locals_are_not_globals() {
    let (names, env, const_global) = globals(Lang::TypeScript, SOURCE);
    for keep in ["top_let", "top_var", "exported_let"] {
        assert!(
            names.contains(&keep.to_string()),
            "missing {keep}: {names:?}"
        );
    }
    for local in ["hits", "i", "block_let"] {
        assert!(
            !names.contains(&local.to_string()),
            "leaked {local}: {names:?}"
        );
    }
    assert!(!const_global, "const leaked: {names:?}");
    assert!(env, "process.env read lost: {names:?}");
}

#[test]
fn javascript_matches_typescript_scoping() {
    let (ts, _, _) = globals(Lang::TypeScript, SOURCE);
    let (js, _, _) = globals(Lang::JavaScript, SOURCE);
    assert_eq!(
        ts, js,
        "wave-self-lint: JavaScript scoping diverged from TypeScript: ts={ts:?} js={js:?}"
    );
    assert!(
        !js.contains(&"hits".to_string()),
        "wave-self-lint: block-local `hits` leaked to globals: {js:?}"
    );
}
