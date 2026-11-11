use sloplint_core::facts::MutBinding;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn with_ruby_bindings<R>(source: &str, read: impl FnOnce(&[MutBinding<'_>]) -> R) -> R {
    let analysis = run_clean(Lang::Ruby, "case.rb", source);
    read(&analysis.facts.bindings)
}

fn named_all<'a>(found: &'a [MutBinding<'a>], name: &str) -> Vec<&'a MutBinding<'a>> {
    found
        .iter()
        .filter(|binding| binding.name == name)
        .collect()
}

#[test]
fn block_param_stops_operator_write_to_outer() {
    let source = "def f(xs)\n  x = 0\n  xs.each { |x| x += 1 }\n  x = 2\nend\n";
    with_ruby_bindings(source, |found| {
        let outer = named_all(found, "x")
            .into_iter()
            .find(|binding| binding.pos.line == 2);
        let Some(binding) = outer else {
            panic!("outer x declared at line 2 is missing: {found:?}");
        };
        assert_eq!(
            (binding.write_lines, binding.first_write_line),
            (1, 4),
            "block param absorbs the +=, was write_lines 2 with first_write_line 3: {found:?}"
        );
        assert_eq!(
            binding.last_write_line, 4,
            "outer span ends at the post-block write: {found:?}"
        );
    });
}

#[test]
fn for_target_writes_the_method_level_binding() {
    let source = "def f(xs)\n  x = 0\n  for x in xs\n    puts x\n  end\n  x = 1\nend\n";
    with_ruby_bindings(source, |found| {
        let outer = named_all(found, "x")
            .into_iter()
            .find(|binding| binding.pos.line == 2);
        let Some(binding) = outer else {
            panic!("outer x declared at line 2 is missing: {found:?}");
        };
        assert_eq!(
            (binding.write_lines, binding.first_write_line),
            (2, 3),
            "for target records a write, was write_lines 1 with the loop write missing: {found:?}"
        );
        assert_eq!(
            binding.last_write_line, 6,
            "span reaches the post-loop write: {found:?}"
        );
    });
}

#[test]
fn rescue_variable_declares_when_absent() {
    let source = "def f\n  begin\n    risky\n  rescue => e\n    puts e\n  end\n  e = 1\nend\n";
    with_ruby_bindings(source, |found| {
        let rescued = named_all(found, "e")
            .into_iter()
            .find(|binding| binding.pos.line == 4);
        let Some(binding) = rescued else {
            panic!("rescue variable declared at line 4 is missing: {found:?}");
        };
        assert_eq!(
            (binding.write_lines, binding.last_write_line),
            (1, 7),
            "rescue declares e and the later write lands on it, was one binding declared at line 7 with write_lines 0: {found:?}"
        );
    });
}

#[test]
fn method_level_operator_write_still_counts() {
    let source = "def f\n  x = 0\n  x += 1\n  x = 2\nend\n";
    with_ruby_bindings(source, |found| {
        let outer = named_all(found, "x")
            .into_iter()
            .find(|binding| binding.pos.line == 2);
        let Some(binding) = outer else {
            panic!("outer x declared at line 2 is missing: {found:?}");
        };
        assert_eq!(
            (binding.write_lines, binding.first_write_line),
            (2, 3),
            "plain method writes are untouched by the block scoping work: {found:?}"
        );
        assert_eq!(
            binding.last_write_line, 4,
            "span still reaches the final write: {found:?}"
        );
    });
}

#[test]
fn plain_assignment_inside_a_block_writes_the_method_local() {
    let source = "def f(xs)\n  total = 0\n  xs.each { |x| total = total + x }\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n# gap\n  total = 0\nend\n";
    with_ruby_bindings(source, |found| {
        let totals = named_all(found, "total");
        assert_eq!(
            totals.len(),
            1,
            "blocks close over the method local, so no block-local total splits off: {found:?}"
        );
        assert_eq!(
            (totals[0].write_lines, totals[0].first_write_line),
            (2, 3),
            "both the block write and the late write land on the method binding: {found:?}"
        );
        assert_eq!(
            totals[0].last_write_line, 23,
            "span reaches across the gap to the late write: {found:?}"
        );
    });
}

#[test]
fn block_param_plain_assignment_stays_out_of_the_outer_local() {
    let source = "def f(xs)\n  total = 0\n  xs.each { |total| total = 1 }\n  total = 2\nend\n";
    with_ruby_bindings(source, |found| {
        let totals = named_all(found, "total");
        assert_eq!(
            totals.len(),
            1,
            "the param assignment binds nothing, leaving only the method local: {found:?}"
        );
        assert_eq!(
            (totals[0].write_lines, totals[0].first_write_line),
            (1, 4),
            "the block-param write never reaches the outer total: {found:?}"
        );
    });
}
