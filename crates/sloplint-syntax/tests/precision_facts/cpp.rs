use sloplint_core::facts::{
    ArgShape, AssertShape, FileFacts, ImportKind, ImportScope, ScrutineeKind, Visibility,
};
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
fn out_of_class_definitions_are_methods() {
    with_facts(
        "struct Button {\n    void resize(int w);\n};\nvoid Button::resize(int w) { return; }\nvoid render(int w) { return; }\n",
        |facts| {
            let resize = function(facts, "resize");
            assert!(
                resize.is_method,
                "qualified declarator lives in a type scope"
            );
            assert_eq!(
                resize.visibility,
                Visibility::Exported,
                "out-of-line member keeps its access"
            );
            let render = function(facts, "render");
            assert!(!render.is_method, "free function stays a free function");
        },
    );
}

#[test]
fn file_scope_main_is_the_entry_point() {
    with_facts(
        "int main(int argc, char **argv) { return 0; }\nint helper() { return 1; }\n",
        |facts| {
            let main = function(facts, "main");
            assert!(main.entry_point, "file-scope main is the program entry");
            assert_eq!(
                main.visibility,
                Visibility::Exported,
                "entry stays externally visible"
            );
            let helper = function(facts, "helper");
            assert!(!helper.entry_point, "ordinary function is not an entry");
        },
    );
}

#[test]
fn virtual_and_override_force_signatures() {
    with_facts(
        "struct Base {\n    virtual void process(bool a) { return; }\n    void plain(bool a) { return; }\n    void overrides(bool a) override { return; }\n};\nstruct Derived : Base {\n    virtual void process(bool a) { return; }\n};\n",
        |facts| {
            let mut slots: Vec<bool> = facts
                .functions
                .iter()
                .filter(|f| f.name == "process")
                .map(|f| f.signature_forced)
                .collect();
            slots.sort_unstable();
            assert_eq!(
                slots,
                vec![true, true],
                "base slot and pre-C++11 re-declaration both force"
            );
            assert!(
                !function(facts, "plain").signature_forced,
                "plain member keeps a free signature"
            );
            assert!(
                function(facts, "overrides").signature_forced,
                "override states the inherited signature"
            );
        },
    );
}

#[test]
fn qualified_and_member_callees_are_simple_paths() {
    with_facts(
        "double compute(int a, int b) { return a + b; }\ndouble area(int s, int k) { return ns::compute(s, k); }\nint run(int a);\nint call(int a) { return run(a); }\nint outer(int x) { return make()(x); }\n",
        |facts| {
            let qualified = facts
                .calls
                .iter()
                .find(|c| c.callee == "ns::compute")
                .expect("has qualified call");
            assert!(
                qualified.is_simple_path,
                "scope-joined names are a simple path"
            );
            assert_eq!(
                qualified.callee_root,
                Some("ns"),
                "qualified root is the leftmost namespace"
            );
            let member = facts
                .calls
                .iter()
                .find(|c| c.callee == "run")
                .expect("has bare call");
            assert!(member.is_simple_path, "bare name is a simple path");
            assert_eq!(member.callee_root, Some("run"), "bare name roots at itself");
            let chained = facts
                .calls
                .iter()
                .find(|c| c.callee == "make" && !c.is_simple_path)
                .expect("has chained call");
            assert!(
                !chained.is_simple_path,
                "callee through a call stays non-simple"
            );
            assert_eq!(
                chained.callee_root, None,
                "chain bottoming at a call keeps no root"
            );
        },
    );
}

#[test]
fn new_expression_paths_depend_on_the_type() {
    with_facts(
        "struct Box { int v; };\nBox *plain(int x) { return new Box(x); }\nBox *scoped(int x) { return new ns::Box(x); }\nBox *bad(int x) { return new ns::Box<int>(x); }\n",
        |facts| {
            let plain = facts
                .calls
                .iter()
                .find(|c| c.callee == "Box")
                .expect("has plain new");
            assert!(plain.is_simple_path, "bare type is a plain path");
            let scoped = facts
                .calls
                .iter()
                .find(|c| c.callee == "ns::Box")
                .expect("has scoped new");
            assert!(
                scoped.is_simple_path,
                "identifier-only scope chain stays a path"
            );
            let bad = facts
                .calls
                .iter()
                .find(|c| c.callee == "ns::Box<int>")
                .expect("has template new");
            assert!(
                !bad.is_simple_path,
                "template type is not a plain identifier path"
            );
        },
    );
}

