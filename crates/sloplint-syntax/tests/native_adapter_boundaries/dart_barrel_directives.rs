//! A Dart file made only of directives that re-export other libraries is a barrel.
//!
//! The grammar wraps every top-level `import` and `export` in an `import_or_export` node,
//! so the barrel check has to look through that wrapper to see the exports.

use crate::support::with_facts;
use sloplint_core::lang::Lang;

/// The re-export target candidates of `source` analyzed as `lib/shapes.dart`, or `None` when
/// the file is not a barrel.
fn targets(source: &str) -> Option<Vec<Vec<String>>> {
    with_facts(Lang::Dart, "lib/shapes.dart", source, |facts| {
        facts.barrel.as_ref().map(|barrel| barrel.targets.clone())
    })
}

#[test]
fn a_file_of_exports_is_a_barrel_with_resolved_targets() {
    let found = targets("export 'b.dart';\n");
    assert_eq!(
        found,
        Some(vec![vec!["lib/b.dart".to_string()]]),
        "a lone export must resolve beside the barrel; observed targets {found:?}"
    );
}

#[test]
fn imports_comments_and_library_names_do_not_break_the_barrel() {
    let source = "// Public surface of the shapes package.
library shapes;

import 'dart:async';
export 'src/circle.dart';
export 'package:shapes/src/square.dart' show Square;
export 'dart:math';
";
    assert_eq!(
        targets(source),
        Some(vec![
            vec!["lib/src/circle.dart".to_string()],
            vec!["lib/src/square.dart".to_string()],
        ]),
        "relative and package exports resolve; `dart:` exports have no file"
    );
}

#[test]
fn a_declaration_or_the_absence_of_exports_means_no_barrel() {
    assert_eq!(
        targets("export 'b.dart';\nclass Circle {}\n"),
        None,
        "a declaration makes the file a real module"
    );
    assert_eq!(
        targets("import 'dart:async';\n"),
        None,
        "imports alone re-export nothing"
    );
}
