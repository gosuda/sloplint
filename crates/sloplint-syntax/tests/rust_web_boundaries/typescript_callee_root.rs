//! TypeScript call sites record the leftmost name of their callee chain, exactly as
//! JavaScript does, so delegation through `this` or `super` is distinguishable from a
//! free-function wrapper and the passthrough-wrapper rule can leave receiver forwarding alone.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

type Row = (String, Option<String>);

fn roots(lang: Lang, path: &str) -> Vec<Row> {
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
        .map(|c| (c.callee.to_string(), c.callee_root.map(str::to_string)))
        .collect()
}

fn row(callee: &str, root: Option<&str>) -> Row {
    (callee.to_string(), root.map(str::to_string))
}

const SOURCE: &str = "class Cache {
  get(key) {
    this.store.get(key);
    (this.store).get(key);
    super.get(key);
    helper(key);
    a.b[c].d(key);
    new Store(key);
    return null;
  }
}
";

fn expected() -> Vec<Row> {
    vec![
        row("this.store.get", Some("this")),
        row("(this.store).get", Some("this")),
        row("super.get", Some("super")),
        row("helper", Some("helper")),
        row("a.b[c].d", Some("a")),
        row("Store", None),
    ]
}

#[test]
fn typescript_calls_record_their_callee_root() {
    assert_eq!(roots(Lang::TypeScript, "cache.ts"), expected());
}

#[test]
fn javascript_records_the_same_roots() {
    assert_eq!(roots(Lang::JavaScript, "cache.js"), expected());
}
