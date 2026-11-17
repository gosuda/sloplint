//! Member-pointer typedefs: `typedef void (C::*name)(Args)` parses through the
//! vendored cpp grammar (scoped `*` inside a parenthesized type declarator). The
//! alias must land in `facts.types` under the typedef's own name — the walk used
//! to follow the qualified name's scope (the class) and drop the alias entirely.

use sloplint_core::lang::Lang;

use crate::support::run_clean;

const MEMBERPTR: &str = "struct dummy {};

typedef void (dummy::*safe_bool)();
typedef void (dummy::*safe_bool2)(int);
";

#[test]
fn a_member_pointer_typedef_yields_its_alias_name() {
    let analysis = run_clean(Lang::Cpp, "include/safe_bool.h", MEMBERPTR);
    let types: Vec<&str> = analysis.facts.types.iter().map(|t| t.name).collect();
    for alias in ["dummy", "safe_bool", "safe_bool2"] {
        assert!(types.contains(&alias), "{alias} is a type: {types:?}");
    }
}
