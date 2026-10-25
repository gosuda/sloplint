//! Valid C# that tree-sitter-c-sharp 0.23.5 rejected, observed through
//! `analyze` only.
//!
//! Each repro below is accepted by Roslyn CSC (dotnet SDK 10, syntax-only:
//! no CS1xxx). The vendored grammar in `vendor/tree-sitter-c-sharp` must
//! parse it. Controls pin validated-invalid inputs (preview `safe`,
//! `closed`/`union` type proposals, file-based `#:property`), which must
//! keep reporting `parse_error`.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::CSharp,
        path,
        source,
        is_test_file: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// The harness `first_error_pos` cannot see hidden MISSING recovery nodes,
/// so a clean `parse_error` alone could false-green; assert raw-tree
/// health (no ERROR, no MISSING anywhere in the sexp) on the same source.
fn assert_raw_clean(source: &str) {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .expect("load vendored c-sharp grammar");
    let tree = parser.parse(source, None).expect("parse source");
    let root = tree.root_node();
    assert!(
        !root.has_error(),
        "raw tree must be error-free, sexp: {}",
        root.to_sexp()
    );
    assert!(
        !root.to_sexp().contains("MISSING"),
        "raw tree must not hide MISSING nodes, sexp: {}",
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
        "{path} must still report a parse error",
    );
}

fn function_names<'a>(analysis: &'a Analysis<'_>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

fn type_names<'a>(analysis: &'a Analysis<'_>) -> Vec<&'a str> {
    analysis.facts.types.iter().map(|t| t.name).collect()
}

// CS-ALLOWS: `where T : allows ref struct` (C# 13-style anti-constraint).

#[test]
fn allows_ref_struct_parses() {
    let a = run_clean(
        "allows.cs",
        "public interface I<T> where T : allows ref struct {}\n",
    );
    assert!(
        type_names(&a).contains(&"I"),
        "interface I recorded: {:?}",
        type_names(&a)
    );
    let b = run_clean(
        "allows_notnull.cs",
        "public interface J<T> where T : notnull, allows ref struct {}\n",
    );
    assert!(
        type_names(&b).contains(&"J"),
        "interface J recorded: {:?}",
        type_names(&b)
    );
    // Existing behavior: ordinary constraints still parse.
    let c = run_clean(
        "where_plain.cs",
        "public interface K<T> where T : struct {}\n",
    );
    assert!(
        type_names(&c).contains(&"K"),
        "interface K recorded: {:?}",
        type_names(&c)
    );
}

// CS-REFPARTIAL: `ref` before `partial` on struct declarations.

#[test]
fn ref_partial_struct_parses() {
    let analysis = run_clean(
        "ref_partial.cs",
        "public ref partial struct S1 {}\n\
         public unsafe ref partial struct S2 {}\n\
         internal ref partial struct S3 {}\n",
    );
    for name in ["S1", "S2", "S3"] {
        assert!(
            type_names(&analysis).contains(&name),
            "struct {name} recorded: {:?}",
            type_names(&analysis)
        );
    }
    // Existing behavior: `partial` without `ref` still parses.
    let b = run_clean("partial_plain.cs", "public partial struct P {}\n");
    assert!(
        type_names(&b).contains(&"P"),
        "struct P recorded: {:?}",
        type_names(&b)
    );
}

// CS-PTRDEREF: pointer indirection over casts, arithmetic, and calls in
// value positions.

#[test]
fn pointer_deref_reads_parse() {
    let source = "using System;\n\
        class C {\n\
        \tunsafe int M(IntPtr p, char* a, int i, byte* q, int* e0) {\n\
        \t\tint z = *(int*)p;\n\
        \t\tchar c = *(a + i);\n\
        \t\tuint u = *(uint*)q;\n\
        \t\tint w = *q++;\n\
        \t\treturn z + w + *(int*)&e0;\n\
        \t}\n\
        }\n";
    let analysis = run_clean("deref_read.cs", source);
    assert!(
        function_names(&analysis).contains(&"M"),
        "function M recorded: {:?}",
        function_names(&analysis)
    );
}

#[test]
fn pointer_deref_call_and_ref_parse() {
    let source = "using System;\n\
        class C {\n\
        \tunsafe void M(byte* ptr, int pos, int off) {\n\
        \t\tref byte r = ref *(ptr + pos + off);\n\
        \t\tint* q = GetP();\n\
        \t\t*q = 1;\n\
        \t}\n\
        \tunsafe static int* GetP() => null;\n\
        }\n";
    let analysis = run_clean("deref_call.cs", source);
    assert!(
        function_names(&analysis).contains(&"GetP"),
        "function GetP recorded: {:?}",
        function_names(&analysis)
    );
}

// Ref-returning calls and null-conditional/ref-conditional assignment
// (C# 14) as assignment targets.

#[test]
fn ref_return_assign_parses() {
    let source = "using System;\n\
        class C {\n\
        \tstatic int _x;\n\
        \tstatic ref int GetRef() => ref _x;\n\
        \tvoid M() { GetRef() = 1; }\n\
        }\n";
    let analysis = run_clean("assign_call.cs", source);
    assert!(
        function_names(&analysis).contains(&"GetRef"),
        "function GetRef recorded: {:?}",
        function_names(&analysis)
    );
}

