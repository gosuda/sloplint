//! C++ member access follows the nearest preceding access label in the member's own
//! field list, with `struct` defaulting to public and `class` to private.
//!
//! Each case pins one rule of the label walk: label switches, templates, macros between
//! members, conditional blocks that leave the field list, statics, anonymous namespaces,
//! and nested classes, observed through `analyze` only.

use crate::support::with_facts;
use sloplint_core::lang::Lang;

/// `(name, public)` for every function in `source`, sorted by name.
fn publicity(source: &str) -> Vec<(String, bool)> {
    with_facts(Lang::Cpp, "case.cpp", source, |facts| {
        let mut out: Vec<(String, bool)> = facts
            .functions
            .iter()
            .map(|f| (f.name.to_owned(), f.public))
            .collect();
        out.sort();
        out
    })
}

/// The expected `(name, public)` pairs, sorted by name.
fn expected(pairs: &[(&str, bool)]) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = pairs
        .iter()
        .map(|(name, public)| ((*name).to_owned(), *public))
        .collect();
    out.sort();
    out
}

#[test]
fn struct_defaults_to_public_and_class_to_private() {
    let source = r#"struct Plain {
    void default_struct(bool a) { (void)a; }
};
class Hidden {
    void default_class(bool a) { (void)a; }
};
"#;
    assert_eq!(
        publicity(source),
        expected(&[("default_struct", true), ("default_class", false)])
    );
}

#[test]
fn each_label_sets_access_for_the_members_after_it() {
    let source = r#"class Labels {
public:
    void first_public(bool a) { (void)a; }
private:
    void after_private(bool a) { (void)a; }
protected:
    void after_protected(bool a) { (void)a; }
public:
    void after_second_public(bool a) { (void)a; }
};
"#;
    assert_eq!(
        publicity(source),
        expected(&[
            ("first_public", true),
            ("after_private", false),
            ("after_protected", false),
            ("after_second_public", true),
        ])
    );
}

#[test]
fn struct_with_a_private_label_switches_to_private() {
    let source = r#"struct Mixed {
    void open_struct(bool a) { (void)a; }
private:
    void closed_struct(bool a) { (void)a; }
};
"#;
    assert_eq!(
        publicity(source),
        expected(&[("open_struct", true), ("closed_struct", false)])
    );
}

#[test]
fn template_members_take_the_access_of_their_template_declaration() {
    let source = r#"class Templated {
public:
    template <typename T>
    void open_template(T value) { (void)value; }
private:
    template <typename T>
    void closed_template(T value) { (void)value; }
};
"#;
    assert_eq!(
        publicity(source),
        expected(&[("open_template", true), ("closed_template", false)])
    );
}

#[test]
fn preprocessor_defines_between_members_keep_the_label_state() {
    let source = r#"class Macros {
public:
#define SHIFT 1
    void after_macro_public(bool a) { (void)a; }
private:
#define HIDE 2
    void after_macro_private(bool a) { (void)a; }
};
"#;
    assert_eq!(
        publicity(source),
        expected(&[("after_macro_public", true), ("after_macro_private", false),])
    );
}

#[test]
fn members_inside_a_conditional_block_ignore_the_class_label() {
    let source = r#"class Guarded {
private:
#ifdef FEATURE
    void in_ifdef(bool a) { (void)a; }
#endif
};
"#;
    assert_eq!(publicity(source), expected(&[("in_ifdef", true)]));
}

#[test]
fn statics_follow_labels_inside_classes_and_free_static_is_private() {
    let source = r#"static void static_free(bool a) { (void)a; }
void free_open(bool a) { (void)a; }
class Statics {
private:
    static void static_private_member(bool a) { (void)a; }
public:
    static void static_public_member(bool a) { (void)a; }
};
"#;
    assert_eq!(
        publicity(source),
        expected(&[
            ("static_free", false),
            ("free_open", true),
            ("static_private_member", false),
            ("static_public_member", true),
        ])
    );
}

#[test]
fn members_inside_an_anonymous_namespace_are_never_public() {
    let source = r#"namespace {
struct Hidden {
public:
    void in_anonymous_namespace(bool a) { (void)a; }
};
}
void outside_namespace(bool a) { (void)a; }
"#;
    assert_eq!(
        publicity(source),
        expected(&[
            ("in_anonymous_namespace", false),
            ("outside_namespace", true),
        ])
    );
}

#[test]
fn nested_class_members_use_their_own_field_list() {
    let source = r#"class Outer {
public:
    class Inner {
        void inner_default(bool a) { (void)a; }
    };
    void outer_public(bool a) { (void)a; }
};
"#;
    assert_eq!(
        publicity(source),
        expected(&[("inner_default", false), ("outer_public", true)])
    );
}
