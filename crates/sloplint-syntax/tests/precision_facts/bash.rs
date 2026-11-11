use sloplint_core::facts::{AmbientKind, ArgShape, ScrutineeKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

struct FnFacts {
    name: String,
    visibility: Visibility,
    signature_forced: bool,
    entry_point: bool,
    in_test: bool,
    is_overload: bool,
    is_deprecated: bool,
    is_decorated: bool,
    is_trait_item: bool,
}

fn functions_of(source: &str) -> Vec<FnFacts> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Bash,
        path: "deploy.sh",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "bash parses");
    analysis
        .facts
        .functions
        .iter()
        .map(|f| FnFacts {
            name: f.name.to_owned(),
            visibility: f.visibility,
            signature_forced: f.signature_forced,
            entry_point: f.entry_point,
            in_test: f.in_test,
            is_overload: f.is_overload,
            is_deprecated: f.is_deprecated,
            is_decorated: f.is_decorated,
            is_trait_item: f.is_trait_item,
        })
        .collect()
}

fn calls_of(source: &str) -> Vec<(String, Option<String>, bool, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Bash,
        path: "deploy.sh",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "bash parses");
    analysis
        .facts
        .calls
        .iter()
        .map(|c| {
            (
                c.callee.to_owned(),
                c.callee_root.map(str::to_owned),
                c.is_simple_path,
                c.initializes_binding,
            )
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
enum Shape {
    Str,
    Concat(u32),
    Ident(String),
    Collection,
    CodeBody,
    Other,
}

fn shapes_of(source: &str) -> Vec<Vec<Shape>> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Bash,
        path: "deploy.sh",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "bash parses");
    analysis
        .facts
        .calls
        .iter()
        .map(|c| {
            c.args
                .iter()
                .map(|a| match &a.shape {
                    ArgShape::Str { .. } => Shape::Str,
                    ArgShape::Concat { longest_static } => Shape::Concat(*longest_static),
                    ArgShape::Ident(name) => Shape::Ident(name.to_string()),
                    ArgShape::Collection => Shape::Collection,
                    ArgShape::CodeBody => Shape::CodeBody,
                    ArgShape::Other => Shape::Other,
                })
                .collect()
        })
        .collect()
}

fn arms_of(source: &str) -> Vec<(ScrutineeKind, u32, bool, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Bash,
        path: "deploy.sh",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "bash parses");
    analysis
        .facts
        .wildcard_arms
        .iter()
        .map(|a| {
            (
                a.scrutinee,
                a.variant_count,
                a.has_nullable_ctor,
                a.enum_like,
            )
        })
        .collect()
}

fn handler_assertion_counts(source: &str) -> (usize, usize) {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Bash,
        path: "deploy.sh",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "bash parses");
    (
        analysis.facts.handlers.len(),
        analysis.facts.assertions.len(),
    )
}

#[test]
fn top_level_function_is_exported() {
    let rows = functions_of("deploy() {\n    echo ok\n}\n");
    assert_eq!(rows.len(), 1, "one function recorded");
    assert_eq!(rows[0].name, "deploy", "function name kept");
    assert_eq!(
        rows[0].visibility,
        Visibility::Exported,
        "bash function is exported"
    );
    let none = functions_of("echo hi\n");
    assert!(none.is_empty(), "command-only file has no functions");
}

#[test]
fn function_head_carries_no_forced_signature() {
    let rows = functions_of("main() {\n    echo hi\n}\n");
    assert_eq!(rows.len(), 1, "main function recorded");
    assert!(!rows[0].signature_forced, "bash never forces a signature");
    assert!(!rows[0].entry_point, "bash has no entry-point concept");
    assert!(!rows[0].in_test, "library file is not a test region");
    assert!(!rows[0].is_overload, "bash has no overloads");
    assert!(!rows[0].is_deprecated, "bash has no deprecation marker");
    assert!(!rows[0].is_decorated, "bash has no decorators");
    assert!(!rows[0].is_trait_item, "bash has no traits");
    let none = functions_of("FOO=bar\n");
    assert!(none.is_empty(), "assignment-only file has no functions");
}

