//! Qt's `Q_OBJECT`/`Q_GADGET` leave `private:` in force. Unreal's modern
//! `GENERATED_BODY` and its `GENERATED_USTRUCT_BODY` alias preserve access;
//! the legacy class and interface macros leave `public:` in force.
//!
//! `Q_FOREACH (type var, container) body` is a range-for in disguise: the
//! variable must bind so a write inside the body cannot be charged to an
//! outer name, and the body must count as a loop body for depth tracking.

use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn function_visibility(source: &str, name: &str) -> Visibility {
    let analysis = run_clean(Lang::Cpp, "widget.cpp", source);
    analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("missing {name} in {:?}", analysis.facts.functions))
        .visibility
}

/// `struct` defaults to public, so a member reading private after Q_GADGET
/// can only come from the macro's own `private:` transition.
const GADGET_STRUCT: &str = "struct PointGadget {\n\
    Q_GADGET\n\
    void metadata() {}\n\
public:\n\
    void api() {}\n\
};\n";

const OBJECT_CLASS: &str = "class Widget {\n\
    Q_OBJECT\n\
    void internal() {}\n\
public:\n\
    void api() {}\n\
};\n";

#[test]
fn q_gadget_marks_following_members_private() {
    assert_eq!(
        function_visibility(GADGET_STRUCT, "metadata"),
        Visibility::Private,
        "Q_GADGET ends in private:, was Exported when the macro was ignored"
    );
    assert_eq!(
        function_visibility(GADGET_STRUCT, "api"),
        Visibility::Exported
    );
}

#[test]
fn generated_body_preserves_member_access() {
    for (macro_name, kind, default_access) in [
        ("GENERATED_BODY", "class", Visibility::Private),
        ("GENERATED_BODY", "struct", Visibility::Exported),
        ("GENERATED_USTRUCT_BODY", "struct", Visibility::Exported),
    ] {
        for (label, access) in [
            ("", default_access),
            ("private:", Visibility::Private),
            ("public:", Visibility::Exported),
        ] {
            let source = format!(
                "{kind} Thing {{\n\
                    {label}\n\
                    {macro_name}()\n\
                    void helper() {{}}\n\
                    void separate();\n\
                public:\n\
                    void api() {{}}\n\
                    void separate_api();\n\
                }};\n\
                void Thing::separate() {{}}\n\
                void Thing::separate_api() {{}}\n"
            );
            let analysis = run_clean(Lang::Cpp, "thing.cpp", &source);
            let functions: Vec<_> = analysis
                .facts
                .functions
                .iter()
                .map(|function| (function.name, function.visibility))
                .collect();
            assert_eq!(
                functions,
                [
                    ("helper", access),
                    ("api", Visibility::Exported),
                    ("separate", access),
                    ("separate_api", Visibility::Exported),
                ],
                "{macro_name} in {kind} after {label:?}"
            );
        }
    }
}

#[test]
fn legacy_unreal_macros_start_public_member_sections() {
    for macro_name in [
        "GENERATED_UCLASS_BODY",
        "GENERATED_UINTERFACE_BODY",
        "GENERATED_IINTERFACE_BODY",
    ] {
        let source = format!(
            "class Thing {{\n\
                {macro_name}()\n\
                void api() {{}}\n\
            private:\n\
                void helper() {{}}\n\
            }};\n"
        );
        assert_eq!(function_visibility(&source, "api"), Visibility::Exported);
        assert_eq!(function_visibility(&source, "helper"), Visibility::Private);
    }
}

#[test]
fn q_object_marks_following_members_private() {
    assert_eq!(
        function_visibility(OBJECT_CLASS, "internal"),
        Visibility::Private
    );
    assert_eq!(
        function_visibility(OBJECT_CLASS, "api"),
        Visibility::Exported
    );
}

/// The iterator declared by Q_FOREACH binds inside the loop: a write there
/// is not charged to a same-named outer variable, and the body counts as a
/// loop body for depth tracking.
const FOREACH_SHADOW: &str = "void f(std::vector<int> xs) {\n\
    int x = 0;\n\
    Q_FOREACH (auto x, xs) {\n\
        x = 1;\n\
    }\n\
    x = 2;\n\
}\n";

#[test]
fn q_foreach_variable_stays_outside_outer_binding() {
    let analysis = run_clean(Lang::Cpp, "iter.cpp", FOREACH_SHADOW);
    let xs: Vec<_> = analysis
        .facts
        .bindings
        .iter()
        .filter(|b| b.name == "x")
        .collect();
    assert_eq!(
        xs.len(),
        1,
        "the Q_FOREACH variable creates no binding: {:?}",
        analysis.facts.bindings
    );
    assert_eq!(
        (
            xs[0].write_lines,
            xs[0].first_write_line,
            xs[0].last_write_line
        ),
        (1, 6, 6),
        "the loop write is charged to the iterator, not the outer x: {:?}",
        analysis.facts.bindings
    );
    assert_eq!(
        xs[0].loop_depth, 0,
        "outer x carries no loop depth from the misattributed write"
    );
}
