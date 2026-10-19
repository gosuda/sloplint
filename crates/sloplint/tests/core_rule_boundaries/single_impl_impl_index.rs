//! `slop/single-impl-abstraction`: an abstraction is reported only when exactly one type implements it.

use sloplint::{RunOptions, execute};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

const RULE: &str = "slop/single-impl-abstraction";

fn interface(name: &str) -> (String, String) {
    (
        format!("src/{name}.java"),
        format!("public interface {name} {{\n    void run();\n}}\n"),
    )
}

fn implementor(class: &str, interface: &str) -> (String, String) {
    (
        format!("src/{class}.java"),
        format!(
            "public class {class} implements {interface} {{\n    @Override\n    public void run() {{\n    }}\n}}\n"
        ),
    )
}

/// `(path, message)` of every finding for a project made of `files`.
fn reported(files: &[(String, String)]) -> Vec<(String, String)> {
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
        .map(|finding| (finding.path.to_string(), finding.message.clone()))
        .collect()
}

#[test]
fn single_implementation_is_reported_with_its_location() {
    let files = [interface("Lone"), implementor("Worker", "Lone")];
    assert_eq!(
        reported(&files),
        [(
            "src/Lone.java".to_string(),
            "`Lone` has exactly one implementation (src/Worker.java:1)".to_string()
        )]
    );
}

#[test]
fn second_implementation_in_another_file_clears_the_report() {
    let files = [
        interface("Drawable"),
        implementor("Circle", "Drawable"),
        implementor("Square", "Drawable"),
    ];
    let found = reported(&files);
    assert!(
        found.is_empty(),
        "a second implementation in another file must clear the report, but got {found:?}"
    );
}

#[test]
fn second_implementation_in_the_same_file_clears_the_report() {
    let (circle_path, circle_source) = implementor("Circle", "Drawable");
    let extra =
        "\nclass Square implements Drawable {\n    @Override\n    public void run() {\n    }\n}\n";
    let files = [
        interface("Drawable"),
        (circle_path, format!("{circle_source}{extra}")),
    ];
    let found = reported(&files);
    assert!(
        found.is_empty(),
        "a second implementation in the same file must clear the report, but got {found:?}"
    );
}

#[test]
fn abstraction_without_implementation_is_not_reported() {
    let found = reported(&[interface("Orphan")]);
    assert!(
        found.is_empty(),
        "an abstraction with no implementation must not be reported, but got {found:?}"
    );
}

#[test]
fn each_abstraction_resolves_against_its_own_implementations() {
    let mut files: Vec<(String, String)> = (0..60).map(|i| interface(&format!("I{i}"))).collect();
    for i in (0..60).filter(|i| i % 3 != 0) {
        files.push(implementor(&format!("A{i}"), &format!("I{i}")));
    }
    for i in (0..60).filter(|i| i % 3 == 2) {
        files.push(implementor(&format!("B{i}"), &format!("I{i}")));
    }
    let expected: Vec<(String, String)> = {
        let mut rows: Vec<(String, String)> = (0..60)
            .filter(|i| i % 3 == 1)
            .map(|i| {
                (
                    format!("src/I{i}.java"),
                    format!("`I{i}` has exactly one implementation (src/A{i}.java:1)"),
                )
            })
            .collect();
        rows.sort();
        rows
    };
    assert_eq!(reported(&files), expected);
}
