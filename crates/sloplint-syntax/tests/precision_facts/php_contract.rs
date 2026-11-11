use sloplint_core::facts::{ArgShape, PrimitiveKind};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn with_facts(source: &str, mut check: impl FnMut(&sloplint_syntax::Analysis<'_>)) {
    let custom = match CustomRules::compile(&[]) {
        Ok(custom) => custom,
        Err(err) => panic!("empty custom rules compile: {err:?}"),
    };
    let input = FileInput {
        lang: Lang::Php,
        path: "src/C.php",
        source,
        role: CodeRole::Library,
    };
    check(&analyze(&mut Parsers::new(), &input, &custom));
}

const CONCAT_MSG: &str = r#"<?php
error_log("Failed to save order " . $order->id . $suffix);
error_log("err: " . $x);
error_log($order->id . $suffix);
error_log("Saved order {$id} to the database");
error_log("plain static message here");
"#;

#[test]
fn nested_concat_carries_longest_static_run() {
    with_facts(CONCAT_MSG, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let has_shape = |shape: &ArgShape| {
            analysis
                .facts
                .calls
                .iter()
                .any(|c| c.args.first().is_some_and(|a| &a.shape == shape))
        };
        assert!(
            has_shape(&ArgShape::Concat { longest_static: 21 }),
            "nested literal concatenation reports its 21-char static run"
        );
        assert!(
            has_shape(&ArgShape::Concat { longest_static: 5 }),
            "short static run is reported as concat with 5"
        );
        assert!(
            has_shape(&ArgShape::Concat { longest_static: 0 }),
            "all-dynamic concatenation is concat with zero static run"
        );
        assert!(
            has_shape(&ArgShape::Concat { longest_static: 16 }),
            "interpolated string reports its longest chunk as concat"
        );
        assert!(
            has_shape(&ArgShape::Str { longest_static: 25 }),
            "plain static string stays a string shape"
        );
    });
}

const CHAIN_CALLEE: &str = r#"<?php
Log::channel('ops')->info("Order failed");
$this->logger->info("Order failed");
Log::info("Order failed");
"#;

#[test]
fn callee_drops_receiver_call_text() {
    with_facts(CHAIN_CALLEE, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let find = |callee: &str| {
            analysis
                .facts
                .calls
                .iter()
                .find(|c| c.callee == callee)
                .unwrap_or_else(|| panic!("has call with callee {callee}"))
        };
        let chained = find("info");
        assert!(
            !chained.callee.contains('('),
            "callee never carries argument text"
        );
        assert_eq!(
            chained.callee_root, None,
            "chain through a call has no root"
        );
        assert!(
            !chained.is_simple_path,
            "chain through a call is not a simple path"
        );
        assert!(
            find("$this->logger->info").is_simple_path,
            "clean member chain keeps receiver text and simple-path"
        );
        assert_eq!(
            find("Log::info").callee_root,
            Some("Log"),
            "static class call roots at the class name"
        );
        assert_eq!(
            find("Log::channel").callee,
            "Log::channel",
            "direct scoped call keeps its own receiver path"
        );
    });
}

const SCOPE_ROOTS: &str = r#"<?php
function forward($event) {
    return $event::dispatch($event);
}
function slug($t) {
    return \App\Support\Str::slug($t);
}
function plain(Foo $f) {
    return Foo::bar($f);
}
"#;

#[test]
fn scope_roots_are_bare_identifiers_or_none() {
    with_facts(SCOPE_ROOTS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let root_of = |callee: &str| {
            analysis
                .facts
                .calls
                .iter()
                .find(|c| c.callee == callee)
                .map(|c| c.callee_root)
        };
        assert_eq!(root_of("$event::dispatch"), Some(Some("event")));
        assert_eq!(root_of("\\App\\Support\\Str::slug"), Some(None));
        assert_eq!(root_of("Foo::bar"), Some(Some("Foo")));
    });
}

const VARIANTS: &str = r#"<?php
function pick($status) {
    return match ($status) {
        Status::Active => 'on',
        Status::Idle => 'off',
        default => 'x',
    };
}
function pick_one($status) {
    return match ($status) {
        Status::Active => 'on',
        default => 'x',
    };
}
function pick_switch($s) {
    switch ($s) {
        case Status::Active:
            return 1;
        case Status::Active:
            return 2;
        default:
            return 0;
    }
}
function pick_switch_two($s) {
    switch ($s) {
        case Status::Active:
            return 1;
        case Status::Idle:
            return 2;
        default:
            return 0;
    }
}
"#;