#[test]
fn calls_carry_path_root_and_binding_facts() {
    let rows = calls_of("grep -r \"needle\" .\n");
    assert!(!rows.is_empty(), "command call recorded");
    for (callee, root, is_simple_path, initializes_binding) in &rows {
        assert_eq!(callee, "grep", "callee is the command name");
        assert_eq!(root.as_deref(), Some("grep"), "bare word is its own root");
        assert!(is_simple_path, "bare word is a simple path");
        assert!(!initializes_binding, "plain command initializes no binding");
    }
    let nested = calls_of("result=$(grep -r \"needle\" .)\n");
    assert_eq!(nested.len(), 1, "substitution call recorded");
    assert!(
        nested[0].3,
        "whole-value substitution initializes the binding"
    );
}

#[test]
fn star_arm_reports_other_scrutinee() {
    let arms = arms_of("case \"$x\" in\nfoo) echo a ;;\n*) echo b ;;\nesac\n");
    assert_eq!(arms.len(), 1, "star arm recorded once");
    assert_eq!(
        arms[0].0,
        ScrutineeKind::Other,
        "bash scrutinee stays Other"
    );
    assert_eq!(arms[0].1, 0, "bash counts no variants");
    assert!(!arms[0].2, "bash has no nullable constructors");
    assert!(!arms[0].3, "star arm is not enum-like");
    let plain = arms_of("case \"$x\" in\nfoo) echo a ;;\nesac\n");
    assert!(plain.is_empty(), "literal-only case has no wildcard arm");
}

#[test]
fn trap_file_has_no_handler_or_assertion_facts() {
    let (handlers, assertions) = handler_assertion_counts("trap 'cleanup' EXIT\n");
    assert_eq!(handlers, 0, "bash emits no handler facts");
    assert_eq!(assertions, 0, "bash emits no assertion facts");
    let (plain_handlers, plain_assertions) = handler_assertion_counts("[[ -n $x ]] && echo yes\n");
    assert_eq!(plain_handlers, 0, "conditional file still has no handlers");
    assert_eq!(
        plain_assertions, 0,
        "conditional file still has no assertions"
    );
}

fn ambient_of(source: &str) -> Vec<(AmbientKind, String, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Bash,
        path: "deploy.sh",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "bash parses");
    analysis
        .facts
        .ambient
        .iter()
        .map(|a| (a.kind, a.name.to_owned(), a.in_function.is_some()))
        .collect()
}

#[test]
fn in_function_let_write_to_declared_name_is_one_global_write() {
    let ambient = ambient_of("declare count=0\nf() {\nlet count=count+1\n}\n");
    let mutables: Vec<_> = ambient
        .iter()
        .filter(|(kind, _, _)| *kind == AmbientKind::GlobalMutable)
        .collect();
    assert_eq!(mutables.len(), 1, "one declared name is a global mutable");
    assert_eq!(mutables[0].1, "count", "mutable keeps the declared name");
    let writes: Vec<_> = ambient
        .iter()
        .filter(|(kind, _, _)| *kind == AmbientKind::GlobalWrite)
        .collect();
    assert_eq!(writes.len(), 1, "in-function let joins the declared name");
    assert_eq!(writes[0].1, "count", "write keeps the assigned name");
    assert!(writes[0].2, "write is attributed inside the function");
}

#[test]
fn local_shadowing_a_declared_name_writes_no_global() {
    let ambient = ambient_of("declare count=0\nf() {\nlocal count=5\nlet count=count+1\n}\n");
    let writes: Vec<_> = ambient
        .iter()
        .filter(|(kind, _, _)| *kind == AmbientKind::GlobalWrite)
        .collect();
    assert!(writes.is_empty(), "local shadow means no global write");
}

