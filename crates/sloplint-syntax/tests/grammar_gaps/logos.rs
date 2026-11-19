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

use sloplint_core::facts::{AmbientKind, ArgShape};
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
/// When the walk finds no such node but the root still reports an error, the
/// recovery is hidden, so the root position is reported instead of silence.
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
                return tree
                    .root_node()
                    .has_error()
                    .then(|| format!("error at {}", tree.root_node().start_position()));
            }
        }
    }
}
/// Reads the corpus directory for `run_corpus_parses_clean`. An unreadable
/// default checkout is a skip (`None`); an unreadable directory the developer
/// explicitly requested through `LOGOS_RUN_DIR` is a setup failure and panics.
fn corpus_paths(dir: &str, explicit: bool) -> Option<Vec<std::path::PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if !explicit => {
            eprintln!("skipping run_corpus_parses_clean: LOGOS_RUN_DIR {dir} is not readable: {e}");
            return None;
        }
        Err(e) => panic!("LOGOS_RUN_DIR {dir} must be readable: {e}"),
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "lgo"))
        .collect();
    paths.sort();
    Some(paths)
}

#[test]
fn corpus_paths_skips_unreadable_default() {
    assert!(
        corpus_paths("/nonexistent-sloplint-corpus-dir", false).is_none(),
        "an unreadable default checkout must skip, not fail"
    );
}

#[test]
#[should_panic(expected = "must be readable")]
fn corpus_paths_fails_explicit_unreadable() {
    corpus_paths("/nonexistent-sloplint-corpus-dir", true);
}

#[test]
fn run_corpus_parses_clean() {
    let (dir, explicit) = match std::env::var("LOGOS_RUN_DIR") {
        Ok(dir) => (dir, true),
        Err(_) => (
            "/home/alpha/repos-devtool/LoGOs/.outline/worktree/spira-upstream-plain/test/logos/run"
                .to_string(),
            false,
        ),
    };
    let Some(paths) = corpus_paths(&dir, explicit) else {
        return;
    };
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

#[test]
fn call_spread_parses_clean() {
    let source = "package p\n\nfunc sum(vals ...int) int {\n\ttotal := 0\n\tfor _, v := range vals {\n\t\ttotal += v\n\t}\n\treturn total\n}\n\nfunc g(args []int, rest []int) int {\n\tn := sum(..args)\n\treturn sum(n, ..rest)\n}\n";
    run_clean("src/spread.lgo", source);
    let tree = parse_raw(source);
    assert!(
        tree.root_node().to_sexp().contains("(spread_element"),
        "call spread must surface as spread_element, got {}",
        tree.root_node().to_sexp()
    );
}

#[test]
fn tuple_rejects_empty_elements() {
    for source in [
        "package p\n\nfunc f() {\n\tt := (1,,)\n\t_ = t\n}\n",
        "package p\n\nvar t (int,,)\n",
    ] {
        assert!(
            run("src/tuple.lgo", source).parse_error.is_some(),
            "empty tuple element must be a parse error for {source:?}"
        );
    }
}

#[test]
fn switch_expr_type_arm_parses_clean() {
    run_clean(
        "src/switchtype.lgo",
        "package p\n\nfunc f(x any) int {\n\treturn switch x.(type) {\n\tcase []byte => 1\n\tcase string => 0\n\t}\n}\n",
    );
}

#[test]
fn interp_with_comment_parses_clean() {
    run_clean(
        "src/comment.lgo",
        "package p\n\nfunc f() string {\n\treturn $\"value {1 /* { */ + 2}\"\n}\n",
    );
}

#[test]
fn const_func_keeps_doc() {
    let analysis = run_clean(
        "src/constfn.lgo",
        "package p\n\n// Process the order.\nconst func process_order() int {\n\treturn 0\n}\n",
    );
    let func = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "process_order")
        .expect("process_order must be a function fact");
    assert!(
        func.doc
            .iter()
            .any(|line| line.contains("Process the order")),
        "doc above const func must attach, got {:?}",
        func.doc
    );
}

#[test]
fn logos_test_file_marks_test_code() {
    let analysis = run_clean(
        "src/decode_test.lgo",
        "package p\n\nimport \"testing\"\n\nfunc BenchmarkDecode(b *testing.B) {\n}\n",
    );
    let func = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "BenchmarkDecode")
        .expect("BenchmarkDecode must be a function fact");
    assert!(
        func.in_test,
        "a _test.lgo file must mark its functions as test code"
    );
}

