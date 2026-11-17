//! Shared C/C++ headers write `extern "C"` inside preproc guards: the opening brace lives
//! inside one `#ifdef __cplusplus` block and the closing brace inside a later one, so
//! neither block parses on its own. Both vendored grammars carry a `guarded_linkage` rule
//! for the idiom; without it every such header died at the include guard and yielded no
//! facts at all. These tests pin the parse through the public `analyze` surface.

use sloplint_core::lang::Lang;

use crate::support::run_clean;

/// The canonical shared-header shape from real C and C++ libraries: prototypes and an
/// `static inline` definition between the guarded braces.
const GUARDED: &str = "#ifndef MY_API_H
#define MY_API_H

#ifdef __cplusplus
extern \"C\" {
#endif

int api_open(const char *path);
void api_close(int fd);
static inline int api_ok(int rc) { return rc == 0; }

#ifdef __cplusplus
}
#endif

#endif  // MY_API_H
";

/// The same linkage split across `#if defined` instead of `#ifdef`.
const GUARDED_DEFINED: &str = "#if defined(__cplusplus)
extern \"C\" {
#endif

static inline int api_version(void) { return 3; }

#if defined(__cplusplus)
}
#endif
";

/// `run_clean` already pins a zero-error parse; these assert the linkage body's contents
/// still reach the fact walk.
#[test]
fn a_guarded_c_linkage_block_parses_as_c_and_yields_its_definitions() {
    let analysis = run_clean(Lang::C, "include/api.h", GUARDED);
    assert_eq!(analysis.facts.lang, Lang::C);
    let fns: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert!(
        fns.contains(&"api_ok"),
        "api_ok is a function inside the guard: {fns:?}"
    );
}

#[test]
fn a_guarded_cpp_linkage_block_parses_as_cpp_and_yields_its_definitions() {
    let analysis = run_clean(Lang::Cpp, "include/api.hpp", GUARDED);
    assert_eq!(analysis.facts.lang, Lang::Cpp);
    let fns: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert!(
        fns.contains(&"api_ok"),
        "api_ok is a function inside the guard: {fns:?}"
    );
}

#[test]
fn a_guarded_linkage_split_over_if_defined_also_parses() {
    let analysis = run_clean(Lang::C, "include/version.h", GUARDED_DEFINED);
    let fns: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert!(
        fns.contains(&"api_version"),
        "api_version is a function inside the guard: {fns:?}"
    );
}

/// Code after the closing guard stays reachable: a definition after the `#endif` pairs.
#[test]
fn code_after_the_closing_guard_is_still_walked() {
    let source = "#ifdef __cplusplus
extern \"C\" {
#endif

int api_open(void);

#ifdef __cplusplus
}
#endif

static int outside(void) { return 1; }
";
    let analysis = run_clean(Lang::C, "include/mixed.h", source);
    let fns: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert!(
        fns.contains(&"outside"),
        "outside is a function after the guard: {fns:?}"
    );
}
