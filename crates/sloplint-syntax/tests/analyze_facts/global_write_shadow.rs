//! Shadowed writes stay out of the ambient facts: a local or a parameter with
//! the same name as a file-scope mutable keeps its assignment local, while a
//! plain assignment to the file-scope name still reports a GlobalWrite.

use sloplint_core::facts::AmbientKind;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn in_function_writes(lang: Lang, path: &str, source: &str) -> usize {
    run_clean(lang, path, source)
        .facts
        .ambient
        .iter()
        .filter(|site| site.kind == AmbientKind::GlobalWrite && site.in_function.is_some())
        .count()
}

fn check_shadowed(lang: Lang, path: &str, plain: &str, local: &str, param: &str) {
    assert_eq!(
        in_function_writes(lang, path, plain),
        1,
        "{path}: a plain assignment to the file-scope mutable is one GlobalWrite"
    );
    assert_eq!(
        in_function_writes(lang, path, local),
        0,
        "{path}: an assignment to a shadowing local is no GlobalWrite"
    );
    assert_eq!(
        in_function_writes(lang, path, param),
        0,
        "{path}: an assignment to a shadowing parameter is no GlobalWrite"
    );
}

#[test]
fn go_locals_and_params_shadow_the_global() {
    check_shadowed(
        Lang::Go,
        "shadow.go",
        "package main\n\nvar counter = 0\n\nfunc plain() {\ncounter = 1\n}\n",
        "package main\n\nvar counter = 0\n\nfunc shadowed() {\ncounter := 0\ncounter = 2\n}\n",
        "package main\n\nvar counter = 0\n\nfunc withParam(counter int) {\ncounter = 3\n}\n",
    );
}

#[test]
fn java_locals_and_params_shadow_the_static_field() {
    check_shadowed(
        Lang::Java,
        "Shadow.java",
        "class A {\nstatic int counter = 0;\nvoid plain() {\ncounter = 1;\n}\n}\n",
        "class A {\nstatic int counter = 0;\nvoid shadowed() {\nint counter = 0;\ncounter = 2;\n}\n}\n",
        "class A {\nstatic int counter = 0;\nvoid withParam(int counter) {\ncounter = 3;\n}\n}\n",
    );
}

#[test]
fn swift_locals_and_params_shadow_the_global() {
    check_shadowed(
        Lang::Swift,
        "shadow.swift",
        "var counter = 0\n\nfunc plain() {\ncounter = 1\n}\n",
        "var counter = 0\n\nfunc shadowed() {\nvar counter = 0\ncounter = 2\n}\n",
        "var counter = 0\n\nfunc withParam(counter: Int) {\ncounter = 3\n}\n",
    );
}

#[test]
fn kotlin_locals_and_params_shadow_the_global() {
    check_shadowed(
        Lang::Kotlin,
        "shadow.kt",
        "var counter = 0\n\nfun plain() {\ncounter = 1\n}\n",
        "var counter = 0\n\nfun shadowed() {\nvar counter = 0\ncounter = 2\n}\n",
        "var counter = 0\n\nfun withParam(counter: Int) {\ncounter = 3\n}\n",
    );
}

#[test]
fn ruby_sigil_writes_are_never_shadowed() {
    assert_eq!(
        in_function_writes(
            Lang::Ruby,
            "a.rb",
            "$config = nil\n\ndef configure(value)\n  $config = value\nend\n"
        ),
        1,
        "a.rb: assignment to a Ruby global keeps its GlobalWrite"
    );
}

#[test]
fn go_named_results_stay_out_of_params() {
    let analysis = run_clean(
        Lang::Go,
        "named_result_params.go",
        "package main\n\nfunc f(a bool) (ok bool, err bool) {\nok = a\nreturn\n}\n",
    );
    assert_eq!(
        analysis.facts.functions.len(),
        1,
        "named_result_params.go: one function fact"
    );
    assert_eq!(
        analysis.facts.functions[0]
            .params
            .iter()
            .map(|p| p.name)
            .collect::<Vec<_>>(),
        ["a"],
        "named_result_params.go: named results are not parameters to rules"
    );
}

#[test]
fn go_named_results_shadow_the_global() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "named_result.go",
            "package main\n\nvar g = 0\n\nfunc f() (g int) {\ng = 1\nreturn\n}\n",
        ),
        0,
        "named_result.go: an assignment to a named result is no GlobalWrite"
    );
}

#[test]
fn go_range_loop_vars_shadow_the_global() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "range_shadow.go",
            "package main\n\nvar g = 0\n\nfunc f(xs []int) {\nfor g := range xs {\ng = 1\n}\n}\n",
        ),
        0,
        "range_shadow.go: an assignment to a range loop variable is no GlobalWrite"
    );
}