#[test]
fn conditional_assign_parses() {
    let source = "using System;\n\
        class C {\n\
        \tpublic int X;\n\
        \tvoid M(C? c, int[]? a, int v, bool b, ref int x, ref int y) {\n\
        \t\tc?.X = v;\n\
        \t\ta?[0] = v;\n\
        \t\t(b ? ref x : ref y) = v;\n\
        \t}\n\
        }\n";
    let analysis = run_clean("cond_assign.cs", source);
    assert!(
        function_names(&analysis).contains(&"M"),
        "function M recorded: {:?}",
        function_names(&analysis)
    );
    // Existing behavior: null-conditional reads still parse.
    run_clean(
        "qindex_read.cs",
        "using System;\nclass C { int M(int[]? a) => a?[0] ?? 0; }\n",
    );
}

// CS-METHODELEM: contextual keywords as collection elements.

#[test]
fn contextual_keyword_elements_parse() {
    let source = "using System;\n\
        class C {\n\
        \tvoid M(object o, object method, Type type, string name) {\n\
        \t\to.Invoke(null, [method]);\n\
        \t\tobject[] arr = [type, name];\n\
        \t}\n\
        }\n";
    let analysis = run_clean("methodelem.cs", source);
    assert!(
        function_names(&analysis).contains(&"M"),
        "function M recorded: {:?}",
        function_names(&analysis)
    );
}

// CS-EXTBLOCK: C# 14 extension blocks.

#[test]
fn extension_blocks_parse() {
    let source = "using System;\n\
        static class P {\n\
        \textension(float) {\n\
        \t\tpublic static bool IsF(float x) => x > 0;\n\
        \t}\n\
        }\n\
        struct V<T> {\n\
        \textension<T>(V<T>) where T : struct {\n\
        \t\tpublic static int N() => 0;\n\
        \t}\n\
        }\n";
    let analysis = run_clean("extblock.cs", source);
    assert!(
        function_names(&analysis).contains(&"IsF"),
        "extension method IsF recorded: {:?}",
        function_names(&analysis)
    );
    assert!(
        function_names(&analysis).contains(&"N"),
        "extension method N recorded: {:?}",
        function_names(&analysis)
    );
}

// CS-U202F: narrow no-break space is trivia.

#[test]
fn u202f_trivia_parses() {
    let narrow_nbsp = char::from_u32(0x202F).expect("U+202F is a valid scalar");
    let mut source = String::from("using System;\nclass");
    source.push(narrow_nbsp);
    source.push_str("C {}\n");
    run_clean("u202f.cs", &source);
}

// Unsafe destructors.

#[test]
fn unsafe_destructor_parses() {
    let analysis = run_clean(
        "unsafe_dtor.cs",
        "using System;\nclass C { unsafe ~C() {} }\n",
    );
    assert!(
        type_names(&analysis).contains(&"C"),
        "type C recorded: {:?}",
        type_names(&analysis)
    );
    // Existing behavior: plain and extern destructors still parse.
    run_clean("plain_dtor.cs", "class D { ~D() {} }\n");
}

// CS-SPLITPP: method bodies, const initializers, and accessors split
// across `#if` branches (balanced, no condition evaluation).

#[test]
fn split_bodies_parse() {
    let source = "using System;\n\
        class C {\n\
        \tvoid M()\n\
        #if X\n\
        \t=> A();\n\
        #else\n\
        \t{ B(); }\n\
        #endif\n\
        \tvoid A() {}\n\
        \tvoid B() {}\n\
        \tpublic static bool P\n\
        #if F\n\
        \t=> true;\n\
        #else\n\
        \t=> false;\n\
        #endif\n\
        }\n";
    let analysis = run_clean("split_body.cs", source);
    // NOTE: the split-bodied `M` itself is not recorded (the adapter only
    // reads block/arrow bodies); the sibling methods pin the facts.
    for name in ["A", "B"] {
        assert!(
            function_names(&analysis).contains(&name),
            "method {name} recorded: {:?}",
            function_names(&analysis)
        );
    }
}

#[test]
fn split_const_and_accessor_parse() {
    let source = "using System;\n\
        class C {\n\
        \tprivate const int N =\n\
        #if M\n\
        \t10;\n\
        #else\n\
        \t300;\n\
        #endif\n\
        \tpublic ulong Q {\n\
        #if B\n\
        \tget => 1;\n\
        \tset { }\n\
        #else\n\
        \tget => 2;\n\
        #endif\n\
        \t}\n\
        }\n";
    run_clean("split_const.cs", source);
}

// Controls: validated-invalid inputs must keep erroring.

#[test]
fn preview_and_invalid_still_error() {
    // `safe` is a preview-only modifier (CSC CS8652).
    run_error(
        "safe_preview.cs",
        "using System;\nstruct S { public safe object? O; }\n",
    );
    // `closed` classes are preview-only (CSC CS8652).
    run_error(
        "closed_preview.cs",
        "using System;\npublic closed class C {}\n",
    );
    // `union` declarations are not valid C# (CSC CS1001).
    run_error("union_invalid.cs", "using System;\npublic union U(int);\n");
    // `#:` directives only exist in file-based programs (CSC CS9298).
    run_error(
        "file_directive.cs",
        "#:property PublishAot=false\nusing System;\nclass C {}\n",
    );
}