#[test]
fn wildcard_arms_need_two_distinct_variants() {
    with_facts(VARIANTS, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let mut arms: Vec<_> = analysis.facts.wildcard_arms.clone();
        arms.sort_by_key(|arm| arm.pos.byte);
        let describe = |arm: &sloplint_core::facts::WildcardArm| (arm.enum_like, arm.variant_count);
        let found: Vec<_> = arms.iter().map(describe).collect();
        assert_eq!(
            found,
            vec![(true, 2), (false, 1), (false, 1), (true, 2)],
            "match counts condition-list variants, switch counts distinct names"
        );
    });
}

const FORCED: &str = r#"<?php
class HtmlRenderer implements Renderer {
    public function render(bool $escape = true, bool $trim = false): string { return ""; }
}
class Widget {
    public function draw(bool $fill = false): void {}
    public function __toString(): string { return ""; }
}
class ChildWidget extends Widget {
    public function draw(bool $fill = false): void {}
}
function free_flag(bool $flag = false): bool { return $flag; }
"#;

#[test]
fn signature_forced_follows_clauses_and_magic_names() {
    with_facts(FORCED, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let forced_of = |name: &str| {
            analysis
                .facts
                .functions
                .iter()
                .find(|f| f.name == name)
                .unwrap_or_else(|| panic!("has function {name}"))
                .signature_forced
        };
        assert!(forced_of("render"), "interface implementation is forced");
        let draws: Vec<bool> = {
            let mut found: Vec<_> = analysis
                .facts
                .functions
                .iter()
                .filter(|f| f.name == "draw")
                .map(|f| (f.pos.byte, f.signature_forced))
                .collect();
            found.sort_by_key(|(byte, _)| *byte);
            found.into_iter().map(|(_, forced)| forced).collect()
        };
        assert_eq!(
            draws,
            vec![false, true],
            "base draw stays free, overriding draw is forced"
        );
        assert!(forced_of("__toString"), "magic method is forced");
        assert!(!forced_of("free_flag"), "free function stays free");
    });
}

const PRIMITIVES: &str = "<?php\nfunction flags(?bool $a = null, bool|null $b = FALSE, bool|string $c = true, $d = TRUE): void {}\n";

#[test]
fn nullable_unions_and_case_insensitive_defaults_count() {
    with_facts(PRIMITIVES, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let flags = analysis
            .facts
            .functions
            .iter()
            .find(|f| f.name == "flags")
            .expect("has flags");
        let param = |name: &str| {
            flags
                .params
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("has param {name}"))
        };
        assert_eq!(param("a").primitive_kind, PrimitiveKind::Bool);
        assert!(param("a").has_default, "nullable bool has a default");
        assert_eq!(param("b").primitive_kind, PrimitiveKind::Bool);
        assert!(
            param("b").bool_default,
            "FALSE literal is a boolean default"
        );
        assert_eq!(param("c").primitive_kind, PrimitiveKind::Other);
        assert!(param("d").bool_default, "untyped TRUE literal counts");
    });
}

const USES: &str = r#"<?php
namespace App\Http;

use App\{Kernel, Router};
use App\Http\Kernel;
use Vendor\Pack\Thing;
use Exception;
"#;

#[test]
fn group_uses_name_roots_and_same_package_stays_none() {
    with_facts(USES, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let root_for = |module: &str| {
            analysis
                .facts
                .imports
                .iter()
                .find(|i| i.module.as_ref() == module)
                .map(|i| i.external_root)
        };
        assert_eq!(root_for("App"), Some(Some("App")));
        assert_eq!(root_for("App\\Http"), Some(None));
        assert_eq!(root_for("Vendor\\Pack"), Some(Some("Vendor")));
        let global = analysis
            .facts
            .imports
            .iter()
            .find(|i| i.name == "Exception")
            .expect("has Exception import");
        assert_eq!(global.external_root, None, "single-segment use has no root");
    });
}

const TYPES: &str = r#"<?php
abstract class Shape implements Drawable {}
interface Stream extends Reader {}
class Circle implements Drawable {}
"#;

#[test]
fn abstract_types_are_abstractions_with_abstract_children() {
    with_facts(TYPES, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let abstraction_of = |name: &str| {
            analysis
                .facts
                .types
                .iter()
                .find(|t| t.name == name)
                .unwrap_or_else(|| panic!("has type {name}"))
                .abstraction
        };
        assert!(abstraction_of("Shape"), "abstract class is an abstraction");
        assert!(abstraction_of("Stream"), "interface is an abstraction");
        assert!(!abstraction_of("Circle"), "concrete class is not");
        let mut refs: Vec<_> = analysis.facts.impls.clone();
        refs.sort_by_key(|r| r.pos.byte);
        let flags: Vec<_> = refs.iter().map(|r| (r.name, r.child_is_abstract)).collect();
        assert_eq!(
            flags,
            vec![("Drawable", true), ("Reader", true), ("Drawable", false),],
            "abstract class and interface children are abstract"
        );
    });
}