#[test]
fn lambda_collection_is_code_body() {
    let analysis = run_clean(
        "src/lambda.lgo",
        "package p\n\nfunc consume(vals []int) int {\n\treturn 0\n}\n\nfunc f() int {\n\treturn consume([]int{(x) => x})\n}\n",
    );
    let call = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "consume")
        .expect("consume must be a call fact");
    assert_eq!(
        call.args.first().map(|a| &a.shape),
        Some(&ArgShape::CodeBody),
        "a collection holding a lambda is code, got {:?}",
        call.args.first().map(|a| &a.shape)
    );
}

#[test]
fn named_collection_arg_keeps_collection_shape() {
    let analysis = run_clean(
        "src/named.lgo",
        "package p\n\nfunc consume(vals []int) int {\n\treturn 0\n}\n\nfunc f() int {\n\treturn consume(opts = []int{1, 2})\n}\n",
    );
    let call = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "consume")
        .expect("consume must be a call fact");
    assert_eq!(
        call.args.first().map(|a| &a.shape),
        Some(&ArgShape::Collection),
        "a named multiline collection stays a collection, got {:?}",
        call.args.first().map(|a| &a.shape)
    );
}

#[test]
fn try_call_initializes_binding() {
    let analysis = run_clean(
        "src/tryinit.lgo",
        "package p\n\nfunc Load(vals []int) []int {\n\treturn vals\n}\n\nfunc f() {\n\trows := Load([]int{1, 2})?\n\t_ = rows\n}\n",
    );
    let call = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "Load")
        .expect("Load must be a call fact");
    assert!(call.initializes_binding, "Load(...)? must initialize rows");
}

#[test]
fn scope_defer_marks_cleanup() {
    let analysis = run_clean(
        "src/deferred.lgo",
        "package probe\n\nimport (\n\t\"log\"\n\t\"os\"\n)\n\nfunc Load(path string) (*os.File, error) {\n\tf, err := os.Open(path)\n\tif err != nil {\n\t\treturn nil, err\n\t}\n\tscope defer func() {\n\t\tif err := f.Close(); err != nil {\n\t\t\tlog.Printf(\"close failed\")\n\t\t}\n\t}()\n\treturn f, nil\n}\n",
    );
    assert!(
        analysis.facts.handlers.iter().any(|h| h.in_cleanup),
        "an if inside a scope defer closure is cleanup"
    );
}

#[test]
fn interp_string_reports_static_prefix() {
    let analysis = run_clean(
        "src/interp.lgo",
        "package p\n\nimport \"log\"\n\nfunc f(name string) {\n\tlog.Print($\"service started: {name}\")\n}\n",
    );
    let call = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "log.Print")
        .expect("log.Print must be a call fact");
    let shape = call.args.first().map(|a| &a.shape);
    assert!(
        matches!(shape, Some(ArgShape::Str { longest_static }) if *longest_static >= 8),
        "the fixed prefix must read as a searchable literal, got {shape:?}"
    );
}

#[test]
fn first_problem_agrees_with_has_error() {
    for source in [
        "package p\n",
        "package p\n\nfunc f() int {\n\treturn 0\n}\n",
        "func f( {",
    ] {
        let tree = parse_raw(source);
        assert_eq!(
            first_problem(&tree).is_some(),
            tree.root_node().has_error(),
            "first_problem must agree with has_error for {source:?}"
        );
    }
}

#[test]
fn nested_interp_depth_is_bounded() {
    let mut deep = String::from("package p\n\nfunc f() string {\n\treturn $\"");
    for _ in 0..40 {
        deep.push_str("{\"a\", $\"");
    }
    deep.push_str("deep");
    for _ in 0..40 {
        deep.push_str("\"}");
    }
    deep.push_str("\"\n}\n");
    assert!(
        run("src/deep.lgo", &deep).parse_error.is_some(),
        "40 nested interpolations must exceed the scanner depth limit"
    );
    run_clean(
        "src/shallow.lgo",
        "package p\n\nfunc f() string {\n\treturn $\"a{\"b\", $\"c\"}\"\n}\n",
    );
}

#[test]
fn spread_collection_arg_keeps_collection_shape() {
    let analysis = run_clean(
        "src/spread.lgo",
        "package p\n\nfunc consume(vals []int) int {\n\treturn 0\n}\n\nfunc f() int {\n\treturn consume(..[]int{1, 2})\n}\n",
    );
    let call = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "consume")
        .expect("consume must be a call fact");
    assert_eq!(
        call.args.first().map(|a| &a.shape),
        Some(&ArgShape::Collection),
        "a spread inline collection stays a collection, got {:?}",
        call.args.first().map(|a| &a.shape)
    );
}

