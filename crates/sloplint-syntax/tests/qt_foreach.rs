use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Cpp,
        path: "foreach.cpp",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &NO_CUSTOM);
    assert!(analysis.parse_error.is_none(), "{source}");
    analysis
}

#[test]
fn foreach_declaration_shadows_the_outer_binding() {
    let analysis = run(concat!(
        "void update() {\n",
        "  int x = 0;\n",
        "  Q_FOREACH(int x, xs) {\n",
        "    x = 1;\n",
        "    x = 2;\n",
        "  }\n",
        "}\n",
    ));
    let outer = &analysis.facts.bindings[0];
    assert_eq!((outer.name, outer.pos.line), ("x", 2));
    assert_eq!(outer.write_lines, 0);
}

#[test]
fn foreach_pointer_declaration_shadows_the_outer_binding() {
    let analysis = run(concat!(
        "void update() {\n",
        "  Widget *item = nullptr;\n",
        "  Q_FOREACH(Widget *item, widgets) {\n",
        "    item = nullptr;\n",
        "  }\n",
        "  item = fallback;\n",
        "}\n",
    ));
    let outer = &analysis.facts.bindings[0];
    assert_eq!((outer.name, outer.pos.line), ("item", 2));
    assert_eq!((outer.write_lines, outer.first_write_line), (1, 6));
}

#[test]
fn foreach_counts_nested_control_flow() {
    let analysis = run(concat!(
        "void update() {\n",
        "  Q_FOREACH(int x, xs) {\n",
        "    if (x) { consume(x); }\n",
        "  }\n",
        "  if (done) { finish(); }\n",
        "}\n",
    ));
    let function = &analysis.facts.functions[0];
    assert_eq!((function.cognitive, function.max_depth), (2, 2));
}

#[test]
fn foreach_existing_variable_remains_the_write_target() {
    let analysis = run(concat!(
        "void update() {\n",
        "  int x = 0;\n",
        "  Q_FOREACH(x, xs) {\n",
        "    x = 1;\n",
        "  }\n",
        "  x = 2;\n",
        "}\n",
    ));
    let outer = &analysis.facts.bindings[0];
    assert_eq!((outer.name, outer.pos.line), ("x", 2));
    assert_eq!((outer.write_lines, outer.first_write_line), (3, 3));
    assert_eq!((outer.last_write_line, outer.loop_depth), (6, 1));
}

#[test]
fn foreach_container_is_evaluated_before_iterator_shadowing() {
    let analysis = run(concat!(
        "void update() {\n",
        "  int x = 0;\n",
        "  Q_FOREACH(int x, make(++x)) {\n",
        "    x = 1;\n",
        "  }\n",
        "}\n",
    ));
    let outer = &analysis.facts.bindings[0];
    assert_eq!((outer.write_lines, outer.first_write_line), (1, 3));
}

#[test]
fn foreach_empty_body_still_assigns_the_existing_variable() {
    let analysis = run(concat!(
        "void update() {\n",
        "  int x = 0;\n",
        "  Q_FOREACH(x, xs);\n",
        "}\n",
    ));
    let outer = &analysis.facts.bindings[0];
    assert_eq!((outer.write_lines, outer.first_write_line), (1, 3));
    assert_eq!(outer.loop_depth, 1);
}

#[test]
fn foreach_header_writes_keep_source_order_across_lines() {
    let analysis = run(concat!(
        "void update() {\n",
        "  int x = 0;\n",
        "  Q_FOREACH(x,\n",
        "    make(++x));\n",
        "}\n",
    ));
    let outer = &analysis.facts.bindings[0];
    assert_eq!((outer.write_lines, outer.first_write_line), (2, 3));
    assert_eq!((outer.last_write_line, outer.loop_depth), (4, 1));
}

#[test]
fn foreach_aliases_share_iterator_scope() {
    for name in ["Q_FOREACH", "BOOST_FOREACH", "foreach"] {
        let source = format!("void update() {{ int x = 0; {name}(int x, xs) {{ x = 1; }} }}\n");
        let analysis = run(&source);
        assert_eq!(analysis.facts.bindings[0].write_lines, 0, "{name}");
        assert_eq!(analysis.facts.functions[0].max_depth, 1, "{name}");
    }
}

#[test]
fn foreach_inside_an_unbraced_if_keeps_the_else_outside() {
    let analysis = run(concat!(
        "void update(bool enabled) {\n",
        "  int x = 0;\n",
        "  if (enabled)\n",
        "    Q_FOREACH(int x, xs)\n",
        "      x = 1;\n",
        "  else\n",
        "    x = 2;\n",
        "}\n",
    ));
    let outer = &analysis.facts.bindings[0];
    assert_eq!((outer.write_lines, outer.first_write_line), (1, 7));
    assert_eq!(outer.loop_depth, 0);
    let function = &analysis.facts.functions[0];
    assert_eq!((function.cognitive, function.max_depth), (2, 2));
}
