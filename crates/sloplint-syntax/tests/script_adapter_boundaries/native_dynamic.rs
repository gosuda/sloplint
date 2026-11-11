//! C++ and ObjC dynamic symbols: a plain literal name stays silent.

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn symbols(source: &str, lang: Lang, path: &str) -> Vec<String> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang,
        path,
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "parses");
    analysis
        .facts
        .dynamic_symbols
        .iter()
        .map(|d| d.what.to_owned())
        .collect()
}

const CPP_LITERAL: &str = r#"#include <dlfcn.h>
void* load(void* h) { return dlsym(h, "symbol_name"); }
"#;

const CPP_VARIABLE: &str = r#"#include <dlfcn.h>
void* load(void* h, const char* name) { return dlsym(h, name); }
"#;

const OBJC_LITERAL_LOOKUP: &str = r#"#include <dlfcn.h>
void* load(void* h) { return dlsym(h, "symbol_name"); }
"#;

const OBJC_LITERAL_SELECTOR: &str = r#"void use_selector(void) {
    NSSelectorFromString(@"run");
}
"#;

const OBJC_VARIABLE_SELECTOR: &str = r#"void use_name(const char* name) {
    NSSelectorFromString(name);
}
"#;

#[test]
fn cpp_literal_stays_silent() {
    assert_eq!(
        symbols(CPP_LITERAL, Lang::Cpp, "case.cpp"),
        Vec::<String>::new()
    );
}

#[test]
fn cpp_variable_still_flags() {
    assert_eq!(symbols(CPP_VARIABLE, Lang::Cpp, "case.cpp").len(), 1);
}

#[test]
fn objc_literal_lookup_stays_silent() {
    assert_eq!(
        symbols(OBJC_LITERAL_LOOKUP, Lang::ObjC, "case.m"),
        Vec::<String>::new()
    );
}

#[test]
fn objc_literal_selector_stays_silent() {
    assert_eq!(
        symbols(OBJC_LITERAL_SELECTOR, Lang::ObjC, "case.m"),
        Vec::<String>::new()
    );
}

#[test]
fn objc_variable_selector_still_flags() {
    assert_eq!(
        symbols(OBJC_VARIABLE_SELECTOR, Lang::ObjC, "case.m").len(),
        1
    );
}
const C_LITERAL: &str = r#"#include <dlfcn.h>
void* load(void* h) { return dlsym(h, "symbol_name"); }
"#;
const C_VARIABLE: &str = r#"#include <dlfcn.h>
void* load(void* h, const char* name) { return dlsym(h, name); }
"#;
const C_ADJACENT: &str = r#"#include <dlfcn.h>
void* load(void* h) { return dlsym(h, "sym" "bol_name"); }
"#;
const CPP_ADJACENT: &str = r#"#include <dlfcn.h>
void* load(void* h) { return dlsym(h, "sym" "bol_name"); }
"#;
const C_MACRO_NAME: &str = r#"#include <dlfcn.h>
#define SYM "symbol_name"
void* load(void* h) { return dlsym(h, SYM); }
"#;
#[test]
fn c_literal_stays_silent() {
    assert_eq!(
        symbols(C_LITERAL, Lang::C, "case.c"),
        Vec::<String>::new(),
        "C literal reported 1 before fix"
    );
}
#[test]
fn c_variable_still_flags() {
    assert_eq!(
        symbols(C_VARIABLE, Lang::C, "case.c").len(),
        1,
        "C variable keeps 1 dynamic symbol"
    );
}
#[test]
fn c_adjacent_stays_silent() {
    assert_eq!(
        symbols(C_ADJACENT, Lang::C, "case.c"),
        Vec::<String>::new(),
        "C adjacent literals reported 1 before fix"
    );
}
#[test]
fn cpp_adjacent_stays_silent() {
    assert_eq!(
        symbols(CPP_ADJACENT, Lang::Cpp, "case.cpp"),
        Vec::<String>::new(),
        "C++ adjacent literals reported 1 before fix"
    );
}
#[test]
fn macro_identifier_still_flags() {
    assert_eq!(
        symbols(C_MACRO_NAME, Lang::C, "case.c").len(),
        1,
        "macro identifier keeps 1 dynamic symbol"
    );
}
