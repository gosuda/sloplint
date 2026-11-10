//! Role regions through the public `analyze` entry.
//!
//! Glob roles arrive on `FileInput` and pass through untouched; adapters add
//! Test and Bench regions, and any Test region sets the inline marker.

use std::sync::LazyLock;

use sloplint_core::facts::FileFacts;
use sloplint_core::lang::Lang;
use sloplint_core::role::{CodeRole, RoleRegion};
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn analyze_as<'s>(lang: Lang, path: &'s str, role: CodeRole, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang,
        path,
        source,
        role,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &NO_CUSTOM);
    assert!(
        analysis.parse_error.is_none(),
        "{path} parses, first error at {:?}",
        analysis.parse_error
    );
    analysis
}

fn region_lines(facts: &FileFacts<'_>) -> Vec<(u32, u32, CodeRole)> {
    facts
        .regions
        .iter()
        .map(|region: &RoleRegion| (region.start_line, region.end_line, region.role))
        .collect()
}

#[test]
fn go_functions_emit_test_bench_and_example_regions() {
    let source = "package parse\n\nimport \"testing\"\n\nfunc TestParse(t *testing.T) {}\nfunc BenchmarkParse(b *testing.B) {}\nfunc FuzzParse(f *testing.F) {}\nfunc ExampleParse() {}\nfunc helper() {}\n";
    let analysis = analyze_as(Lang::Go, "parse_test.go", CodeRole::Library, source);
    assert_eq!(
        region_lines(&analysis.facts),
        [
            (5, 5, CodeRole::Test),
            (6, 6, CodeRole::Bench),
            (7, 7, CodeRole::Bench),
            (8, 8, CodeRole::Example),
        ]
    );
    assert!(
        analysis.facts.has_inline_tests,
        "a Test region sets the inline marker"
    );
    assert_eq!(analysis.facts.role, CodeRole::Library);
}