#[test]
fn adjacent_literals_measure_as_one_run() {
    with_facts(
        "void log_error(const char *msg);\nvoid f() { log_error(\"abc \" \"defgh\"); }\nvoid g() { log_error(\"abc\"); }\n",
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
            assert_eq!(runs, vec![3, 9], "adjacent pieces join before measuring");
        },
    );
}

#[test]
fn escape_breaks_the_joined_run() {
    with_facts(
        "void log_error(const char *msg);\nvoid f() { log_error(\"abcd\\n\" \"efgh\"); }\n",
        |facts| {
            let call = facts
                .calls
                .iter()
                .find(|c| c.callee == "log_error")
                .expect("has log call");
            match &call.args[0].shape {
                ArgShape::Str { longest_static } => assert_eq!(
                    *longest_static, 4,
                    "the escape ends the run across the join"
                ),
                _ => panic!("string argument stays a string"),
            }
        },
    );
}

#[test]
fn switch_defaults_carry_scrutinee_and_label_counts() {
    with_facts(
        "int classify(int color) {\n    switch (color) {\n        case 1: return 1;\n        case 2: return 2;\n        default: return 0;\n    }\n}\nint single(int mode) {\n    switch (mode) {\n        case 4: return 1;\n        default: return 0;\n    }\n}\nenum Mode { A, B };\nint one(Mode mode) {\n    switch (mode) {\n        case A: return 1;\n        default: return 0;\n    }\n}\nint two(Mode mode) {\n    switch (mode) {\n        case A: return 1;\n        case B: return 2;\n        default: return 0;\n    }\n}\n",
        |facts| {
            assert_eq!(facts.wildcard_arms.len(), 4, "one arm per default");
            let numeric = &facts.wildcard_arms[0];
            assert_eq!(
                numeric.scrutinee,
                ScrutineeKind::Ident,
                "switch over a variable names its scrutinee"
            );
            assert_eq!(numeric.variant_count, 2, "both case labels count");
            assert!(
                !numeric.enum_like,
                "integer labels are not an enum-like arm"
            );
            let lone = &facts.wildcard_arms[1];
            assert_eq!(lone.variant_count, 1, "single label counts once");
            let one_ident = &facts.wildcard_arms[2];
            assert!(
                !one_ident.enum_like,
                "one identifier label is not enum-like"
            );
            let two_idents = &facts.wildcard_arms[3];
            assert!(two_idents.enum_like, "two identifier labels are enum-like");
            assert_eq!(two_idents.variant_count, 2, "both identifier labels count");
        },
    );
}

#[test]
fn abstract_mid_counts_as_abstract_child() {
    with_facts(
        "struct Iface { virtual void f() = 0; };\nstruct Mid : Iface { virtual void g() = 0; };\nstruct Leaf : Mid { void f() override { return; } void g() override { return; } };\n",
        |facts| {
            let base = facts
                .impls
                .iter()
                .find(|i| i.name == "Iface")
                .expect("has Iface impl");
            assert!(
                base.child_is_abstract,
                "pure-virtual mid stays an abstract child"
            );
            let mid = facts
                .impls
                .iter()
                .find(|i| i.name == "Mid")
                .expect("has Mid impl");
            assert!(
                !mid.child_is_abstract,
                "concrete leaf stays a concrete child"
            );
        },
    );
}

#[test]
fn runtime_lookup_matches_bare_names() {
    with_facts(
        "#include <dlfcn.h>\nvoid *load(void *h, const char *sym) { return dlsym(h, sym); }\nvoid logx() { printf(\"x\"); }\n",
        |facts| {
            assert!(
                facts
                    .dynamic_symbols
                    .iter()
                    .any(|d| d.what.contains("dlsym")),
                "bare dlsym keeps its finding"
            );
            assert_eq!(
                facts.dynamic_symbols.len(),
                1,
                "ordinary calls report no dynamic symbol"
            );
        },
    );
}