#[test]
fn java_enhanced_for_variable_shadows_the_static_field() {
    assert_eq!(
        in_function_writes(
            Lang::Java,
            "Shadow.java",
            "class C {\nstatic int g = 0;\nvoid f(int[] xs) {\nfor (int g : xs) {\ng = 0;\n}\n}\n}\n",
        ),
        0,
        "Shadow.java: a write to the enhanced-for variable is no GlobalWrite"
    );
}

#[test]
fn java_catch_parameter_shadows_the_static_field() {
    assert_eq!(
        in_function_writes(
            Lang::Java,
            "Shadow.java",
            "class C {\nstatic String g = null;\nvoid f() {\ntry {\nwork();\n} catch (Exception g) {\ng = null;\n}\n}\n}\n",
        ),
        0,
        "Shadow.java: a write to the catch parameter is no GlobalWrite"
    );
}

#[test]
fn java_blank_final_local_shadows_the_static_field() {
    check_shadowed(
        Lang::Java,
        "Shadow.java",
        "class A {\nstatic int limit = 0;\nvoid plain() {\nlimit = 5;\n}\n}\n",
        "class A {\nstatic int limit = 0;\nvoid shadowed() {\nfinal int limit;\nlimit = 5;\n}\n}\n",
        "class A {\nstatic int limit = 0;\nvoid withParam(int limit) {\nlimit = 5;\n}\n}\n",
    );
}

#[test]
fn go_method_receiver_shadows_the_global() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "receiver_shadow.go",
            "package main\n\ntype Server struct{}\n\nvar cfg *Server\n\nfunc (other *Server) Load() {\ncfg = nil\n}\n",
        ),
        1,
        "receiver_shadow.go: a plain assignment to the file-scope mutable is one GlobalWrite"
    );
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "receiver_shadow.go",
            "package main\n\ntype Server struct{}\n\nvar cfg *Server\n\nfunc (cfg *Server) Load() {\ncfg = nil\n}\n",
        ),
        0,
        "receiver_shadow.go: an assignment to a shadowing receiver is no GlobalWrite"
    );
}

#[test]
fn go_type_switch_alias_shadows_the_global() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "type_switch_shadow.go",
            "package main\n\nvar v int\n\nfunc f(x any) {\nswitch x.(type) {\ncase int:\nv = 1\n}\n}\n",
        ),
        1,
        "type_switch_shadow.go: a plain assignment to the file-scope mutable is one GlobalWrite"
    );
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "type_switch_shadow.go",
            "package main\n\nvar v int\n\nfunc f(x any) {\nswitch v := x.(type) {\ncase int:\nv = 1\n}\n}\n",
        ),
        0,
        "type_switch_shadow.go: an assignment to a shadowing type-switch alias is no GlobalWrite"
    );
}

#[test]
fn go_select_receive_shadows_the_global() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "select_shadow.go",
            "package main\n\nvar v int\n\nfunc f(ch chan int) {\nselect {\ncase <-ch:\nv = 1\n}\n}\n",
        ),
        1,
        "select_shadow.go: a plain assignment to the file-scope mutable is one GlobalWrite"
    );
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "select_shadow.go",
            "package main\n\nvar v int\n\nfunc f(ch chan int) {\nselect {\ncase v := <-ch:\nv = 1\n}\n}\n",
        ),
        0,
        "select_shadow.go: an assignment to a shadowing select receive is no GlobalWrite"
    );
}
#[test]
fn js_for_let_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::JavaScript,
            "shadow.js",
            "let i = 0;\nfunction f() {\nfor (let i = 0; i < 3; i++) {}\ni = 5;\n}\n",
        ),
        1,
        "shadow.js: a write after the for loop reaches the file-scope mutable"
    );
    assert_eq!(
        in_function_writes(
            Lang::JavaScript,
            "shadow.js",
            "let i = 0;\nfunction f() {\nfor (let i = 0; i < 3; i++) {\ni = 5;\n}\n}\n",
        ),
        0,
        "shadow.js: a write inside the for loop stays local to the loop variable"
    );
}

#[test]
fn js_block_const_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::JavaScript,
            "shadow.js",
            "let counter = 0;\nfunction f() {\n{ const counter = 1; }\ncounter = 2;\n}\n",
        ),
        1,
        "shadow.js: a write after the block reaches the file-scope mutable"
    );
    assert_eq!(
        in_function_writes(
            Lang::JavaScript,
            "shadow.js",
            "let counter = 0;\nfunction f() {\n{ const counter = 1;\ncounter = 2;\n}\n}\n",
        ),
        0,
        "shadow.js: a write inside the block stays local to the const"
    );
}

