//! Odin initializer and sole-call facts: typed declarations, attributes,
//! multi-value returns.
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn with_facts<R>(
    path: &str,
    source: &str,
    read: impl FnOnce(&sloplint_core::facts::FileFacts<'_>) -> R,
) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: sloplint_core::lang::Lang::Odin,
        path,
        source,
        role: sloplint_core::role::CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "{path} parses");
    read(&analysis.facts)
}

#[test]
fn typed_and_attributed_declarations_initialize_plain_assign_does_not() {
    let typed = with_facts(
        "typed.odin",
        "package p\nrun :: proc() {\n\ttable: Table = make_table(1)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "make_table")
                .map(|c| c.initializes_binding)
                .expect("the typed initializer is recorded")
        },
    );
    assert!(
        typed,
        "`table: Table = make_table(1)` initializes the binding"
    );
    let attributed = with_facts(
        "attr.odin",
        "package p\nrun :: proc() {\n\t@(private)\n\ttable := make_table(1)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "make_table")
                .map(|c| c.initializes_binding)
                .expect("the attributed initializer is recorded")
        },
    );
    assert!(
        attributed,
        "attributes before the declaration hide no initializer"
    );
    let plain = with_facts(
        "assign.odin",
        "package p\nrun :: proc() {\n\ttable = make_table(1)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "make_table")
                .map(|c| c.initializes_binding)
                .expect("the assigned call is recorded")
        },
    );
    assert!(!plain, "a plain `=` reassigns, it initializes nothing");
}

#[test]
fn multi_value_return_is_no_sole_call_single_call_is() {
    let multi = with_facts(
        "wrap.odin",
        "package p\nwrap :: proc(n: int) -> (int, bool) {\n\treturn compute(n), true\n}\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "wrap")
                .map(|f| f.sole_call)
                .expect("`wrap` is recorded")
        },
    );
    assert_eq!(
        multi, None,
        "a two-value return forwards no single call, got {multi:?}"
    );
    let single = with_facts(
        "solo.odin",
        "package p\nsolo :: proc(n: int) -> int {\n\treturn compute(n)\n}\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "solo")
                .map(|f| f.sole_call)
                .expect("`solo` is recorded")
        },
    );
    assert!(
        single.is_some(),
        "a one-call body still links its call, got {single:?}"
    );
}

#[test]
fn keyed_literal_is_other_unkeyed_stays_collection() {
    let keyed = with_facts(
        "keyed.odin",
        "package p\nrun :: proc() {\n\tconfigure(Config{host = \"h\", port = 1})\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "configure")
                .map(|c| {
                    c.args
                        .iter()
                        .any(|a| a.shape == sloplint_core::facts::ArgShape::Other)
                })
                .expect("`configure` is recorded")
        },
    );
    assert!(keyed, "a keyed record is no list, got {keyed:?}");
    let plain = with_facts(
        "plain.odin",
        "package p\nrun :: proc() {\n\tcompute([]int{7})\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "compute")
                .map(|c| {
                    c.args
                        .iter()
                        .any(|a| a.shape == sloplint_core::facts::ArgShape::Collection)
                })
                .expect("`compute` is recorded")
        },
    );
    assert!(
        plain,
        "an unkeyed element list stays a Collection, got {plain:?}"
    );
}

#[test]
fn named_argument_counts_once_as_its_value() {
    let named = with_facts(
        "wrap.odin",
        "package p\nwrap :: proc(n: int, s: string) -> int {\n\treturn f(n, sep = s)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "f")
                .map(|c| {
                    c.args
                        .iter()
                        .filter_map(|a| match a.shape {
                            sloplint_core::facts::ArgShape::Ident(name) => Some(name.to_string()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .expect("`f` is recorded")
        },
    );
    assert_eq!(
        named,
        vec![String::from("n"), String::from("s")],
        "the named value counts once under its value, got {named:?}"
    );
    let plain = with_facts(
        "plain.odin",
        "package p\nwrap :: proc(n: int, s: string) -> int {\n\treturn f(n, s)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "f")
                .map(|c| {
                    c.args
                        .iter()
                        .filter_map(|a| match a.shape {
                            sloplint_core::facts::ArgShape::Ident(name) => Some(name.to_string()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .expect("`f` is recorded")
        },
    );
    assert_eq!(
        plain,
        vec![String::from("n"), String::from("s")],
        "plain positionals keep their order, got {plain:?}"
    );
}

#[test]
fn qualified_variants_count_like_implicit_ones() {
    let qualified = with_facts(
        "status.odin",
        "package p\nrun :: proc(s: Status) {\n\tswitch s {\n\tcase Status.Active, Status.Idle:\n\t\tf()\n\tcase:\n\t\tg()\n\t}\n}\n",
        |facts| {
            facts
                .wildcard_arms
                .first()
                .map(|w| (w.variant_count, w.enum_like))
                .expect("the wildcard arm is recorded")
        },
    );
    assert_eq!(
        qualified,
        (2, true),
        "two qualified variants count two, got {qualified:?}"
    );
}
