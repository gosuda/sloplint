// `MACRO(args) { members };` inside a class accepts every member shape the
// braced-macro member list models: bit-fields, friend declarations, access
// specifiers, and plain declarations.
#[test]
fn macro_braced_member_shapes_parse() {
    let source = concat!(
        "struct Widget {\n",
        "  PRIVILEGED_MEMBERS(int tag) {\n",
        "    int flags : 3;\n",
        "    friend class Helper;\n",
        "  public:\n",
        "    int value;\n",
        "  };\n",
        "};\n",
    );
    let language: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("cpp language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let sexp = tree.root_node().to_sexp();
    assert!(
        !tree.root_node().has_error() && !sexp.contains("MISSING"),
        "must parse clean, sexp: {sexp}"
    );
    for needle in [
        "macro_braced_declaration",
        "bitfield_clause",
        "friend_declaration",
    ] {
        assert!(sexp.contains(needle), "missing {needle}, sexp: {sexp}");
    }
}

// `S() { this->x = 1; };` in member position is a constructor, not a
// braced macro — even with the trailing `;`. The same `NAME() { ... }`
// at file scope cannot be a constructor, so the loose declaration form
// claims it as a macro.
#[test]
fn empty_paren_braced_decl_ctor_vs_macro() {
    let language: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("cpp language loads");

    let member = "struct S {\n  S() { this->x = 1; };\n};\n";
    let tree = parser.parse(member, None).expect("parser returns a tree");
    let sexp = tree.root_node().to_sexp();
    assert!(
        !tree.root_node().has_error() && !sexp.contains("MISSING"),
        "ctor must parse clean, sexp: {sexp}"
    );
    assert!(
        sexp.contains("function_definition") && !sexp.contains("macro_braced_declaration"),
        "member S(){{}} must stay a function_definition, sexp: {sexp}"
    );

    let file = "WIDGET() {\n  int make();\n};\n";
    let tree = parser.parse(file, None).expect("parser returns a tree");
    let sexp = tree.root_node().to_sexp();
    assert!(
        !tree.root_node().has_error() && !sexp.contains("MISSING"),
        "file-scope empty macro must parse clean, sexp: {sexp}"
    );
    assert!(
        sexp.contains("macro_braced_declaration"),
        "file-scope NAME() {{}} must read as macro_braced_declaration, sexp: {sexp}"
    );
}

// `operator[]` is not a lambda capture, and `operator()([]{...})` still
// sees the real lambda — the probe must not let the operator keyword
// suppress captures that follow other tokens.
#[test]
fn operator_brackets_vs_lambda_captures() {
    for (source, want_macro) in [
        (
            "struct S {\n  MACRO(int t) {\n    int operator[](int i) { return t[i]; }\n  };\n};\n",
            true,
        ),
        (
            "void f(void) {\n  Widget{factory.operator()([] { return 1; })};\n}\n",
            false,
        ),
    ] {
        let language: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).expect("cpp language loads");
        let tree = parser.parse(source, None).expect("parser returns a tree");
        let sexp = tree.root_node().to_sexp();
        assert!(
            !tree.root_node().has_error() && !sexp.contains("MISSING"),
            "must parse clean, sexp: {sexp}"
        );
        if want_macro {
            assert!(
                sexp.contains("macro_braced_declaration"),
                "must claim the macro, sexp: {sexp}"
            );
        } else {
            assert!(
                !sexp.contains("macro_braced"),
                "must stay a braced init, sexp: {sexp}"
            );
        }
    }
}

// Comments are legal inside an operator name: `operator /*c*/ []` must
// still suppress lambda marking on its `]` so the enclosing macro body
// binds, while a division `/` after `operator` clears the flag.
#[test]
fn operator_comment_brackets_vs_division() {
    let language: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("cpp language loads");

    let source = concat!(
        "struct S {\n",
        "  MACRO(int t) {\n",
        "    int operator /*c*/ [](int i) { return t[i]; }\n",
        "    int operator // line\n",
        "      [](int i) { return t[i]; }\n",
        "    int operator /(int i) { return t[i]; }\n",
        "  };\n",
        "};\n",
    );
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let sexp = tree.root_node().to_sexp();
    assert!(
        !tree.root_node().has_error() && !sexp.contains("MISSING"),
        "commented operator[] must parse clean, sexp: {sexp}"
    );
    assert!(
        sexp.contains("macro_braced_declaration"),
        "must claim the macro, sexp: {sexp}"
    );
}

// `#ifdef`-guarded `WIDGET() { member-only content };` binds the loose
// braced decl at file, namespace, and statement scope — the inherited
// un-suffixed preproc arms take `_block_item`, so the child grammar
// re-declares them with the loose variant added.
#[test]
fn ifdef_guarded_empty_paren_braced_macro() {
    let language: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("cpp language loads");

    let source = concat!(
        "#ifdef FLAG\n",
        "WIDGET() { int bits: 3; };\n",
        "#endif\n",
        "namespace N {\n",
        "#ifdef FLAG\n",
        "WIDGET() { int bits: 3; };\n",
        "#endif\n",
        "}\n",
        "void f() {\n",
        "#ifdef FLAG\n",
        "  WIDGET() { int bits: 3; };\n",
        "#endif\n",
        "}\n",
    );
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let sexp = tree.root_node().to_sexp();
    assert!(
        !tree.root_node().has_error() && !sexp.contains("MISSING"),
        "guarded empty-paren macros must parse clean, sexp: {sexp}"
    );
    assert!(
        sexp.matches("macro_braced_declaration").count() == 3,
        "all three guarded macros must bind, sexp: {sexp}"
    );
}

// `WIDGET() { int bits : 3; };` inside a function body takes the
// paren-required member read (`macro_braced_declaration`): the bitfield
// is not a statement, so `macro_braced_statement` can't claim it and
// the base grammar errors. Bare `NAME {` keeps the statement read —
// the paren form never steals `ON_SCOPE_EXIT { cleanup(); };`.
#[test]
fn paren_braced_decl_at_statement_scope() {
    let source = concat!(
        "void f() {\n",
        "  WIDGET() { int bits : 3; };\n",
        "  ON_SCOPE_EXIT { cleanup(); };\n",
        "}\n",
    );
    let language: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("cpp language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let sexp = tree.root_node().to_sexp();
    assert!(
        !tree.root_node().has_error() && !sexp.contains("MISSING"),
        "must parse clean, sexp: {sexp}"
    );
    assert!(
        sexp.contains("macro_braced_declaration") && sexp.contains("bitfield_clause"),
        "paren form must read as member decl, sexp: {sexp}"
    );
    assert!(
        sexp.contains("macro_braced_statement"),
        "bare NAME block must stay a statement macro, sexp: {sexp}"
    );
}
