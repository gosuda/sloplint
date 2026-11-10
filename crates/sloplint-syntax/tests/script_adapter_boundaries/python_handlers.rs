//! Python handler, assertion, struct-literal, and match-arm facts.

use sloplint_core::facts::{AssertShape, FileFacts, ScrutineeKind};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const HANDLERS: &str = r#"def read(path):
    try:
        return open(path).read()
    except OSError:
        pass


def load(name):
    try:
        import optional
    except ImportError:
        pass


def cleanup(self):
    try:
        self.close()
    except OSError:
        pass


class Store:
    def __del__(self):
        try:
            self.close()
        except OSError:
            pass
"#;

const ASSERTS: &str = r#"import sys


def check(ready, count):
    assert ready
    assert count == 2
    assert isinstance(count, int)
"#;

const TYPECHECK: &str = r#"import typing


def check(x):
    if typing.TYPE_CHECKING:
        assert x
"#;

const COPIES: &str = r#"def build(s):
    return Point(x=s.x, y=s.y, z=s.z, w=s.w, v=s.v, u=s.u)


def convert(obj):
    return obj.convert(a=obj.a, b=obj.b)


def rebuild(self, params):
    return type(self)(**params)


def bag(items):
    return dict(a=items.a, b=items.b)
"#;

const MATCHES: &str = r#"def f(color):
    match color:
        case Color.RED: return 1
        case Color.GREEN: return 2
        case _: return 0


def g(x):
    match x:
        case 1: return 1
        case _: return 0
"#;

fn with_facts(source: &str, read: impl FnOnce(&FileFacts<'_>)) {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "case.py",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "parses");
    read(&analysis.facts);
}

#[test]
fn plain_swallow_has_no_flags() {
    with_facts(HANDLERS, |facts| {
        let first = facts.handlers.first().expect("has handler");
        assert!(!first.has_else);
        assert!(!first.try_imports_only);
        assert!(!first.in_cleanup);
    });
}

#[test]
fn import_only_try_is_marked() {
    with_facts(HANDLERS, |facts| {
        assert!(facts.handlers.len() >= 2);
        let second = &facts.handlers[1];
        assert!(second.try_imports_only);
    });
}

#[test]
fn del_cleanup_is_marked() {
    with_facts(HANDLERS, |facts| {
        let last = facts.handlers.last().expect("has handler");
        assert!(last.in_cleanup);
    });
}

#[test]
fn cond_shapes_are_classified() {
    with_facts(ASSERTS, |facts| {
        let shapes = facts.assertions.iter().map(|a| a.shape).collect::<Vec<_>>();
        assert!(shapes.contains(&AssertShape::BareName));
        assert!(shapes.contains(&AssertShape::TopEqNe));
        assert!(shapes.contains(&AssertShape::Other));
    });
}

#[test]
fn type_checking_assert_is_marked() {
    with_facts(TYPECHECK, |facts| {
        let only = facts.assertions.first().expect("has assertion");
        assert!(only.in_typecheck);
        assert_eq!(only.shape, AssertShape::BareName);
    });
    with_facts(ASSERTS, |facts| {
        assert!(facts.assertions.iter().all(|a| !a.in_typecheck));
    });
}

#[test]
fn bare_ident_copy_reports_source() {
    with_facts(COPIES, |facts| {
        let hits = facts
            .field_copies
            .iter()
            .filter(|c| c.source == Some("s"))
            .collect::<Vec<_>>();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].fields, 6);
    });
}

#[test]
fn member_dict_and_rebuild_copies_stay_silent() {
    with_facts(COPIES, |facts| {
        assert!(
            facts
                .field_copies
                .iter()
                .all(|c| c.source != Some("obj") && c.source != Some("items"))
        );
        assert_eq!(facts.field_copies.len(), 1);
    });
}

#[test]
fn wildcard_arm_reads_match_subject_and_siblings() {
    with_facts(MATCHES, |facts| {
        assert_eq!(facts.wildcard_arms.len(), 2, "one wildcard arm per match");
        let dotted = facts
            .wildcard_arms
            .iter()
            .find(|a| a.variant_count == 2)
            .expect("dotted patterns count two siblings");
        assert_eq!(
            dotted.scrutinee,
            ScrutineeKind::Ident,
            "match subject identifier is reported"
        );
        assert!(dotted.enum_like, "dotted siblings are enum-like");
        let literal = facts
            .wildcard_arms
            .iter()
            .find(|a| a.variant_count == 0)
            .expect("literal patterns count no siblings");
        assert!(!literal.enum_like, "literal patterns are not enum-like");
    });
}
