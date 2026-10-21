//! C++ operator overloads are functions: they get complexity metrics, and because the
//! language fixes their names and signatures they are interface-bound.
//!
//! The metric formula is the one in `cognitive_depth.rs`: an `if` costs `1 + nest` and
//! opens a level, so an `if` nested in an `if` scores `1 + 2` at depth `2`.

use crate::support::with_facts;
use sloplint_core::lang::Lang;

const SOURCE: &str = "struct T {
    int a;
    bool operator==(const T& o) const {
        if (a) {
            if (o.a) {
                return true;
            }
        }
        return false;
    }
    int& operator[](int i) {
        return a;
    }
    bool operator<(const T& o) const;
};

bool T::operator<(const T& o) const {
    if (a) {
        if (o.a) {
            return true;
        }
    }
    return false;
}

bool operator!=(const T& l, const T& r) {
    return !(l == r);
}

bool plain(const T& o) {
    if (o.a) {
        if (o.a) {
            return true;
        }
    }
    return false;
}
";

/// `(name, cognitive, max_depth, interface_bound)` of every function in `SOURCE`.
fn functions() -> Vec<(String, u32, u32, bool)> {
    with_facts(Lang::Cpp, "case.cpp", SOURCE, |facts| {
        facts
            .functions
            .iter()
            .map(|f| {
                (
                    f.name.to_string(),
                    f.cognitive,
                    f.max_depth,
                    f.interface_bound,
                )
            })
            .collect()
    })
}

fn find(all: &[(String, u32, u32, bool)], name: &str) -> (u32, u32, bool) {
    let found = all
        .iter()
        .find(|f| f.0 == name)
        .unwrap_or_else(|| panic!("no function {name} in {all:?}"));
    (found.1, found.2, found.3)
}

#[test]
fn operator_definitions_are_functions_in_every_declarator_position() {
    let all = functions();
    let mut names: Vec<&str> = all.iter().map(|f| f.0.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "operator!=",
            "operator<",
            "operator==",
            "operator[]",
            "plain"
        ],
        "member, subscript, out-of-line qualified, and free operators"
    );
}

#[test]
fn operator_bodies_are_scored_like_any_other_function() {
    let all = functions();
    for name in ["operator==", "operator<"] {
        assert_eq!(
            (find(&all, name).0, find(&all, name).1),
            (3, 2),
            "{name} holds an `if` inside an `if`"
        );
    }
    assert_eq!(
        (find(&all, "plain").0, find(&all, "plain").1),
        (3, 2),
        "the same body in a plain function is the control"
    );
}

#[test]
fn operators_are_interface_bound_and_plain_functions_are_not() {
    let all = functions();
    for name in ["operator==", "operator[]", "operator<", "operator!="] {
        assert!(
            find(&all, name).2,
            "{name} is dictated by the operator protocol"
        );
    }
    assert!(
        !find(&all, "plain").2,
        "a free function is not interface-bound"
    );
}
