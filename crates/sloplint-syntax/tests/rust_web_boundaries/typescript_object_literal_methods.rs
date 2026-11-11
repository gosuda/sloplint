//! Only methods declared directly in a class body inherit that class's visibility and
//! interface binding. A method of an object literal returned from a class method is neither
//! public nor interface-bound, which is how the JavaScript adapter already reads it.

use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

type Row = (String, Visibility, bool);

fn methods(lang: Lang, path: &str, source: &str) -> Vec<Row> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang,
        path,
        source,
        role: CodeRole::Library,
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
        .map(|f| (f.name.to_string(), f.visibility, f.signature_forced))
        .collect()
}

fn row(name: &str, visibility: Visibility, forced: bool) -> Row {
    (name.to_string(), visibility, forced)
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
fn object_literal_methods_stay_private_without_forced_signatures() {
    assert_eq!(
        methods(Lang::TypeScript, "factory.ts", TYPESCRIPT),
        vec![
            row("make", Visibility::Exported, false),
            row("run", Visibility::Private, false),
            row("stop", Visibility::Exported, false),
            row("keep", Visibility::Private, false),
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
        vec![
            row("make", Visibility::Exported, false),
            row("run", Visibility::Private, false)
        ],
        "JavaScript never forces a signature and never exports literal methods"
    );
}
