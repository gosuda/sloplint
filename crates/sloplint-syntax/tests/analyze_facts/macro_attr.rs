//! Vendor headers pre-decorate declarations with calling-convention, storage and
//! compiler-attribute macros: `__IO uint32_t ISER[1];` in CMSIS,
//! `VKAPI_ATTR VkBool32 VKAPI_CALL cb(void *);` in Vulkan, `NAPI_CDECL f(int)` in
//! Node's headers, `int WINAPI wWinMain(...)` in Win32. Both vendored grammars carry
//! `macro_attribute`, a literal enumeration of those names; without it each
//! declaration died at the macro and yielded no facts. These tests pin the parse
//! through the public `analyze` surface.

use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

/// CMSIS/Vulkan/Win32 shapes: macro before the type, around the declarator, and
/// alone in the type position.
const DECORATED_C: &str = "__IO unsigned int ISER[1];\n\
__STATIC_INLINE int clip(int x) { return x < 0 ? 0 : x; }\n\
VKAPI_ATTR int VKAPI_CALL debug_cb(void *ud) { return ud != 0; }\n\
int WINAPI wWinMain(void *h, void *p, char *c, int s) { return s; }\n";

const DECORATED_CPP: &str = "extern \"C\" {\n\
VKAPI_ATTR unsigned int VKAPI_CALL vk_version(void) { return 1; }\n\
}\n";

/// `run_clean` already pins a zero-error parse; these assert the decorated
/// declarations still reach the fact walk.
#[test]
fn macro_decorated_c_declarations_parse_clean_and_yield_facts() {
    let analysis = run_clean(Lang::C, "include/board.h", DECORATED_C);
    assert_eq!(analysis.facts.lang, Lang::C);
    let fns: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    for want in ["clip", "debug_cb", "wWinMain"] {
        assert!(fns.contains(&want), "missing {want} in {fns:?}");
    }
    // `__STATIC_INLINE` is CMSIS for `static inline`: the helper is private,
    // unlike `VKAPI_ATTR`/`WINAPI` which carry no storage semantics.
    let clip = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "clip")
        .expect("clip is a function fact");
    assert_eq!(clip.visibility, Visibility::Private);
    let cb = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "debug_cb")
        .expect("debug_cb is a function fact");
    assert_eq!(cb.visibility, Visibility::Exported);
}

#[test]
fn macro_decorated_cpp_declarations_parse_clean_and_yield_facts() {
    let analysis = run_clean(Lang::Cpp, "include/api.hpp", DECORATED_CPP);
    assert_eq!(analysis.facts.lang, Lang::Cpp);
    let fns: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert!(fns.contains(&"vk_version"), "missing vk_version in {fns:?}");
}
