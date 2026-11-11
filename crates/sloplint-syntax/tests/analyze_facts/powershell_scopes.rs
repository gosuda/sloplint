use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_powershell_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::PowerShell, "case.ps1", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn script_block_param_write_stays_in_the_block() {
    let pad = "  # pad\n".repeat(26);
    let source = format!(
        "function Invoke-Run {{\n  $x = 0\n  $f = {{ param($x) $x = 1; $x }}\n{pad}  $x = 5\n}}\n"
    );
    with_powershell_bindings(&source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            2,
            "the param assignment declares a block-local binding beside the outer one: {found:?}"
        );
        let outer = xs
            .iter()
            .find(|binding| binding.pos.line == 2)
            .expect("the outer $x stays declared at line 2");
        assert_eq!(
            (
                outer.write_lines,
                outer.first_write_line,
                outer.last_write_line
            ),
            (1, 30, 30),
            "only the line-30 write reaches the outer $x, was 2 writes spanning 3 to 30 before the script-block frame: {found:?}"
        );
    });
}

#[test]
fn foreach_variable_writes_the_function_binding() {
    let pad = "  # pad\n".repeat(36);
    let source =
        format!("function F {{\n  $i = 0\n  foreach ($i in $xs) {{ }}\n{pad}  $i = 5\n}}\n");
    with_powershell_bindings(&source, |found| {
        let is = named_all(found, "i");
        assert_eq!(
            is.len(),
            1,
            "the loop target reuses the function-level $i instead of declaring: {found:?}"
        );
        assert_eq!(
            (
                is[0].write_lines,
                is[0].first_write_line,
                is[0].last_write_line
            ),
            (2, 3, 40),
            "writes land at lines 3 and 40, was a single write before the loop target was recorded: {found:?}"
        );
        assert_eq!(
            is[0].loop_depth, 0,
            "the loop-target write lands on entry before the loop frame opens: {found:?}"
        );
    });
}

#[test]
fn increment_through_a_script_block_reaches_the_outer_binding() {
    let source = "function F {\n  $x = 0\n  $f = { $x++ }\n  $x = 2\n}\n";
    with_powershell_bindings(source, |found| {
        let xs = named_all(found, "x");
        assert_eq!(
            xs.len(),
            1,
            "the increment introduces no extra binding: {found:?}"
        );
        assert_eq!(
            (
                xs[0].write_lines,
                xs[0].first_write_line,
                xs[0].last_write_line
            ),
            (2, 3, 4),
            "the block increment and the later assignment both write the outer $x: {found:?}"
        );
    });
}
