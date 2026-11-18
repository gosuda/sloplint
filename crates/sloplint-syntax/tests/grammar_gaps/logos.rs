//! LoGOs that the accepted-programs corpus exercises, observed through a raw-tree
//! walk because a hidden MISSING recovery node sets `has_error` without producing
//! a cursor-visible error position.
//!
//! `run_corpus_parses_clean` parses every `.lgo` file under the LoGOs toolchain
//! worktree's `test/logos/run/` directory (override with `LOGOS_RUN_DIR`); those
//! files are the compiler's own accepted-program corpus, so the vendored grammar
//! in `vendor/tree-sitter-logos` must parse all of them. The test prints the count
//! of files with an ERROR node and names each, then fails if the count is nonzero.
//!
//! The controls pin the facts the LoGOs adapter reads, so a shape regression in
//! the grammar patch shows up here.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Logos,
        path,
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn run_clean<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let analysis = run(path, source);
    assert!(
        analysis.parse_error.is_none(),
        "{path} must parse cleanly, first error at {:?}",
        analysis.parse_error
    );
    analysis
}

fn parse_raw(source: &str) -> tree_sitter::Tree {
    let language: tree_sitter::Language = tree_sitter_logos::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("logos language loads");
    parser.parse(source, None).expect("parser returns a tree")
}

/// True when the tree holds a visible ERROR node or a MISSING node anywhere.
fn first_problem(tree: &tree_sitter::Tree) -> Option<String> {
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            return Some(format!("{} at {}", node.kind(), node.start_position()));
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return None;
            }
        }
    }
}

#[test]
fn run_corpus_parses_clean() {
    let dir = std::env::var("LOGOS_RUN_DIR").unwrap_or_else(|_| {
        "/home/alpha/repos-devtool/LoGOs/.outline/worktree/spira-upstream-plain/test/logos/run"
            .to_string()
    });
    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("LOGOS_RUN_DIR {dir} must be readable: {e}"))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "lgo"))
        .collect();
    paths.sort();
    assert!(
        paths.len() >= 100,
        "run corpus must be present, got {}",
        paths.len()
    );
    let mut broken: Vec<String> = Vec::new();
    let mut details: Vec<String> = Vec::new();
    for path in &paths {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
        let tree = parse_raw(&source);
        if let Some(problem) = first_problem(&tree) {
            broken.push(path.display().to_string());
            details.push(format!("{}: {problem}", path.display()));
        }
    }
    println!(
        "logos run corpus: {} files, {} with an ERROR node",
        paths.len(),
        broken.len()
    );
    for name in &broken {
        println!("  ERROR {name}");
    }
    if !details.is_empty() {
        println!("{}", details.join("\n"));
    }
    assert!(
        broken.is_empty(),
        "{} run files have an ERROR node: {broken:?}",
        broken.len()
    );
}

// Invalid LoGOs must not become valid under the patched grammar.

#[test]
fn invalid_logos_still_reports_a_parse_error() {
    let analysis = run("src/bad.lgo", "func f( {");
    assert!(
        analysis.parse_error.is_some(),
        "a truncated parameter list must not parse cleanly"
    );
}

// The LoGOs forms the grammar adds must keep their node shapes.

#[test]
fn logos_only_forms_keep_their_node_shapes() {
    let source = "package p\n\nimport \"fmt\"\n\nfunc f(s string?) (int, error) {\n\treturn 1, nil\n}\n\nfunc Gen() int {\n\tyield 1\n\treturn 1\n}\n\nfunc Main(x *T) {\n\tgo group (ctx) {\n\t\tgo {\n\t\t\tfmt.Println(1)\n\t\t}\n\t}\n\tn := f(nil)?\n\tw := x?.f ?? 0\n\t_ = switch n.(type) {\n\tcase int => 1\n\t}\n\t_ = $\"a{n}b\"\n\tg := (v) => v\n\t_ = g(n)\n}\n";
    let analysis = run_clean("src/forms.lgo", source);
    assert!(
        analysis.facts.functions.iter().any(|f| f.name == "Main"),
        "Main must surface as a function, got {:?}",
        analysis
            .facts
            .functions
            .iter()
            .map(|f| f.name)
            .collect::<Vec<_>>()
    );
    let tree = parse_raw(source);
    let sexp = tree.root_node().to_sexp();
    for shape in [
        "(yield_statement",
        "(go_group_statement",
        "(option_type",
        "(try_expression",
        "(opt_selector_expression",
        "(coalesce_expression",
        "(switch_expression",
        "(interp_string_literal",
        "(lambda_expression",
    ] {
        assert!(
            sexp.contains(shape),
            "tree must contain {shape}, got {sexp}"
        );
    }
}

// A `const if` is an `if_statement` for depth: the nesting-depth rule must see
// it, or a LoGOs-only control form would silently skip the rule.

#[test]
fn const_if_counts_toward_function_depth() {
    let analysis = run_clean(
        "src/deep.lgo",
        "package p\n\nfunc Deep() int {\n\tconst if true {\n\t\tif true {\n\t\t\tif true {\n\t\t\t\tif true {\n\t\t\t\t\tif true {\n\t\t\t\t\t\treturn 1\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n\treturn 0\n}\n",
    );
    let deep = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "Deep")
        .expect("Deep must be a function fact");
    assert_eq!(
        deep.max_depth, 5,
        "const if + four ifs must reach depth 5, got {}",
        deep.max_depth
    );
}
