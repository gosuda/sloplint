//! C# test exemptions: one child module per mechanism, so each fix lands with
//! its own case module while the crate links a single integration-test executable.
//!
//! A method is a test when its own attribute list names a test attribute, and every
//! member of a `[TestClass]`/`[TestFixture]` class is a test through module containment.

use crate::support::with_facts;
use sloplint_core::lang::Lang;

const ATTRIBUTED: &str = "public class Widget\n{\n    [Test]\n    public void Flag() { }\n    [TestMethod]\n    public void Check() { }\n    [TestCase]\n    public void Case() { }\n    [TestCaseSource]\n    public void Sourced() { }\n    [DataTestMethod]\n    public void Data() { }\n    [Fact]\n    public void Fact() { }\n    [Theory]\n    public void Theory() { }\n    [SetUp]\n    public void Setup() { }\n    [TearDown]\n    public void Teardown() { }\n    public void Helper() { }\n}\n";

const CONTAINED: &str = "[TestClass]\npublic class Contained\n{\n    public void Check() { }\n    public void Work() { }\n}\n[TestFixture]\npublic class Fixtured\n{\n    public void Verify() { }\n}\npublic class Plain\n{\n    public void Run() { }\n}\n";

fn flags(source: &str) -> Vec<(String, bool)> {
    with_facts(Lang::CSharp, "case.cs", source, |facts| {
        facts
            .functions
            .iter()
            .map(|f| (f.name.to_owned(), f.in_test))
            .collect()
    })
}

#[test]
fn every_test_attribute_marks_only_its_method() {
    assert_eq!(
        flags(ATTRIBUTED),
        [
            ("Flag", true),
            ("Check", true),
            ("Case", true),
            ("Sourced", true),
            ("Data", true),
            ("Fact", true),
            ("Theory", true),
            ("Setup", true),
            ("Teardown", true),
            ("Helper", false),
        ]
        .into_iter()
        .map(|(name, in_test)| (name.to_owned(), in_test))
        .collect::<Vec<_>>(),
    );
}

#[test]
fn test_class_and_fixture_members_are_in_test() {
    assert_eq!(
        flags(CONTAINED),
        [
            ("Check", true),
            ("Work", true),
            ("Verify", true),
            ("Run", false),
        ]
        .into_iter()
        .map(|(name, in_test)| (name.to_owned(), in_test))
        .collect::<Vec<_>>(),
    );
}