#[test]
fn saved_lambda_inside_deferred_closure_is_not_cleanup() {
    let analysis = run_clean(
        "src/savedlambda.lgo",
        "package p\n\nfunc save(f func()) {}\n\nfunc work() error {\n\treturn nil\n}\n\nfunc f() {\n\tscope defer func() {\n\t\tsave(() => {\n\t\t\tif err := work(); err != nil {\n\t\t\t}\n\t\t})\n\t}()\n}\n",
    );
    assert!(
        !analysis.facts.handlers.is_empty(),
        "the saved lambda must emit a handler fact"
    );
    assert!(
        !analysis.facts.handlers.iter().any(|h| h.in_cleanup),
        "a handler in a saved lambda must not inherit deferred cleanup"
    );
}

#[test]
fn spread_ident_arg_is_not_a_plain_identifier() {
    let analysis = run_clean(
        "src/spreadident.lgo",
        "package p\n\nfunc sum(vals ...int) int {\n\treturn 0\n}\n\nfunc total(values []int) int {\n\treturn sum(..values)\n}\n",
    );
    let call = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "sum")
        .expect("sum must be a call fact");
    assert_eq!(
        call.args.first().map(|a| &a.shape),
        Some(&ArgShape::Other),
        "a spread identifier expands elements instead of forwarding one value, got {:?}",
        call.args.first().map(|a| &a.shape)
    );
}

#[test]
fn swapped_named_args_are_not_plain_identifiers() {
    let analysis = run_clean(
        "src/swappednamed.lgo",
        "package p\n\nfunc Render(left string, right string) string {\n\treturn left\n}\n\nfunc Swap(left string, right string) string {\n\treturn Render(right = left, left = right)\n}\n",
    );
    let call = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "Render")
        .expect("Render must be a call fact");
    assert!(
        call.args
            .iter()
            .all(|a| !matches!(a.shape, ArgShape::Ident(_))),
        "named arguments bind by name, so no value may read as a forwarded identifier, got {:?}",
        call.args.iter().map(|a| &a.shape).collect::<Vec<_>>()
    );
    assert_eq!(
        call.args.iter().map(|a| &a.shape).collect::<Vec<_>>(),
        vec![
            &ArgShape::NamedIdent("left"),
            &ArgShape::NamedIdent("right"),
        ],
        "a named identifier still references its name, got {:?}",
        call.args.iter().map(|a| &a.shape).collect::<Vec<_>>()
    );
}

#[test]
fn interp_prefix_in_concat_keeps_its_static_run() {
    let analysis = run_clean(
        "src/interpconcat.lgo",
        "package p\n\nimport \"log\"\n\nfunc Greet(name string, suffix string) {\n\tlog.Print($\"service started: {name}\" + suffix)\n}\n",
    );
    let call = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "log.Print")
        .expect("Print must be a call fact");
    assert_eq!(
        call.args.first().map(|a| &a.shape),
        Some(&ArgShape::Concat { longest_static: 17 }),
        "\"service started: \" is 17 static chars, got {:?}",
        call.args.first().map(|a| &a.shape)
    );
}

#[test]
fn var_group_without_separator_is_a_parse_error() {
    let analysis = run("src/varsep.lgo", "var ( x int y int )\n");
    assert!(
        first_problem(&parse_raw("var ( x int y int )\n")).is_some(),
        "adjacent declarations without a separator must not parse cleanly"
    );
    assert!(
        analysis.parse_error.is_some(),
        "adjacent declarations without a separator must not parse cleanly"
    );
}

#[test]
fn var_group_without_trailing_separator_parses() {
    run_clean(
        "src/varok.lgo",
        "var ( x int; y int )\nvar ( x int )\nvar (\n\tx int\n\ty int\n)\nvar ( x int; )\n",
    );
}

#[test]
fn var_group_with_newline_comment_separator_parses() {
    // Go inserts a semicolon after a declaration when a block comment on the
    // same line spans a newline, so the comment separates the declarations
    // instead of producing a parse error.
    let analysis = run_clean(
        "src/varcommentsep.lgo",
        "package p\n\nvar (\n\tfirst int /* note\n */ second int\n)\n",
    );
    let mut names: Vec<&str> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalMutable)
        .map(|a| a.name)
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["first", "second"],
        "both declarations around the comment must read as globals, got {names:?}"
    );
    // The separator comment is aliased to `comment`, so doc and suppression
    // handling keeps seeing it.
    let source = "package p\n\nvar (\n\tfirst int /* note\n */ second int\n)\n";
    let tree = parse_raw(source);
    let mut seen = false;
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "comment"
            && node
                .utf8_text(source.as_bytes())
                .is_ok_and(|text| text.contains("note"))
        {
            seen = true;
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    assert!(seen, "the separator comment must stay a comment node");
}