const CREATION: &str = r#"<?php
function make($data, $class) {
    $one = new Foo($data);
    $two = new $class($data);
    return [$one, $two];
}
"#;

#[test]
fn named_creation_roots_at_the_class_and_dynamic_stays_absent() {
    with_facts(CREATION, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let news: Vec<_> = analysis
            .facts
            .calls
            .iter()
            .filter(|c| c.kind == sloplint_core::facts::CallKind::New)
            .collect();
        assert_eq!(news.len(), 1, "only named creation yields a call fact");
        assert_eq!(news[0].callee_root, Some("Foo"));
        assert!(news[0].is_simple_path, "named creation is a simple path");
    });
}

const DESTRUCTURE: &str = r#"<?php
[$a, $b] = compute([1, 2]);
$c = compute([1, 2]);
"#;

#[test]
fn destructuring_is_not_a_binding_initializer() {
    with_facts(DESTRUCTURE, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let mut calls: Vec<_> = analysis.facts.calls.clone();
        calls.sort_by_key(|c| c.pos.byte);
        assert_eq!(calls.len(), 2, "both compute calls are facts");
        assert!(
            !calls[0].initializes_binding,
            "destructuring left side is not a plain binding"
        );
        assert!(
            calls[1].initializes_binding,
            "variable left side initializes"
        );
    });
}

const ASSERT_TOP: &str = r#"<?php
class Checker {
    public function t($a, $b, $c): void {
        $this->assertTrue($a == $b);
        $this->assertTrue($a === $b);
        $this->assertTrue($a && $b);
        $this->assertTrue($c);
    }
}
"#;

#[test]
fn top_level_equality_assertions_are_top_eq_ne() {
    with_facts(ASSERT_TOP, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let ordered: Vec<_> = analysis.facts.assertions.iter().map(|a| a.shape).collect();
        assert_eq!(
            ordered,
            vec![
                sloplint_core::facts::AssertShape::TopEqNe,
                sloplint_core::facts::AssertShape::TopEqNe,
                sloplint_core::facts::AssertShape::Other,
                sloplint_core::facts::AssertShape::BareName,
            ],
            "top ==/=== is TopEqNe, && stays Other, bare variable is BareName"
        );
    });
}

const ARG_SHAPES: &str = r#"<?php
$v->render([1, 2, 3]);
$v->save(["a" => 1, "b" => 2]);
$v->register([function () { return 1; }, 2]);
"#;

#[test]
fn keyed_literals_are_other_and_closure_lists_are_code_body() {
    with_facts(ARG_SHAPES, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let shape_of = |callee: &str| {
            analysis
                .facts
                .calls
                .iter()
                .find(|c| c.callee == callee)
                .unwrap_or_else(|| panic!("has call {callee}"))
                .args[0]
                .shape
                .clone()
        };
        assert_eq!(
            shape_of("$v->render"),
            ArgShape::Collection,
            "plain list literal is a collection"
        );
        assert_eq!(
            shape_of("$v->save"),
            ArgShape::Other,
            "keyed literal is not a collection"
        );
        assert_eq!(
            shape_of("$v->register"),
            ArgShape::CodeBody,
            "list containing a closure is a code body"
        );
    });
}

const DELEGATION: &str = r#"<?php
class Child extends Base {
    public function __construct($name) {
        parent::__construct($name);
    }
    public static function create($name) {
        return static::create($name);
    }
}
function relay($name) {
    return Other::create($name);
}
"#;

#[test]
fn receiver_keywords_root_at_the_keyword() {
    with_facts(DELEGATION, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses");
        let head_of = |callee: &str| {
            analysis
                .facts
                .calls
                .iter()
                .find(|c| c.callee == callee)
                .unwrap_or_else(|| panic!("has call {callee}"))
        };
        let parent = head_of("parent::__construct");
        assert_eq!(parent.callee_root, Some("parent"), "parent roots at parent");
        assert!(parent.is_simple_path, "parent delegation is a simple path");
        let static_head = head_of("static::create");
        assert_eq!(
            static_head.callee_root,
            Some("static"),
            "static roots at static"
        );
        assert!(
            static_head.is_simple_path,
            "static delegation is a simple path"
        );
        assert_eq!(
            head_of("Other::create").callee_root,
            Some("Other"),
            "plain class scope still roots at the class"
        );
    });
}
