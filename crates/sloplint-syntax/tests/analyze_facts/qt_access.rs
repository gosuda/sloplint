//! Meta-object generator macros inject declarations that end in an access
//! specifier: Qt's `Q_OBJECT`/`Q_GADGET` leave `private:` in force and
//! Unreal's legacy `GENERATED_*_BODY` macros leave `public:` — while UE5's
//! `GENERATED_BODY` keeps the incoming access. A member declared after one
//! of those macros without an explicit label takes that injected access —
//! the macro is itself the transition, not a no-op statement.
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

/// `class` defaults to private: UE5's GENERATED_BODY keeps that access,
/// while the legacy GENERATED_UCLASS_BODY ends in `public:`.
const UNREAL_CLASS: &str = "class UThing {\n\
    GENERATED_BODY()\n\
    void helper() {}\n\
};\n";

const UNREAL_LEGACY_CLASS: &str = "class ULegacy {\n\
    GENERATED_UCLASS_BODY()\n\
    void helper() {}\n\
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
fn generated_body_keeps_incoming_access() {
    assert_eq!(
        function_visibility(UNREAL_CLASS, "helper"),
        Visibility::Private,
        "UE5 GENERATED_BODY preserves incoming access, was Exported"
    );
}

#[test]
fn generated_uclass_body_marks_following_members_public() {
    assert_eq!(
        function_visibility(UNREAL_LEGACY_CLASS, "helper"),
        Visibility::Exported,
        "GENERATED_UCLASS_BODY ends in public:, was Private when the macro was ignored"
    );
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
