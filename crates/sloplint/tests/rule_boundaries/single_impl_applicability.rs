//! `slop/single-impl-abstraction` reads only languages with an interface and implements relation.

use sloplint::{RunOptions, execute};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

const RULE: &str = "slop/single-impl-abstraction";

const RUBY_MIXIN: [(&str, &str); 2] = [
    (
        "greet.rb",
        "module Greet\n  def hi\n    \"hi\"\n  end\nend\n",
    ),
    ("person.rb", "class Person\n  include Greet\nend\n"),
];
const PHP_INTERFACE: [(&str, &str); 2] = [
    (
        "Shape.php",
        "<?php\ninterface Shape {\n    public function area(): float;\n}\n",
    ),
    (
        "Circle.php",
        "<?php\nclass Circle implements Shape {\n    public function area(): float { return 1.0; }\n}\n",
    ),
];
const JAVA_SHAPE: [(&str, &str); 2] = [
    (
        "src/Shape.java",
        "public interface Shape {\n    double area();\n}\n",
    ),
    (
        "src/Circle.java",
        "public class Circle implements Shape {\n    @Override\n    public double area() {\n        return 1.0;\n    }\n}\n",
    ),
];

/// `(path, line, message)` of every finding for a project made of `files`.
fn reported(files: &[(&str, &str)]) -> Vec<(String, u32, String)> {
    let tmp = tempfile::tempdir().unwrap();
    for (rel, contents) in files {
        let path = tmp.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    let execution = execute(&RunOptions {
        cwd: tmp.path().to_path_buf(),
        root: Some(tmp.path().to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![RulePattern::parse(RULE).unwrap()]),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    })
    .unwrap();
    execution
        .report
        .findings
        .iter()
        .inspect(|finding| assert_eq!(&*finding.rule, RULE))
        .map(|finding| {
            (
                finding.path.to_string(),
                finding.line,
                finding.message.clone(),
            )
        })
        .collect()
}

#[test]
fn ruby_mixin_with_one_includer_is_not_reported() {
    let found = reported(&RUBY_MIXIN);
    assert!(
        found.is_empty(),
        "a Ruby module with one includer is outside the rule, but got {found:?}"
    );
}

#[test]
fn php_interface_with_one_implementer_is_not_reported() {
    let found = reported(&PHP_INTERFACE);
    assert!(
        found.is_empty(),
        "a PHP interface with one implementer is outside the rule, but got {found:?}"
    );
}

#[test]
fn go_interface_does_not_hide_an_applicable_abstraction_of_the_same_name() {
    let mut files = JAVA_SHAPE.to_vec();
    files.push((
        "shapes/shape.go",
        "package shapes\n\ntype Shape interface {\n\tArea() float64\n}\n",
    ));
    assert_eq!(
        reported(&files),
        [(
            "src/Shape.java".to_string(),
            1,
            "`Shape` has exactly one implementation (src/Circle.java:1)".to_string()
        )]
    );
}

#[test]
fn ruby_include_is_not_a_second_implementation_of_an_applicable_abstraction() {
    let files = [
        (
            "src/Greeter.java",
            "public interface Greeter {\n    void greet();\n}\n",
        ),
        (
            "src/Friendly.java",
            "public class Friendly implements Greeter {\n    @Override\n    public void greet() {\n    }\n}\n",
        ),
        ("person.rb", "class Person\n  include Greeter\nend\n"),
    ];
    assert_eq!(
        reported(&files),
        [(
            "src/Greeter.java".to_string(),
            1,
            "`Greeter` has exactly one implementation (src/Friendly.java:1)".to_string()
        )]
    );
}

#[test]
fn applicable_languages_still_report_beside_non_applicable_ones() {
    let mut files = RUBY_MIXIN.to_vec();
    files.extend(PHP_INTERFACE);
    files.extend([
        ("src/Drawable.java", "public interface Drawable {\n    void draw();\n}\n"),
        (
            "src/Ring.java",
            "public class Ring implements Drawable {\n    @Override\n    public void draw() {\n    }\n}\n",
        ),
        ("lib/draw.dart", "abstract class Draw {\n  void draw();\n}\n"),
        (
            "lib/circle.dart",
            "import 'draw.dart';\n\nclass Circle implements Draw {\n  @override\n  void draw() {}\n}\n",
        ),
        ("src/render.rs", "pub trait Render {\n    fn render(&self);\n}\n"),
        (
            "src/pane.rs",
            "use crate::render::Render;\n\npub struct Pane;\n\nimpl Render for Pane {\n    fn render(&self) {}\n}\n",
        ),
        ("src/widget.tsx", "export interface Widget {\n  show(): void;\n}\n"),
        (
            "src/panel.tsx",
            "import { Widget } from \"./widget\";\nexport class Panel implements Widget {\n  show(): void {\n  }\n}\n",
        ),
    ]);
    let flagged: Vec<(String, u32)> = reported(&files)
        .into_iter()
        .map(|(path, line, _)| (path, line))
        .collect();
    let expected: Vec<(String, u32)> = [
        ("lib/draw.dart", 1),
        ("src/Drawable.java", 1),
        ("src/render.rs", 1),
        ("src/widget.tsx", 1),
    ]
    .iter()
    .map(|(path, line)| ((*path).to_string(), *line))
    .collect();
    assert_eq!(flagged, expected);
}