#[test]
fn go_if_init_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "if_scope.go",
            "package main\n\nvar g int\n\nfunc f() {\nif g := 1; g > 0 {\n}\ng = 2\n}\n",
        ),
        1,
        "if_scope.go: a write after the if reaches the file-scope mutable"
    );
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "if_scope.go",
            "package main\n\nvar g int\n\nfunc f() {\nif g := 1; g > 0 {\ng = 2\n}\n}\n",
        ),
        0,
        "if_scope.go: a write inside the if stays local to the init variable"
    );
}

#[test]
fn go_range_var_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "range_scope.go",
            "package main\n\nvar counter int\n\nfunc f(xs []int) {\nfor counter := range xs {\n_ = counter\n}\ncounter = 5\n}\n",
        ),
        1,
        "range_scope.go: a write after the loop reaches the file-scope mutable"
    );
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "range_scope.go",
            "package main\n\nvar counter int\n\nfunc f(xs []int) {\nfor counter := range xs {\ncounter = 5\n}\n}\n",
        ),
        0,
        "range_scope.go: a write inside the loop stays local to the range variable"
    );
}

#[test]
fn odin_if_walrus_scope_ends_before_later_write() {
    assert_eq!(
        in_function_writes(
            Lang::Odin,
            "scope.odin",
            "package p\ncounter: int\nmain :: proc() {\nif true {\ncounter := 5\n}\ncounter = 7\n}\n",
        ),
        1,
        "scope.odin: a write after the if reaches the file-scope mutable"
    );
    assert_eq!(
        in_function_writes(
            Lang::Odin,
            "scope.odin",
            "package p\ncounter: int\nmain :: proc() {\nif true {\ncounter := 5\ncounter = 7\n}\n}\n",
        ),
        0,
        "scope.odin: a write inside the if stays local to the walrus variable"
    );
}

#[test]
fn go_short_var_initializer_closure_writes_the_global() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "init_closure.go",
            "package main\n\nvar x = 0\n\nfunc f() {\nx := func() {\nx = 1\n}\n}\n",
        ),
        1,
        "init_closure.go: a write inside the initializer closure reaches the file-scope variable"
    );
}

#[test]
fn go_var_initializer_closure_writes_the_global() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "init_var.go",
            "package main\n\nvar cfg = 0\n\nfunc f() {\nvar cfg = load(func() {\ncfg = nil\n})\n}\n",
        ),
        1,
        "init_var.go: a write inside the load closure reaches the file-scope variable"
    );
}

#[test]
fn go_writes_after_the_declaration_stay_local() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "later.go",
            "package main\n\nvar counter = 0\n\nvar total = 0\n\nfunc f() {\ncounter := 0\ncounter = 2\nvar total = 0\ntotal = 5\n}\n",
        ),
        0,
        "later.go: writes after the declaration stay local to the shadowing variable"
    );
}

#[test]
fn js_let_initializer_closure_stays_local() {
    assert_eq!(
        in_function_writes(
            Lang::JavaScript,
            "init.js",
            "let x = 0;\nfunction f() {\nlet x = () => {\nx = 1;\n};\n}\n",
        ),
        0,
        "init.js: a write inside the initializer closure stays local to the shadowing let"
    );
}

#[test]
fn go_var_group_later_spec_closure_writes_the_earlier_name() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "group.go",
            "package p\n\nvar a int\n\nfunc f() int {\nvar (\na = 1\nb = func() int { a = 2; return a }()\n)\na = 7\nreturn a + b\n}\n",
        ),
        0,
        "group.go: a write inside a later spec stays local to the earlier spec name"
    );
}

#[test]
fn go_single_var_spec_still_shadows_after_itself() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "single.go",
            "package p\n\nvar a int\n\nfunc f() {\nvar a = 1\na = 2\n}\n",
        ),
        0,
        "single.go: a write after a single spec stays local to the shadowing variable"
    );
}

#[test]
fn go_for_clause_init_covers_the_loop_body() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "loop.go",
            "package p\n\nvar i int\n\nfunc f() {\nfor i := 0; i < 3; i++ {\ni = 5\n}\n}\n",
        ),
        0,
        "loop.go: a write inside the loop body stays local to the loop variable"
    );
}

#[test]
fn go_write_after_the_loop_reaches_the_global() {
    assert_eq!(
        in_function_writes(
            Lang::Go,
            "after_loop.go",
            "package p\n\nvar i int\n\nfunc f() {\nfor i := 0; i < 3; i++ {\n}\ni = 9\n}\n",
        ),
        1,
        "after_loop.go: a write after the loop reaches the file-scope variable"
    );
}