#[test]
fn relational_asserts_are_not_equality() {
    with_facts(
        "void checks(int a, int b, int c, int d, int n, int m) {\n    EXPECT_EQ(a, b);\n    EXPECT_LT(c, d);\n    REQUIRE(n == 1);\n    REQUIRE(m < 10);\n}\n",
        |facts| {
            assert_eq!(facts.assertions.len(), 4, "one assertion per macro");
            assert_eq!(
                facts.assertions[0].shape,
                AssertShape::TopEqNe,
                "EQ stays an equality shape"
            );
            assert!(facts.assertions[0].comparison, "EQ stays a comparison");
            assert_eq!(
                facts.assertions[1].shape,
                AssertShape::Other,
                "relational gtest is not equality"
            );
            assert!(
                !facts.assertions[1].comparison,
                "relational gtest is not a comparison"
            );
            assert_eq!(
                facts.assertions[2].shape,
                AssertShape::TopEqNe,
                "double-equals stays an equality shape"
            );
            assert_eq!(
                facts.assertions[3].shape,
                AssertShape::Other,
                "relational Catch2 is not equality"
            );
        },
    );
}

#[test]
fn using_inside_a_function_is_function_local() {
    with_facts(
        "using namespace std;\nvoid f() { using namespace detail; }\nvoid g() { using std::string; }\n",
        |facts| {
            let top = facts
                .imports
                .iter()
                .find(|i| i.module == "std" && i.kind == ImportKind::Wildcard)
                .expect("has top-level using-directive");
            assert_eq!(
                top.scope,
                ImportScope::Module,
                "file-scope using stays module scope"
            );
            let inner = facts
                .imports
                .iter()
                .find(|i| i.module == "detail")
                .expect("has function using-directive");
            assert_eq!(
                inner.scope,
                ImportScope::FunctionLocal,
                "using inside a body is function-local"
            );
            let member = facts
                .imports
                .iter()
                .find(|i| i.name == "string")
                .expect("has function using-declaration");
            assert_eq!(
                member.scope,
                ImportScope::FunctionLocal,
                "using-declaration inside a body is function-local"
            );
        },
    );
}

#[test]
fn namespace_qualified_definitions_stay_free() {
    with_facts(
        "namespace util {\n    void helper(int x);\n}\nvoid util::reconcile(int x) { return; }\nstruct Button {\n    void resize(int w);\n};\nvoid Button::resize(int w) { return; }\n",
        |facts| {
            let free = function(facts, "reconcile");
            assert!(!free.is_method, "namespace scope is not a type scope");
            let member = function(facts, "resize");
            assert!(member.is_method, "class scope declared in file is a method");
        },
    );
}

#[test]
fn constexpr_labels_are_not_enum_like() {
    with_facts(
        "constexpr int OP_ADD = 1;\nconstexpr int OP_SUB = 2;\nint apply(int op, int a, int b) {\n    switch (op) {\n        case OP_ADD: return a + b;\n        case OP_SUB: return a - b;\n        default: return 0;\n    }\n}\n",
        |facts| {
            assert_eq!(facts.wildcard_arms.len(), 1, "one default arm");
            let arm = &facts.wildcard_arms[0];
            assert_eq!(
                arm.scrutinee,
                ScrutineeKind::Ident,
                "switch over a variable names its scrutinee"
            );
            assert_eq!(arm.variant_count, 2, "both case labels count");
            assert!(!arm.enum_like, "integer constants are not an enum-like arm");
        },
    );
}

#[test]
fn test_macros_mark_their_bodies() {
    with_facts(
        "int add(int a, int b) { return a + b; }\nTEST(AdderSuite, AddsPositive) { EXPECT_EQ(add(2, 3), 5); }\nTEST_F(FixtureSuite, HandlesZero) { EXPECT_EQ(add(0, 0), 0); }\n",
        |facts| {
            let plain = function(facts, "add");
            assert!(!plain.in_test, "ordinary function stays out of tests");
            let gtest = function(facts, "TEST");
            assert!(gtest.in_test, "TEST body is a test region");
            let fixture = function(facts, "TEST_F");
            assert!(fixture.in_test, "TEST_F body is a test region");
            assert!(
                facts
                    .regions
                    .iter()
                    .any(|region| region.role == CodeRole::Test),
                "macro bodies push a Test region"
            );
        },
    );
}

#[test]
fn plus_concatenations_carry_static_runs() {
    with_facts(
        "void log(const char *msg);\nvoid f(const char *name) {\n    log(\"Hello, \" + \"wonderful world\");\n    log(\"ab\" + name);\n    log(\"\" + name);\n}\n",
        |facts| {
            let mut runs = Vec::new();
            for c in facts.calls.iter().filter(|c| c.callee == "log") {
                match &c.args[0].shape {
                    ArgShape::Concat { longest_static } => runs.push(*longest_static),
                    _ => panic!("plus stays a concatenation"),
                }
            }
            assert_eq!(runs, vec![15, 2, 0], "long, short, and empty runs");
        },
    );
}
