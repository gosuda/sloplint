//! PHP grammar-gap regressions: valid PHP that tree-sitter-php 0.25.1 rejected
//! must parse cleanly through the public API.
//!
//! Each repro below is accepted by `php -l` (PHP 8.5.8). The controls are
//! rejected by `php -l` too, so they must keep reporting `parse_error`.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Php,
        path,
        source,
        is_test_file: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_php::LANGUAGE_PHP.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("php language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let root = tree.root_node();
    assert!(
        !root.has_error() && !root.to_sexp().contains("MISSING"),
        "raw tree must be error-free, sexp: {}",
        root.to_sexp()
    );
}

fn run_clean<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    assert_raw_clean(source);
    let analysis = run(path, source);
    assert!(
        analysis.parse_error.is_none(),
        "{path} must parse cleanly, first error at {:?}",
        analysis.parse_error
    );
    analysis
}

fn run_error(path: &str, source: &str) {
    let analysis = run(path, source);
    assert!(
        analysis.parse_error.is_some(),
        "{path} is invalid PHP and must keep reporting a parse error"
    );
}

// P1: class names that coincide with cast keywords, used in scoped access.
#[test]
fn cast_keyword_class_scope_access_parses() {
    run_clean(
        "binary_const.php",
        "<?php\n$x = new Binary($data, Binary::TYPE_GENERIC);\n",
    );
    run_clean("integer_class_arg.php", "<?php\nfoo(Integer::class);\n");
    run_clean("double_const.php", "<?php\n$y = Double::BAR;\n");
    run_clean("boolean_const.php", "<?php\n$z = Boolean::FOO;\n");
    run_clean("real_class.php", "<?php\n$z = Real::class;\n");
    // Control: a missing member after `::` stays invalid.
    run_error("binary_missing_member.php", "<?php\nfoo(Binary::);\n");
}

// P2: `??=` whose left side is a `clone` expression.
#[test]
fn clone_coalesce_assignment_parses() {
    run_clean(
        "clone_coalesce.php",
        "<?php\n$this->t ??= clone $u ??= Foo::create();\n",
    );
    // Control: a missing right-hand side stays invalid.
    run_error("clone_coalesce_missing.php", "<?php\nclone $u ??= ;\n");
}

// P3: assignments chained through `clone`.
#[test]
fn clone_chained_assignment_parses() {
    run_clean(
        "clone_chain.php",
        "<?php\n$a = clone $b = clone $c = make();\n",
    );
    // Control: an assignment with no right-hand side stays invalid.
    run_error("clone_chain_missing.php", "<?php\n$a = clone $b = ;\n");
}

// P4: a trailing comma in a group `use` declaration.
#[test]
fn group_use_trailing_comma_parses() {
    let a = run_clean(
        "group_use.php",
        "<?php\nuse Foo\\Bar\\{ Baz, Qux as Q, };\n",
    );
    assert!(
        a.facts.imports.iter().any(|i| i.module.contains("Foo")),
        "group use must still surface its imports"
    );
    // Control: an empty group element stays invalid.
    run_error("group_use_empty.php", "<?php\nuse Foo\\Bar\\{ , };\n");
}

// P5: a raw NUL byte inside a single-quoted string.
#[test]
fn single_quoted_nul_byte_parses() {
    run_clean("single_quoted_nul.php", "<?php\n$x = 'a\0b';\n");
    run_clean("single_quoted_nul_end.php", "<?php\n$x = 'c\0';\n");
    // Control: an unterminated single-quoted string stays invalid.
    run_error("single_quoted_nul_unterminated.php", "<?php\n$x = 'a\0b;\n");
}

// P6: a property named like a keyword inside double-quoted interpolation.
#[test]
fn interpolated_keyword_property_parses() {
    run_clean(
        "interp_class.php",
        "<?php\n$keys[] = \"\\0$item->class\\0$key\";\n",
    );
    run_clean("interp_function.php", "<?php\n$x = \"$item->function\";\n");
    // Control: an unterminated interpolated string stays invalid.
    run_error("interp_unterminated.php", "<?php\n$x = \"$item->class\n");
}

// P7: a heredoc body whose lines include `<?php` and a multi-line `/* */`
// comment, followed by a line that is not the end tag.
#[test]
fn heredoc_with_block_comment_body_parses() {
    let body: String = (1..=10).map(|i| format!("line{i}\n")).collect();
    let valid = format!("<?php\n$x = <<<EOF\n<?php\n/*\n{body}*/\ntrailing\nEOF;\n");
    run_clean("heredoc_block_comment.php", &valid);

    // Control: the same body without a closing identifier stays invalid.
    let unterminated = format!("<?php\n$x = <<<EOF\n<?php\n/*\n{body}*/\ntrailing\n;\n");
    run_error("heredoc_unterminated.php", &unterminated);
}
