//! Only methods declared directly in a class body inherit that class's visibility and
//! interface binding. A method of an object literal returned from a class method is neither
//! public nor interface-bound, which is how the JavaScript adapter already reads it.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

type Row = (String, bool, bool);

fn methods(lang: Lang, path: &str, source: &str) -> Vec<Row> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang,
        path,
        source,
        is_test_file: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "{path} parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .functions
        .iter()
        .map(|f| (f.name.to_string(), f.public, f.interface_bound))
        .collect()
}

fn row(name: &str, public: bool, interface_bound: bool) -> Row {
    (name.to_string(), public, interface_bound)
}

const TYPESCRIPT: &str = "export class Factory implements Maker {
  make() {
    return {
      run(input: string, strict: boolean) {
        return check(input, strict);
      },
    };
  }
}

export abstract class Base implements Port {
  stop() {}
}

export const Anonymous = class {
  keep() {}
};
";

#[test]
fn object_literal_methods_do_not_inherit_the_enclosing_class() {
    assert_eq!(
        methods(Lang::TypeScript, "factory.ts", TYPESCRIPT),
        vec![
            row("make", true, true),
            row("run", false, false),
            row("stop", true, true),
            row("keep", false, false),
        ],
        "class members keep their class; the literal's `run` and the class expression's `keep` do not"
    );
}

#[test]
fn javascript_reads_the_same_shapes_the_same_way() {
    let source = "export class Factory {
  make() {
    return {
      run(input) {
        return input;
      },
    };
  }
}
";
    assert_eq!(
        methods(Lang::JavaScript, "factory.js", source),
        vec![row("make", true, false), row("run", false, false)],
        "JavaScript never binds to an interface and never exports literal methods"
    );
}