#[test]
fn plain_assignment_before_let_still_writes_one_global() {
    let ambient = ambient_of("declare count=0\nf() {\ncount=5\nlet count=count+1\n}\n");
    let writes: Vec<_> = ambient
        .iter()
        .filter(|(kind, _, _)| *kind == AmbientKind::GlobalWrite)
        .collect();
    assert_eq!(
        writes.len(),
        1,
        "plain assignment is not a shadowing declaration"
    );
    assert_eq!(writes[0].1, "count", "write keeps the assigned name");
}

#[test]
fn concatenation_carries_longest_static_run() {
    let long = shapes_of("logger finished$rest\n");
    assert_eq!(long.len(), 1, "log call recorded");
    assert_eq!(long[0][0], Shape::Concat(8), "eight-char run is carried");
    let short = shapes_of("logger ready$prefix\n");
    assert_eq!(short.len(), 1, "short call recorded");
    assert_eq!(short[0][0], Shape::Concat(5), "five-char run is carried");
    let dynamic = shapes_of("logger $prefix$suffix\n");
    assert_eq!(dynamic.len(), 1, "dynamic call recorded");
    assert_eq!(dynamic[0][0], Shape::Concat(0), "no static run is zero");
}

#[test]
fn plain_command_name_carries_path_root() {
    let rows = calls_of("ls -la\n");
    assert_eq!(rows.len(), 1, "command call recorded");
    assert_eq!(rows[0].0, "ls", "callee is the command name");
    assert_eq!(
        rows[0].1.as_deref(),
        Some("ls"),
        "bare word is its own root"
    );
    assert!(rows[0].2, "bare word is a simple path");
    let path_rows = calls_of("./deploy.sh --check\n");
    assert_eq!(path_rows.len(), 1, "path command recorded");
    assert!(path_rows[0].1.is_none(), "path has no identifier root");
    assert!(!path_rows[0].2, "path is not a simple path");
}

#[test]
fn variable_argument_uses_bare_name() {
    let rows = shapes_of("logger $name\n");
    assert_eq!(rows.len(), 1, "log call recorded");
    assert_eq!(rows[0].len(), 1, "one message argument recorded");
    assert_eq!(
        rows[0][0],
        Shape::Ident("name".to_string()),
        "ident drops the sigil"
    );
    let word_rows = shapes_of("logger name\n");
    assert_eq!(word_rows.len(), 1, "word call recorded");
    assert_eq!(word_rows[0][0], Shape::Str, "bare word stays a literal");
}

#[test]
fn global_decl_forms_in_function_do_not_shadow() {
    let sources = [
        "declare count=0\nf() {\nexport count=5\nlet count=count+1\n}\n",
        "declare count=0\nf() {\ndeclare -g count=5\nlet count=count+1\n}\n",
    ];
    for source in sources {
        let writes: Vec<_> = ambient_of(source)
            .into_iter()
            .filter(|(kind, name, _)| *kind == AmbientKind::GlobalWrite && name == "count")
            .collect();
        assert_eq!(writes.len(), 1, "global form leaves one write");
    }
    let plain = "declare count=0\nf() {\ndeclare count=5\nlet count=count+1\n}\n";
    let local_writes: Vec<_> = ambient_of(plain)
        .into_iter()
        .filter(|(kind, name, _)| *kind == AmbientKind::GlobalWrite && name == "count")
        .collect();
    assert!(local_writes.is_empty(), "plain declare still shadows");
}

#[test]
fn quoted_variable_argument_uses_bare_name() {
    let rows = shapes_of("logger \"$name\"\n");
    assert_eq!(rows.len(), 1, "log call recorded");
    assert_eq!(
        rows[0][0],
        Shape::Ident("name".to_string()),
        "quoted ident drops the sigil"
    );
    let mixed = shapes_of("logger \"hi $name\"\n");
    assert_eq!(mixed.len(), 1, "mixed call recorded");
    assert_eq!(mixed[0][0], Shape::Str, "static text keeps Str");
}