#[test]
fn single_line_block_comment_is_no_separator() {
    // A block comment without a newline is whitespace: with no other
    // separator the group must still fail, as the Go compiler rejects it.
    let analysis = run(
        "src/varcommentnosep.lgo",
        "package p\n\nvar (\n\tfirst int /* note */ second int\n)\n",
    );
    assert!(
        analysis.parse_error.is_some(),
        "a single-line block comment must not separate declarations"
    );
}

#[test]
fn newline_comment_after_newline_stays_a_comment_node() {
    // A comment after a real newline still lexes as a `comment` extra (the
    // external scanner never skips a newline), so doc and suppression
    // handling keeps seeing it.
    let source =
        "package p\n\nvar (\n\tfirst int\n\t/* docs for second\n\t   more */\n\tsecond int\n)\n";
    let tree = parse_raw(source);
    assert!(
        !tree.root_node().has_error(),
        "the group with a leading comment must parse cleanly"
    );
    let mut seen = false;
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "comment"
            && node
                .utf8_text(source.as_bytes())
                .is_ok_and(|text| text.contains("docs for second"))
        {
            seen = true;
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    assert!(seen, "the comment after a newline must stay a comment node");
}

#[test]
fn deferred_lambda_through_parens_is_cleanup() {
    let analysis = run_clean(
        "src/deferredlambda.lgo",
        "package p\n\nfunc work() error {\n\treturn nil\n}\n\nfunc f() {\n\tscope defer (() => {\n\t\tif err := work(); err != nil {\n\t\t}\n\t})()\n}\n",
    );
    assert!(
        !analysis.facts.handlers.is_empty(),
        "the deferred lambda must emit a handler fact"
    );
    assert!(
        analysis.facts.handlers.iter().all(|h| h.in_cleanup),
        "a directly deferred lambda counts as cleanup, got {:?}",
        analysis
            .facts
            .handlers
            .iter()
            .map(|h| h.in_cleanup)
            .collect::<Vec<_>>()
    );
}

#[test]
fn interp_hole_with_comment_brace_stays_dynamic() {
    let analysis = run_clean(
        "src/interphole.lgo",
        "package p\n\nimport \"log\"\n\nfunc f(n int) {\n\tlog.Print($\"{n /* } explanation */}\")\n}\n",
    );
    let call = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "log.Print")
        .expect("log.Print must be a call fact");
    let shape = call.args.first().map(|a| &a.shape);
    assert!(
        matches!(shape, Some(ArgShape::Str { longest_static }) if *longest_static < 8),
        "expression source after a comment brace must not read as fixed text, got {shape:?}"
    );
}

#[test]
fn paren_around_interp_prefix_keeps_concat() {
    let analysis = run_clean(
        "src/parenconcat.lgo",
        "package p\n\nimport \"log\"\n\nfunc Greet(name string, suffix string) {\n\tlog.Print(($\"service started: {name}\") + suffix)\n\tlog.Print(($\"service started: {name}\" + suffix))\n}\n",
    );
    let shapes: Vec<&ArgShape> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "log.Print")
        .map(|c| {
            c.args
                .first()
                .map(|a| &a.shape)
                .expect("Print takes one arg")
        })
        .collect();
    assert_eq!(
        shapes,
        vec![
            &ArgShape::Concat { longest_static: 17 },
            &ArgShape::Concat { longest_static: 17 },
        ],
        "a parenthesized interpolated prefix is still a concatenation, got {shapes:?}"
    );
}

#[test]
fn right_grouped_concat_keeps_concat() {
    let analysis = run_clean(
        "src/rightgroup.lgo",
        "package p\n\nimport \"log\"\n\nfunc Greet(prefix string, suffix string) {\n\tlog.Print(prefix + (\"service started: \" + suffix))\n}\n",
    );
    let shapes: Vec<&ArgShape> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "log.Print")
        .map(|c| {
            c.args
                .first()
                .map(|a| &a.shape)
                .expect("Print takes one arg")
        })
        .collect();
    assert_eq!(
        shapes,
        vec![&ArgShape::Concat { longest_static: 17 }],
        "a right-grouped literal is still a concatenation, got {shapes:?}"
    );
}
