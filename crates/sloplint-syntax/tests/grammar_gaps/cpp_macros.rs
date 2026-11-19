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
