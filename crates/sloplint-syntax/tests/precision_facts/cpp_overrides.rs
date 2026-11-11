//! Override-forced signatures, out-of-line definition access, and C++ call and literal shapes.

use sloplint_core::facts::{ArgShape, FileFacts, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn with_facts(source: &str, check: impl FnOnce(&FileFacts<'_>)) {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Cpp,
        path: "case.cpp",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "parses");
    check(&analysis.facts);
}

fn function<'a>(facts: &'a FileFacts<'a>, name: &str) -> &'a sloplint_core::facts::Function<'a> {
    facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("has function {name}"))
}

#[test]
fn braced_arguments_split_keyed_code_and_plain() {
    with_facts(
        "void take(int x);\nvoid f() {\n    take({.x = 1, .y = 2});\n    take({1, [](int x) { return x; }});\n    take({1, 2, 3});\n}\n",
        |facts| {
            assert_eq!(facts.calls.len(), 3, "three braced calls");
            assert_eq!(
                facts.calls[0].args[0].shape,
                ArgShape::Other,
                "keyed literal is not a collection"
            );
            assert_eq!(
                facts.calls[1].args[0].shape,
                ArgShape::CodeBody,
                "collection holding a closure is a code body"
            );
            assert_eq!(
                facts.calls[2].args[0].shape,
                ArgShape::Collection,
                "plain braced list stays a collection"
            );
        },
    );
}

#[test]
fn abstract_override_counts_as_abstract_child() {
    with_facts(
        "struct Iface { virtual void f() = 0; };\nstruct Mid : Iface { virtual void f() = 0; };\nstruct Impl : Mid { void f() override {} };\n",
        |facts| {
            let mid = facts
                .impls
                .iter()
                .find(|i| i.name == "Iface")
                .expect("has Iface impl");
            assert!(
                mid.child_is_abstract,
                "pure-virtual overrider stays an abstract child"
            );
            let leaf = facts
                .impls
                .iter()
                .find(|i| i.name == "Mid")
                .expect("has Mid impl");
            assert!(
                !leaf.child_is_abstract,
                "concrete leaf stays a concrete child"
            );
        },
    );
}

#[test]
fn joined_and_single_literals_measure_the_same_text() {
    with_facts(
        "void log_error(const char *msg);\nvoid f() { log_error(\"ab\\x41\" \"ghijkl\"); }\nvoid g() { log_error(\"ab\\x41ghijkl\"); }\n",
        |facts| {
            let mut runs: Vec<u32> = facts
                .calls
                .iter()
                .filter(|c| c.callee == "log_error")
                .map(|c| match &c.args[0].shape {
                    ArgShape::Str { longest_static } => *longest_static,
                    _ => panic!("string argument stays a string"),
                })
                .collect();
            runs.sort_unstable();
            assert_eq!(
                runs,
                vec![6, 6],
                "escape payload never counts as static text"
            );
        },
    );
}

#[test]
fn adjacent_literals_join_before_a_macro_name() {
    with_facts(
        "void log_error(const char *msg);\nvoid f() { log_error(\"abc \" \"defg \" MSG); }\n",
        |facts| {
            let call = facts
                .calls
                .iter()
                .find(|c| c.callee == "log_error")
                .expect("has log call");
            match &call.args[0].shape {
                ArgShape::Concat { longest_static } => {
                    assert_eq!(*longest_static, 9, "literals join across the macro tail")
                }
                _ => panic!("macro-tailed strings stay a concatenation"),
            }
        },
    );
}

#[test]
fn plus_accepts_a_joined_literal_operand() {
    with_facts(
        "void log_error(const char *msg);\nvoid f(const char *path) { log_error(\"ab \" \"cd \" + path); }\n",
        |facts| {
            let call = facts
                .calls
                .iter()
                .find(|c| c.callee == "log_error")
                .expect("has log call");
            match &call.args[0].shape {
                ArgShape::Concat { longest_static } => {
                    assert_eq!(*longest_static, 6, "joined run survives the plus")
                }
                _ => panic!("plus over literals stays a concatenation"),
            }
        },
    );
}

#[test]
fn receiver_chains_name_the_member_alone() {
    with_facts(
        "struct Logger { int run(int x); };\nLogger make(Logger &l);\nint f(Logger &l) { return make(l).run(2); }\n",
        |facts| {
            let chained = facts
                .calls
                .iter()
                .find(|c| c.callee == "run")
                .expect("has chained call");
            assert!(
                !chained.is_simple_path,
                "chain through a call stays non-simple"
            );
            assert_eq!(
                chained.callee_root, None,
                "chain bottoming at a call keeps no root"
            );
            let inner = facts
                .calls
                .iter()
                .find(|c| c.callee == "make")
                .expect("has inner call");
            assert!(inner.is_simple_path, "inner call stays a simple path");
        },
    );
}

#[test]
fn out_of_line_overriders_keep_the_forced_signature() {
    with_facts(
        "struct Base { virtual void process(bool a, bool b); };\nstruct Derived : Base { void process(bool a, bool b) override; };\nvoid Derived::process(bool a, bool b) { (void)a; (void)b; }\nvoid free(bool a, bool b) { (void)a; (void)b; }\n",
        |facts| {
            assert!(
                function(facts, "process").signature_forced,
                "override dictates the out-of-line signature"
            );
            assert!(
                !function(facts, "free").signature_forced,
                "free function keeps a free signature"
            );
        },
    );
}

#[test]
fn out_of_line_definitions_keep_their_declared_access() {
    with_facts(
        "class Impl { void apply(bool a, bool b); };\nvoid Impl::apply(bool a, bool b) { (void)a; (void)b; }\nclass Open { public: void run(bool a, bool b); };\nvoid Open::run(bool a, bool b) { (void)a; (void)b; }\n",
        |facts| {
            assert_eq!(
                function(facts, "apply").visibility,
                Visibility::Private,
                "private member stays private out of line"
            );
            assert_eq!(
                function(facts, "run").visibility,
                Visibility::Exported,
                "public member stays exported out of line"
            );
        },
    );
}

#[test]
fn shape_and_circle_emit_the_abstraction_and_its_impl() {
    with_facts(
        "namespace {\nclass Shape {\npublic:\n    virtual void draw() = 0;\n};\n}\nstruct Circle : Shape {\n    void draw();\n};\n",
        |facts| {
            let shape = facts
                .types
                .iter()
                .find(|t| t.name == "Shape")
                .expect("has Shape type");
            assert!(shape.abstraction, "pure virtual marks the abstraction");
            assert_eq!(
                shape.visibility,
                Visibility::Private,
                "anonymous namespace stays private"
            );
            let imp = facts
                .impls
                .iter()
                .find(|i| i.name == "Shape")
                .expect("has Shape impl");
            assert!(
                !imp.child_is_abstract,
                "body-less declarator stays a concrete child"
            );
            assert!(!imp.is_generic, "plain base stays non-generic");
        },
    );
}
