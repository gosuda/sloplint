use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_objc_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::ObjC, "case.m", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn for_init_decl_ends_with_the_for() {
    let source = "void f(int n) {\nint i = 0;\nfor (int i = 0; i < n; i++) {\n}\ni = 5;\n}\n";
    with_objc_bindings(source, |found| {
        let items = named_all(found, "i");
        assert_eq!(
            items.len(),
            2,
            "loop and outer declarations hold separate bindings: {found:?}"
        );
        assert_eq!(
            (items[0].write_lines, items[0].last_write_line),
            (1, 5),
            "post-loop write lands on the outer binding, was 0 writes ending at line 2 when the leaked loop binding absorbed it: {found:?}"
        );
        assert_eq!(
            (items[1].pos.line, items[1].write_lines),
            (3, 1),
            "loop binding keeps only its own increment: {found:?}"
        );
    });
}

#[test]
fn block_param_write_stays_local() {
    let source = "void f(void) {\nint x = 0;\nvoid (^b)(int) = ^(int x) {\nx = 1;\n};\n}\n";
    with_objc_bindings(source, |found| {
        let items = named_all(found, "x");
        assert_eq!(
            items.len(),
            1,
            "the block parameter declares no binding of its own: {found:?}"
        );
        assert_eq!(
            items[0].write_lines, 0,
            "block parameter keeps the in-block write local, was 1 write at line 4 when the parameter fell through to the outer binding: {found:?}"
        );
    });
}

#[test]
fn for_in_var_write_stays_local() {
    let source = "void f(NSArray *xs) {\nid x = nil;\nfor (id x in xs) {\nx = nil;\n}\n}\n";
    with_objc_bindings(source, |found| {
        let items = named_all(found, "x");
        assert_eq!(
            items.len(),
            1,
            "the for-in variable declares no binding of its own: {found:?}"
        );
        assert_eq!(
            items[0].write_lines, 0,
            "for-in variable keeps the in-loop write local, was 1 write at line 4 when the variable fell through to the outer binding: {found:?}"
        );
    });
}

#[test]
fn block_write_without_params_still_reaches_the_outer_binding() {
    let source = "void f(void) {\nint x = 0;\nvoid (^b)(void) = ^{\nx = 1;\n};\n}\n";
    with_objc_bindings(source, |found| {
        let items = named_all(found, "x");
        assert_eq!(
            items.len(),
            1,
            "a parameterless block declares nothing: {found:?}"
        );
        assert_eq!(
            (items[0].write_lines, items[0].first_write_line),
            (1, 4),
            "a parameterless block still captures the outer binding: {found:?}"
        );
    });
}

#[test]
fn case_label_declaration_spans_later_cases() {
    let source = "void f(int k) {\nswitch (k) {\ncase 1:\nint n = 0;\nn = 1;\nbreak;\ncase 2:\nn = 2;\nn = 3;\nbreak;\n}\n}\n";
    with_objc_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(ns.len(), 1, "case declaration holds one binding: {found:?}");
        assert_eq!(
            (ns[0].pos.line, ns[0].write_lines, ns[0].first_write_line),
            (4, 3, 5),
            "later-case writes reach the case 1 binding, was 1 starting at line 5 when the case exit dropped them: {found:?}"
        );
        assert_eq!(
            ns[0].last_write_line, 9,
            "span ends at the second later-case write: {found:?}"
        );
    });
}

#[test]
fn braced_block_under_case_ends_with_its_block() {
    let source = "void f(int k) {\nint n = 0;\nswitch (k) {\ncase 1: {\nint n = 0;\nn = 1;\n}\nbreak;\ncase 2:\nn = 2;\nbreak;\n}\n}\n";
    with_objc_bindings(source, |found| {
        let ns = named_all(found, "n");
        assert_eq!(
            ns.len(),
            2,
            "block and outer declarations stay separate: {found:?}"
        );
        assert_eq!(
            (ns[0].pos.line, ns[0].write_lines, ns[0].first_write_line),
            (2, 1, 10),
            "later-case write lands on the outer binding: {found:?}"
        );
        assert_eq!(
            (ns[1].pos.line, ns[1].write_lines, ns[1].first_write_line),
            (5, 1, 6),
            "braced case block keeps only its own write: {found:?}"
        );
    });
}
