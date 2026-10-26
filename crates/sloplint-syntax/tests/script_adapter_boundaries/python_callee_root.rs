//! Python call sites record the leftmost name of their callee chain, so receiver
//! delegation (`self.store.get(...)`, `super().peek(...)`) is distinguishable from a
//! free-function or module-level wrapper.

use sloplint_core::facts::FileFacts;
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"class Cache:
    def get(self, key, ttl):
        return self.store.get(key, ttl)

    def peek(self, key, ttl):
        return super().peek(key, ttl)

    def deep(self, key, ttl):
        return self.a.b.c.get(key, ttl)

    def loose(self, key, ttl):
        return self.store[key].get(key, ttl)

    @classmethod
    def make(cls, key, ttl):
        return cls.registry.build(key, ttl)


def wrap(key, ttl):
    return store.get(key, ttl)


def direct(key, ttl):
    return fetch(key, ttl)
"#;

fn root_of(callee: &str) -> Option<String> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "case.py",
        source: SOURCE,
        is_test_file: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "case.py parses");
    let facts: &FileFacts<'_> = &analysis.facts;
    let call = facts
        .calls
        .iter()
        .find(|c| c.callee == callee)
        .unwrap_or_else(|| panic!("case.py has a call to {callee}"));
    call.callee_root.map(str::to_owned)
}

#[test]
fn receiver_attribute_chain_roots_at_self() {
    assert_eq!(root_of("self.store.get"), Some("self".to_owned()));
    assert_eq!(root_of("self.a.b.c.get"), Some("self".to_owned()));
}

#[test]
fn subscripted_receiver_roots_at_self() {
    assert_eq!(root_of("self.store[key].get"), Some("self".to_owned()));
}

#[test]
fn super_call_receiver_roots_at_super() {
    assert_eq!(root_of("super().peek"), Some("super".to_owned()));
}

#[test]
fn class_method_receiver_roots_at_cls() {
    assert_eq!(root_of("cls.registry.build"), Some("cls".to_owned()));
}

#[test]
fn module_level_receiver_keeps_its_own_name() {
    assert_eq!(root_of("store.get"), Some("store".to_owned()));
    assert_eq!(root_of("fetch"), Some("fetch".to_owned()));
}