#[test]
fn compound_arguments_are_never_collection() {
    let rows = shapes_of("cmd $((1 + 2))\n");
    assert_eq!(rows.len(), 1, "call recorded");
    assert_eq!(rows[0][0], Shape::Other, "arithmetic stays Other");
    let words = shapes_of("cmd foo\n");
    assert_eq!(words.len(), 1, "word call recorded");
    assert_eq!(words[0][0], Shape::Str, "word stays Str");
}

#[test]
fn typeset_in_function_shadows_declared_name() {
    let ambient = ambient_of("declare count=0\nf() {\ntypeset count=5\nlet count=count+1\n}\n");
    let writes: Vec<_> = ambient
        .iter()
        .filter(|(kind, name, _)| *kind == AmbientKind::GlobalWrite && name == "count")
        .collect();
    assert!(writes.is_empty(), "typeset shadows like local");
}

#[test]
fn local_in_branch_shadows_later_let_in_function() {
    let sources = [
        "declare count=0\nf() {\nif [ -n \"$1\" ]; then\nlocal count=5\nfi\nlet count=count+1\n}\n",
        "declare count=0\nf() {\nfor i in 1 2; do\nlocal count=5\ndone\nlet count=count+1\n}\n",
    ];
    for source in sources {
        let writes: Vec<_> = ambient_of(source)
            .into_iter()
            .filter(|(kind, name, _)| *kind == AmbientKind::GlobalWrite && name == "count")
            .collect();
        assert!(
            writes.is_empty(),
            "branch-local still shadows at function scope"
        );
    }
}

#[test]
fn branch_local_of_other_name_keeps_global_write() {
    let ambient = ambient_of(
        "declare count=0\nf() {\nif [ -n \"$1\" ]; then\nlocal other=5\nfi\nlet count=count+1\n}\n",
    );
    let writes: Vec<_> = ambient
        .iter()
        .filter(|(kind, name, _)| *kind == AmbientKind::GlobalWrite && name == "count")
        .collect();
    assert_eq!(writes.len(), 1, "unshadowed name still writes");
}

#[test]
fn option_words_are_recorded_as_arguments() {
    let rows = shapes_of("echo -n \"Enter password: \"\n");
    assert_eq!(rows.len(), 1, "call recorded");
    assert_eq!(rows[0].len(), 2, "option stays an argument");
    assert_eq!(rows[0][0], Shape::Str, "option is a literal run");
    assert_eq!(rows[0][1], Shape::Str, "message stays a literal");
}

#[test]
fn plain_message_keeps_a_single_argument() {
    let plain = shapes_of("logger \"Backup finished\"\n");
    assert_eq!(plain.len(), 1, "call recorded");
    assert_eq!(plain[0].len(), 1, "plain message keeps one arg");
    assert_eq!(plain[0][0], Shape::Str, "message stays a literal");
}

#[test]
fn nested_top_level_declare_feeds_let_write() {
    let ambient =
        ambient_of("if [ -f conf ]; then\ndeclare count=0\nfi\nf() {\nlet count=count+1\n}\n");
    let mutables: Vec<_> = ambient
        .iter()
        .filter(|(kind, name, _)| *kind == AmbientKind::GlobalMutable && name == "count")
        .collect();
    assert_eq!(
        mutables.len(),
        1,
        "nested declare is still a global mutable"
    );
    let writes: Vec<_> = ambient
        .iter()
        .filter(|(kind, name, _)| *kind == AmbientKind::GlobalWrite && name == "count")
        .collect();
    assert_eq!(writes.len(), 1, "nested declare still feeds the write");
}

#[test]
fn let_on_undeclared_name_writes_nothing() {
    let ambient =
        ambient_of("if [ -f conf ]; then\ndeclare count=0\nfi\nf() {\nlet other=other+1\n}\n");
    let writes: Vec<_> = ambient
        .iter()
        .filter(|(kind, _, _)| *kind == AmbientKind::GlobalWrite)
        .collect();
    assert!(writes.is_empty(), "undeclared name writes nothing");
}
