use sloplint_core::facts::{AssertShape, ImportKind, ImportScope, PrimitiveKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const MEMBERS: &str = r#"<?php
class Service {
  public function run(): void {}
  private function helper(): void {}
  protected function hook(): void {}
}

function top(): void {}
"#;

const PARAMS: &str = r#"<?php
function send(string $host, int $port = 80, bool $secure = true, float $ratio = 0.5, ...$rest): void {}
"#;

const CALLS: &str = r#"<?php
$x = strlen($s);
$this->render([1, 2, 3]);
$this->save(["a" => 1]);
$fn($s);
dispatch(function () { return 1; });
"#;

const IMPORTS: &str = r#"<?php
use Foo\Bar;
use function Vendor\helper as help;
require 'lib.php';
"#;

const ASSERTS: &str = r#"<?php
class ParserTest extends TestCase {
  public function testParse(): void {
    $this->assertSame($a, $b);
    $this->assertTrue($ok);
  }
}

class Plain {
  public function check(): void {
    $this->assertTrue($ok);
  }
}
"#;

const CHAINED_CALLS: &str = r#"<?php
$cfg->get()->run($p);
$this->render(["a" => 1]);
"#;

fn analyze_src<'s>(
    parsers: &mut Parsers<'_>,
    custom: &'s CustomRules,
    source: &'s str,
    path: &'s str,
) -> sloplint_syntax::Analysis<'s> {
    let input = FileInput {
        lang: Lang::Php,
        path,
        source,
        role: CodeRole::Library,
    };
    analyze(parsers, &input, custom)
}

#[test]
fn member_visibility_keeps_protected_exported() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_src(&mut Parsers::new(), &custom, MEMBERS, "src/Service.php");
    assert!(analysis.parse_error.is_none(), "parses");
    let named_fn = |name: &str| {
        analysis
            .facts
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("has function {name}"))
    };
    assert_eq!(
        named_fn("run").visibility,
        Visibility::Exported,
        "public method is exported"
    );
    assert_eq!(
        named_fn("helper").visibility,
        Visibility::Private,
        "private method is private"
    );
    assert_eq!(
        named_fn("hook").visibility,
        Visibility::Exported,
        "protected method stays exported"
    );
    assert_eq!(
        named_fn("top").visibility,
        Visibility::Exported,
        "free function is exported"
    );
    assert!(
        !named_fn("run").signature_forced,
        "plain method is not forced"
    );
}

#[test]
fn params_carry_primitive_kinds_defaults_and_variadics() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_src(&mut Parsers::new(), &custom, PARAMS, "src/Send.php");
    assert!(analysis.parse_error.is_none(), "parses");
    let send = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "send")
        .expect("has send");
    let named_param = |name: &str| {
        send.params
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("has param {name}"))
    };
    assert_eq!(
        named_param("host").primitive_kind,
        PrimitiveKind::Str,
        "host is string"
    );
    assert!(!named_param("host").has_default, "host has no default");
    assert_eq!(
        named_param("port").primitive_kind,
        PrimitiveKind::Int,
        "port is integer"
    );
    assert!(named_param("port").has_default, "port has a default");
    assert!(
        !named_param("port").bool_default,
        "port default is not boolean"
    );
    assert!(
        named_param("secure").bool_default,
        "secure defaults to true"
    );
    assert_eq!(
        named_param("ratio").primitive_kind,
        PrimitiveKind::Float,
        "ratio is float"
    );
    assert!(named_param("rest").is_variadic, "rest is variadic");
}

