//! Names bound through object-destructuring defaults (`let { timeout = 5000 } = opts`)
//! are declared like any other destructured name, in TypeScript and JavaScript alike.

use sloplint_core::facts::AmbientKind;
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = "const opts = { timeout: 1 };
let { timeout = 5000, retries: tries = 3 } = opts;
var [{ depth = 2 }] = [opts];
export function wait() {
  let { limit = 1 } = opts;
  limit = 2;
  return limit + timeout + tries + depth;
}
";

fn declared(lang: Lang, path: &str) -> (Vec<String>, Vec<String>) {
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
    let mut globals: Vec<String> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalMutable)
        .map(|a| a.name.to_string())
        .collect();
    globals.sort();
    let locals = analysis
        .facts
        .bindings
        .iter()
        .map(|b| b.name.to_string())
        .collect();
    (globals, locals)
}

fn expected() -> (Vec<String>, Vec<String>) {
    let globals = ["depth", "timeout", "tries"].map(str::to_string).to_vec();
    (globals, vec!["limit".to_string()])
}

#[test]
fn typescript_declares_names_bound_with_defaults() {
    assert_eq!(declared(Lang::TypeScript, "mod.ts"), expected());
}

#[test]
fn javascript_declares_names_bound_with_defaults() {
    assert_eq!(declared(Lang::JavaScript, "mod.js"), expected());
}