#[test]
fn go_library_name_prefixes_and_bad_signatures_emit_no_regions() {
    let source = "package search\n\nfunc FuzzyMatch(q, s string) bool { return q != \"\" }\nfunc Testable() bool { return true }\nfunc ExampleHelper() {}\n";
    let analysis = analyze_as(
        Lang::Go,
        "internal/search/fuzzy.go",
        CodeRole::Library,
        source,
    );
    assert!(
        analysis.facts.regions.is_empty(),
        "library prefixes without a _test.go file mark nothing, got {:?}",
        analysis.facts.regions
    );
    let source = "package parse\n\nimport \"testing\"\n\nfunc TestNoSig() {}\nfunc TestExtra(t *testing.T, n int) {}\n";
    let analysis = analyze_as(Lang::Go, "parse_test.go", CodeRole::Test, source);
    assert!(
        analysis.facts.regions.is_empty(),
        "bad Test signatures mark nothing, got {:?}",
        analysis.facts.regions
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn go_test_function_opens_a_test_region() {
    let analysis = analyze_as(
        Lang::Go,
        "parse_test.go",
        CodeRole::Test,
        "package parse\n\nimport \"testing\"\n\nfunc TestParse(t *testing.T) {}\n",
    );
    assert!(
        analysis
            .facts
            .regions
            .iter()
            .any(|region| region.role == CodeRole::Test),
        "the Test function opens a Test region, got {:?}",
        analysis.facts.regions
    );
    assert!(
        analysis.facts.has_inline_tests,
        "a Test region sets the inline marker"
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn typescript_top_level_calls_mark_test_and_bench_regions() {
    let source =
        "it(\"parses\", () => {});\ntest(\"serializes\", () => {});\nbench(\"fast\", () => {});\n";
    let analysis = analyze_as(Lang::TypeScript, "case.ts", CodeRole::Library, source);
    assert_eq!(
        region_lines(&analysis.facts),
        [
            (1, 1, CodeRole::Test),
            (2, 2, CodeRole::Test),
            (3, 3, CodeRole::Bench),
        ]
    );
    assert!(
        analysis.facts.has_inline_tests,
        "a Test region sets the inline marker"
    );
    let with_tests = analyze_as(Lang::TypeScript, "case.test.ts", CodeRole::Test, source);
    assert_eq!(with_tests.facts.role, CodeRole::Test);
    let plain = "export function helper() {\n  return 1;\n}\n";
    let analysis = analyze_as(Lang::TypeScript, "case.test.ts", CodeRole::Test, plain);
    assert!(
        analysis.facts.regions.is_empty(),
        "a helper marks no regions, got {:?}",
        analysis.facts.regions
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn caller_resolved_roles_pass_through() {
    let analysis = analyze_as(Lang::Rust, "src/main.rs", CodeRole::Entry, "fn main() {}\n");
    assert_eq!(analysis.facts.role, CodeRole::Entry);
    let analysis = analyze_as(
        Lang::Rust,
        "src/parse.rs",
        CodeRole::Library,
        "fn parse() {}\n",
    );
    assert_eq!(analysis.facts.role, CodeRole::Library);
}

#[test]
fn odin_block_comment_before_the_returned_call_is_skipped() {
    let source = "package p\n\nping := proc() {\n\treturn /* note */ target()\n}\n";
    let analysis = analyze_as(Lang::Odin, "case.odin", CodeRole::Library, source);
    let ping = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "ping")
        .expect("the procedure is recorded");
    assert!(
        ping.sole_call.is_some(),
        "the block comment is not the returned call"
    );
}

#[test]
fn odin_block_comment_between_statements_is_skipped() {
    let source = "package p\n\nping := proc() {\n\t/* leading */\n\treturn target()\n}\n";
    let analysis = analyze_as(Lang::Odin, "case.odin", CodeRole::Library, source);
    let ping = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "ping")
        .expect("the procedure is recorded");
    assert!(
        ping.sole_call.is_some(),
        "the leading block comment is not a statement"
    );
}

#[test]
fn odin_test_attribute_opens_a_test_region() {
    let source = "package probe\n\n@(test)\ncheck_it :: proc() {\n}\n";
    let analysis = analyze_as(Lang::Odin, "calc.odin", CodeRole::Library, source);
    assert!(
        analysis.facts.regions.iter().any(|region| {
            region.role == CodeRole::Test && region.start_line == 3 && region.end_line == 5
        }),
        "the @(test) region covers the procedure, got {:?}",
        analysis.facts.regions
    );
    assert!(
        analysis.facts.has_inline_tests,
        "a Test region sets the inline marker"
    );
}

#[test]
fn typescript_describe_nested_calls_mark_a_test_region() {
    let source = "describe('parse', () => { it('reads', () => { expect(1).toBe(1); }); });\n";
    let analysis = analyze_as(
        Lang::TypeScript,
        "src/parse.test.ts",
        CodeRole::Test,
        source,
    );
    assert!(
        analysis
            .facts
            .regions
            .iter()
            .any(|region| region.role == CodeRole::Test),
        "the nested it call opens a Test region, got {:?}",
        analysis.facts.regions
    );
    assert!(
        analysis.facts.has_inline_tests,
        "a Test region sets the inline marker"
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn typescript_member_test_calls_mark_no_regions() {
    let source = "parser.test(x);\n/re/.test(s);\n";
    let analysis = analyze_as(Lang::TypeScript, "case.test.ts", CodeRole::Test, source);
    assert!(
        analysis.facts.regions.is_empty(),
        "member test calls mark nothing, got {:?}",
        analysis.facts.regions
    );
    assert!(
        !analysis.facts.has_inline_tests,
        "no Test region sets no marker"
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn ruby_rspec_blocks_mark_a_test_region() {
    let source = "RSpec.describe User do\n  it 'is valid' do\n    expect(User.new).to be_valid\n  end\nend\n";
    let analysis = analyze_as(
        Lang::Ruby,
        "spec/models/user_spec.rb",
        CodeRole::Test,
        source,
    );
    assert!(
        analysis
            .facts
            .regions
            .iter()
            .any(|region| region.role == CodeRole::Test),
        "the it block opens a Test region, got {:?}",
        analysis.facts.regions
    );
    assert!(
        analysis.facts.has_inline_tests,
        "a Test region sets the inline marker"
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn ruby_receiver_it_calls_mark_no_regions() {
    let source = "obj.it('flies') { }\n";
    let analysis = analyze_as(
        Lang::Ruby,
        "spec/models/obj_spec.rb",
        CodeRole::Test,
        source,
    );
    assert!(
        analysis.facts.regions.is_empty(),
        "a receiver it call marks nothing, got {:?}",
        analysis.facts.regions
    );
    assert!(
        !analysis.facts.has_inline_tests,
        "no Test region sets no marker"
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn scala_known_suite_bases_open_test_modules() {
    let source = "import org.scalatest.funsuite.AnyFunSuite\nclass ParserSuite extends AnyFunSuite {\n  def parses: Boolean = true\n}\n";
    let analysis = analyze_as(
        Lang::Scala,
        "src/test/scala/ParserSuite.scala",
        CodeRole::Library,
        source,
    );
    assert!(
        analysis.facts.regions.iter().any(|region| {
            region.role == CodeRole::Test && region.start_line == 3 && region.end_line == 3
        }),
        "the Test region covers the suite body, got {:?}",
        analysis.facts.regions
    );
    let source = "object ParserSpec extends ZIOSpecDefault {\n  def spec = suite(\"p\")(test(\"x\")(assertTrue(true)))\n}\n";
    let analysis = analyze_as(
        Lang::Scala,
        "src/test/scala/ParserSpec.scala",
        CodeRole::Test,
        source,
    );
    assert!(
        analysis
            .facts
            .regions
            .iter()
            .any(|region| region.role == CodeRole::Test),
        "the suite test call opens a Test region, got {:?}",
        analysis.facts.regions
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn scala_spec_suffix_alone_is_not_a_suite() {
    let source = "abstract class BaseSpec\nfinal class ServiceSpec extends BaseSpec {\n  def replicas: Int = 3\n}\n";
    let analysis = analyze_as(
        Lang::Scala,
        "src/main/scala/deploy/ServiceSpec.scala",
        CodeRole::Library,
        source,
    );
    assert!(
        analysis.facts.regions.is_empty(),
        "a production Spec suffix marks nothing, got {:?}",
        analysis.facts.regions
    );
    assert_eq!(analysis.facts.role, CodeRole::Library);
}

#[test]
fn scala_test_calls_mark_a_test_region() {
    let source = "class HashSuite extends FunSuite {\n  test(\"hashes\") { assert(true) }\n}\n";
    let analysis = analyze_as(
        Lang::Scala,
        "src/test/scala/HashSuite.scala",
        CodeRole::Test,
        source,
    );
    assert!(
        analysis
            .facts
            .regions
            .iter()
            .any(|region| region.role == CodeRole::Test),
        "the test call opens a Test region, got {:?}",
        analysis.facts.regions
    );
    assert!(
        analysis.facts.has_inline_tests,
        "a Test region sets the inline marker"
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn scala_test_calls_outside_suites_mark_no_regions() {
    let source = "class Probe {\n  def run(): Unit = { test(\"x\") }\n}\n";
    let analysis = analyze_as(
        Lang::Scala,
        "src/main/scala/Probe.scala",
        CodeRole::Library,
        source,
    );
    assert!(
        analysis.facts.regions.is_empty(),
        "a test call outside a suite marks nothing, got {:?}",
        analysis.facts.regions
    );
    assert_eq!(analysis.facts.role, CodeRole::Library);
}

#[test]
fn dart_loop_nested_test_calls_mark_a_test_region() {
    let source = "void main() { for (final c in cases) { test('case', () {}); } }\n";
    let analysis = analyze_as(Lang::Dart, "test/parse_test.dart", CodeRole::Test, source);
    assert!(
        analysis
            .facts
            .regions
            .iter()
            .any(|region| region.role == CodeRole::Test),
        "the loop-nested test call opens a Test region, got {:?}",
        analysis.facts.regions
    );
    assert!(
        analysis.facts.has_inline_tests,
        "a Test region sets the inline marker"
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn dart_test_calls_outside_main_mark_no_regions() {
    let source = "void helper() { test('x', () {}); }\nvoid main() {}\n";
    let analysis = analyze_as(Lang::Dart, "test/parse_test.dart", CodeRole::Test, source);
    assert!(
        analysis.facts.regions.is_empty(),
        "a test call outside main marks nothing, got {:?}",
        analysis.facts.regions
    );
    assert!(
        !analysis.facts.has_inline_tests,
        "no Test region sets no marker"
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn odin_private_attribute_marks_procedure_not_public() {
    let source = "package p\n@(private)\nhide :: proc() {\n}\n";
    let analysis = analyze_as(Lang::Odin, "hide.odin", CodeRole::Library, source);
    let hide = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "hide")
        .expect("the procedure is recorded");
    assert!(!hide.public, "@(private) clears the public flag");
    let source = "package p\nshow :: proc() {\n}\n";
    let analysis = analyze_as(Lang::Odin, "show.odin", CodeRole::Library, source);
    let show = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "show")
        .expect("the procedure is recorded");
    assert!(
        show.public,
        "an undecorated top-level procedure stays public"
    );
}