#[test]
fn call_binding_paths_and_arg_shapes() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_src(&mut Parsers::new(), &custom, CALLS, "src/C.php");
    assert!(analysis.parse_error.is_none(), "parses");
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "strlen")
        .expect("has strlen call");
    assert!(plain.initializes_binding, "x = strlen initializes");
    assert!(plain.is_simple_path, "bare strlen is a simple path");
    let dynamic = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "$fn")
        .expect("has variable-function call");
    assert!(
        !dynamic.is_simple_path,
        "variable function is not a simple path"
    );
    let shapes: Vec<String> = analysis
        .facts
        .calls
        .iter()
        .flat_map(|c| c.args.iter().map(|a| format!("{:?}", a.shape)))
        .collect();
    assert!(
        shapes.iter().any(|s| s == "Collection"),
        "array literal is a collection"
    );
    assert!(
        shapes.iter().any(|s| s == "CodeBody"),
        "closure is a code body"
    );
}

#[test]
fn chained_member_call_through_a_call_is_not_a_simple_path() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_src(&mut Parsers::new(), &custom, CHAINED_CALLS, "src/C.php");
    assert!(analysis.parse_error.is_none(), "parses");
    let chained = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "run")
        .expect("has chained call");
    assert!(
        !chained.is_simple_path,
        "call whose receiver contains a call is not a simple path"
    );
    let direct = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "$this->render")
        .expect("has direct member call");
    assert!(
        direct.is_simple_path,
        "direct member call on a variable stays a simple path"
    );
}

#[test]
fn php_namespaced_uses_report_vendor_roots() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_src(&mut Parsers::new(), &custom, IMPORTS, "src/C.php");
    assert!(analysis.parse_error.is_none(), "parses");
    let named = analysis
        .facts
        .imports
        .iter()
        .find(|i| i.kind == ImportKind::Named && i.name == "Bar")
        .expect("has Bar import");
    assert_eq!(
        named.scope,
        ImportScope::Module,
        "top-level use is module scope"
    );
    assert_eq!(
        named.external_root,
        Some("Foo"),
        "namespaced use names its vendor root"
    );
    assert!(!named.is_reexport, "use is not a re-export");
    let side = analysis
        .facts
        .imports
        .iter()
        .find(|i| i.kind == ImportKind::SideEffect)
        .expect("has require import");
    assert_eq!(side.module.as_ref(), "lib.php", "require keeps its path");
    assert!(
        side.external_root.is_none(),
        "require names no external root"
    );
}

#[test]
fn assertions_shape_and_test_marking() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_src(
        &mut Parsers::new(),
        &custom,
        ASSERTS,
        "tests/ParserTest.php",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let same = analysis
        .facts
        .assertions
        .iter()
        .find(|a| a.comparison)
        .expect("has comparing assertion");
    assert_eq!(
        same.shape,
        AssertShape::TopEqNe,
        "assertSame is a top-level comparison"
    );
    assert!(
        analysis
            .facts
            .assertions
            .iter()
            .any(|a| a.shape == AssertShape::BareName),
        "assertTrue on a variable is a bare name"
    );
    let test_fn = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "testParse")
        .expect("has testParse");
    assert!(test_fn.in_test, "TestCase method is in test");
    let library = analyze_src(&mut Parsers::new(), &custom, ASSERTS, "src/Plain.php");
    assert!(library.parse_error.is_none(), "parses");
    let plain_fn = library
        .facts
        .functions
        .iter()
        .find(|f| f.name == "check")
        .expect("has check");
    assert!(!plain_fn.in_test, "plain class method is not in test");
}

const CHAINED_FETCH: &str = r#"<?php
$a->b()->c->d($x);
$a->b->c($x);
"#;

#[test]
fn chained_call_behind_a_property_fetch_is_not_a_simple_path() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_src(&mut Parsers::new(), &custom, CHAINED_FETCH, "src/C.php");
    assert!(analysis.parse_error.is_none(), "parses");
    let hidden = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "d")
        .expect("has chained call");
    assert_eq!(hidden.callee_root, None, "chain through a call has no root");
    assert!(
        !hidden.is_simple_path,
        "call hidden behind a property fetch is not a simple path"
    );
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "$a->b->c")
        .expect("has pure property chain call");
    assert!(
        plain.is_simple_path,
        "pure property chain stays a simple path"
    );
}
