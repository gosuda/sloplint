//! Function visibility facts observed through `analyze` only.
//!
//! A top-level `fun` or `def` with the language's default public standing is
//! exported; a nested, block-local, or explicitly private one is not.

use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;

use super::audit_analyze;

#[test]
fn audit_local_function_visibility() {
    let nested = [
        (
            Lang::Kotlin,
            "a.kt",
            "fun total(xs: List<Int>): Int {\n fun twice(x: Int) = x * 2\n return xs.sumOf { twice(it) }\n}\n",
        ),
        (
            Lang::Scala,
            "A.scala",
            "object Calc {\n def total(xs: List[Int]): Int = {\n def twice(x: Int) = x * 2\n xs.map(twice).sum\n }\n}\n",
        ),
    ];
    for (lang, path, source) in nested {
        let a = audit_analyze(lang, path, source);
        assert!(
            a.parse_error.is_none(),
            "{path} local-function repro parses"
        );
        let total = a
            .facts
            .functions
            .iter()
            .find(|f| f.name == "total")
            .unwrap_or_else(|| panic!("{path} has total"));
        assert_eq!(
            total.visibility,
            Visibility::Exported,
            "{path} total stays public"
        );
        let twice = a
            .facts
            .functions
            .iter()
            .find(|f| f.name == "twice")
            .unwrap_or_else(|| panic!("{path} has twice"));
        assert_ne!(
            twice.visibility,
            Visibility::Exported,
            "{path} local twice is not public"
        );
    }
    let members = [
        (
            Lang::Kotlin,
            "C.kt",
            "class C {\n fun total(): Int = 1\n}\n",
            "total",
            true,
        ),
        (
            Lang::Kotlin,
            "P.kt",
            "class C {\n private fun twice(): Int = 1\n}\n",
            "twice",
            false,
        ),
        (
            Lang::Scala,
            "C.scala",
            "object C {\n def total(): Int = 1\n}\n",
            "total",
            true,
        ),
        (
            Lang::Scala,
            "P.scala",
            "class C {\n private def twice(): Int = 1\n}\n",
            "twice",
            false,
        ),
    ];
    for (lang, path, source, name, want) in members {
        let a = audit_analyze(lang, path, source);
        assert!(a.parse_error.is_none(), "{path} member parses");
        let f = a
            .facts
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{path} has {name}"));
        assert_eq!(
            f.visibility == Visibility::Exported,
            want,
            "{path} {name} exported is {want}"
        );
    }
    let block_locals = [
        (
            Lang::Kotlin,
            "I.kt",
            "class C {\n init {\n fun twice(x: Int) = x * 2\n }\n}\n",
            "twice",
        ),
        (
            Lang::Scala,
            "V.scala",
            "object C {\n val k = {\n def twice(x: Int) = x * 2\n twice(1)\n }\n}\n",
            "twice",
        ),
        (
            Lang::Kotlin,
            "L.kt",
            "fun total(): Int {\n val f = { fun twice(x: Int) = x * 2\n twice(1) }\n return 1\n}\n",
            "twice",
        ),
        (
            Lang::Kotlin,
            "N.kt",
            "fun total(): Int {\n class Local {\n fun twice(): Int = 1\n }\n return 1\n}\n",
            "twice",
        ),
        (
            Lang::Scala,
            "W.scala",
            "object C {\n val f = (x: Int) => {\n def twice(y: Int) = y * 2\n twice(x)\n }\n}\n",
            "twice",
        ),
        (
            Lang::Scala,
            "F.scala",
            "object C {\n def total(): Int = {\n class Local {\n def twice(): Int = 1\n }\n 1\n }\n}\n",
            "twice",
        ),
        (
            Lang::Kotlin,
            "O.kt",
            "fun total(): Int {\n object Local {\n fun twice(): Int = 1\n }\n return 1\n}\n",
            "twice",
        ),
        (
            Lang::Scala,
            "G.scala",
            "object C {\n def total(): Int = {\n object Local {\n def twice(): Int = 1\n }\n 1\n }\n}\n",
            "twice",
        ),
    ];
    for (lang, path, source, name) in block_locals {
        let a = audit_analyze(lang, path, source);
        assert!(a.parse_error.is_none(), "{path} block-local repro parses");
        let f = a
            .facts
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{path} has {name}"));
        assert_ne!(
            f.visibility,
            Visibility::Exported,
            "{path} block-local {name} is not public"
        );
    }
}
