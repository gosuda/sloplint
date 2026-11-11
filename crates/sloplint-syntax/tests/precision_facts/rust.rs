use sloplint_core::facts::{ArgShape, ImportKind};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

struct CallFacts {
    callee: String,
    root: Option<String>,
    simple: bool,
    init: bool,
    args: Vec<ArgShape<'static>>,
}

fn calls_of(source: &str) -> Vec<CallFacts> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "main.rs",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "rust parses");
    analysis
        .facts
        .calls
        .iter()
        .map(|c| CallFacts {
            callee: c.callee.to_owned(),
            root: c.callee_root.map(str::to_owned),
            simple: c.is_simple_path,
            init: c.initializes_binding,
            args: c
                .args
                .iter()
                .map(|a| match &a.shape {
                    ArgShape::Str { longest_static } => ArgShape::Str {
                        longest_static: *longest_static,
                    },
                    ArgShape::Concat { longest_static } => ArgShape::Concat {
                        longest_static: *longest_static,
                    },
                    ArgShape::Ident(_) => ArgShape::Ident(""),
                    ArgShape::Collection => ArgShape::Collection,
                    ArgShape::CodeBody => ArgShape::CodeBody,
                    ArgShape::Other => ArgShape::Other,
                })
                .collect(),
        })
        .collect()
}

struct ImportFacts {
    name: String,
    alias: Option<String>,
    kind: ImportKind,
    root: Option<String>,
    scope: String,
}
fn imports_of(source: &str) -> Vec<ImportFacts> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "main.rs",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "rust parses");
    analysis
        .facts
        .imports
        .iter()
        .map(|i| ImportFacts {
            name: i.name.to_owned(),
            alias: i.alias.map(str::to_owned),
            kind: i.kind,
            root: i.external_root.map(str::to_owned),
            scope: format!("{:?}", i.scope),
        })
        .collect()
}

#[test]
fn receiver_call_text_is_dropped_from_callee() {
    let calls = calls_of(
        "fn f(ctx: Ctx) {\n    ctx.log().error(\"connection dropped\");\n    log.error(\"plain path\");\n}\n",
    );
    let chained = calls
        .iter()
        .find(|c| c.callee == "error")
        .expect("chained call found");
    assert!(
        !chained.simple,
        "call through a receiver call is not a simple path"
    );
    assert!(
        chained.root.is_none(),
        "call through a receiver call has no root"
    );
    let plain = calls
        .iter()
        .find(|c| c.callee == "log.error")
        .expect("plain call found");
    assert!(plain.simple, "plain dotted path stays simple");
    assert!(
        plain.root.as_deref() == Some("log"),
        "plain dotted path keeps its root"
    );
}

#[test]
fn initializes_binding_needs_a_plain_identifier() {
    let calls = calls_of(
        "fn f() {\n    let cfg = build();\n    let mut other = build();\n    let (a, b) = build();\n    let _ = build();\n    const C: usize = build();\n}\n",
    );
    assert!(
        calls.len() == 5,
        "five build calls observed, got {}",
        calls.len()
    );
    assert!(calls[0].init, "plain let binding initializes");
    assert!(calls[1].init, "mut let binding initializes");
    assert!(
        !calls[2].init,
        "destructuring pattern is not a named binding"
    );
    assert!(!calls[3].init, "wildcard pattern is not a named binding");
    assert!(calls[4].init, "const item initializes a named binding");
}

#[test]
fn keyword_roots_are_simple_paths() {
    let calls = calls_of(
        "impl R {\n    fn f(&self, name: &str) {\n        self.render.format(name);\n    }\n}\nfn g(n: &str) {\n    crate::render::greeting(n);\n    render::greeting(n);\n}\n",
    );
    let slf = calls
        .iter()
        .find(|c| c.callee == "self.render.format")
        .expect("self call found");
    assert!(slf.simple, "self-rooted chain is a simple path");
    assert!(
        slf.root.as_deref() == Some("self"),
        "self is the chain root"
    );
    let krate = calls
        .iter()
        .find(|c| c.callee == "crate::render::greeting")
        .expect("crate call found");
    assert!(krate.simple, "crate-rooted chain is a simple path");
    assert!(
        krate.root.as_deref() == Some("crate"),
        "crate is the chain root"
    );
    let plain = calls
        .iter()
        .find(|c| c.callee == "render::greeting")
        .expect("relative call found");
    assert!(
        plain.root.as_deref() == Some("render"),
        "relative path keeps its root"
    );
}

#[test]
fn every_macro_group_gets_a_shape() {
    let calls = calls_of(
        "fn f(id: Id, msg: Msg) {\n    tracing::info!(user_id = id, \"Logged in\");\n    info!(\"{} done\", msg);\n    error!(r\"connection refused by peer\");\n}\n",
    );
    let traced = calls
        .iter()
        .find(|c| c.callee == "tracing::info")
        .expect("traced call found");
    assert!(traced.args.len() == 2, "field and message groups observed");
    assert!(
        matches!(traced.args[0], ArgShape::Other),
        "field group stays neutral"
    );
    assert!(
        matches!(traced.args[1], ArgShape::Str { longest_static: 9 }),
        "message group carries its literal"
    );
    let named = calls
        .iter()
        .find(|c| c.callee == "info")
        .expect("info call found");
    assert!(
        matches!(named.args[0], ArgShape::Str { .. }),
        "format message is a string"
    );
    assert!(
        matches!(named.args[1], ArgShape::Ident(_)),
        "trailing identifier stays an identifier"
    );
    let raw = calls
        .iter()
        .find(|c| c.callee == "error")
        .expect("error call found");
    assert!(
        matches!(raw.args[0], ArgShape::Str { longest_static: 26 }),
        "raw string message carries its literal"
    );
}

#[test]
fn format_macro_argument_is_concat() {
    let calls = calls_of(
        "fn f(path: &str, err: &str) {\n    logger.error(format!(\"failed to open {}\", path));\n    logger.error(make_msg(path));\n    logger.error(format!(\"oops: {}\", err));\n    logger.error(format!(\"{}\", err));\n}\n",
    );
    let errors: Vec<_> = calls
        .iter()
        .filter(|c| c.callee == "logger.error")
        .collect();
    assert!(
        errors.len() == 4,
        "four logger calls observed, got {}",
        errors.len()
    );
    assert!(
        matches!(errors[0].args[0], ArgShape::Concat { longest_static: 15 }),
        "long static run is measured"
    );
    assert!(
        matches!(errors[1].args[0], ArgShape::Other),
        "plain call argument stays neutral"
    );
    assert!(
        matches!(errors[2].args[0], ArgShape::Concat { longest_static: 6 }),
        "short static run is measured"
    );
    assert!(
        matches!(errors[3].args[0], ArgShape::Concat { longest_static: 0 }),
        "all-dynamic interpolation measures zero"
    );
}

#[test]
fn nested_plus_chain_keeps_static_operand() {
    let calls = calls_of(
        "fn f(path: &str, suffix: &str) {\n    logger.error(\"failed to open file: \" + path + suffix);\n    logger.error(path + suffix);\n}\n",
    );
    assert!(calls.len() == 2, "two logger calls observed");
    assert!(
        matches!(calls[0].args[0], ArgShape::Concat { longest_static: 21 }),
        "left-nested static is measured"
    );
    assert!(
        matches!(calls[1].args[0], ArgShape::Other),
        "dynamic-only chain stays neutral"
    );
}

#[test]
fn deep_plus_chain_still_measures_static() {
    let calls = calls_of(
        "fn f(s: &str, a: &str, b: &str, c: &str, d: &str, e: &str, f: &str, g: &str, h: &str, i: &str) {\n    logger.error(s + \"static text here\" + a + b + c + d + e + f + g + h + i);\n    logger.error(a + b + c);\n}\n",
    );
    assert!(calls.len() == 2, "two logger calls observed");
    assert!(
        matches!(calls[0].args[0], ArgShape::Concat { longest_static: 16 }),
        "deep static is measured"
    );
    assert!(
        matches!(calls[1].args[0], ArgShape::Other),
        "dynamic-only chain stays neutral"
    );
}

#[test]
fn block_and_borrowed_array_arguments() {
    let calls = calls_of(
        "fn f(list: &[u8]) {\n    spawn(async move { work().await });\n    check(&[1, 2, 3]);\n    check(&list);\n}\n",
    );
    let spawn = calls
        .iter()
        .find(|c| c.callee == "spawn")
        .expect("spawn call found");
    assert!(
        matches!(spawn.args[0], ArgShape::CodeBody),
        "async block is a body"
    );
    let checks: Vec<_> = calls.iter().filter(|c| c.callee == "check").collect();
    assert!(
        checks.len() == 2,
        "two check calls observed, got {}",
        checks.len()
    );
    assert!(
        matches!(checks[0].args[0], ArgShape::Collection),
        "borrowed array is a collection"
    );
    assert!(
        matches!(checks[1].args[0], ArgShape::Other),
        "borrowed slice stays neutral"
    );
}

#[test]
fn underscore_alias_import_is_kept() {
    let imports =
        imports_of("use rayon::prelude::ParallelIterator as _;\nuse std::fmt::Display;\n");
    let kept = imports
        .iter()
        .find(|i| i.name == "ParallelIterator")
        .expect("aliased import kept");
    assert!(
        kept.alias.as_deref() == Some("_"),
        "underscore alias is recorded"
    );
    assert!(kept.kind == ImportKind::Named, "aliased import stays named");
    assert!(
        kept.root.as_deref() == Some("rayon"),
        "third-party root is counted"
    );
    let std = imports
        .iter()
        .find(|i| i.name == "Display")
        .expect("std import found");
    assert!(std.root.is_none(), "stdlib import has no external root");
}

#[test]
fn same_file_child_import_has_no_external_root() {
    let imports = imports_of(
        "mod parser;\nmod lexer;\nuse parser::Token;\nuse lexer::Lexer;\nuse serde::Serialize;\n",
    );
    for name in ["Token", "Lexer"] {
        let local = imports
            .iter()
            .find(|i| i.name == name)
            .expect("local import found");
        assert!(
            local.root.is_none(),
            "same-file module import has no external root"
        );
    }

    let third = imports
        .iter()
        .find(|i| i.name == "Serialize")
        .expect("third-party import found");
    assert!(
        third.root.as_deref() == Some("serde"),
        "third-party root is counted"
    );
}

#[test]
fn struct_literal_argument_is_not_a_collection() {
    let calls =
        calls_of("fn f() {\n    register(Config { name: \"x\" });\n    register((1, 2));\n}\n");
    let record = calls
        .iter()
        .find(|c| c.callee == "register" && matches!(c.args[0], ArgShape::Other))
        .expect("struct call found");
    assert!(
        matches!(record.args[0], ArgShape::Other),
        "struct literal stays neutral"
    );
    let tuple = calls
        .iter()
        .find(|c| c.callee == "register" && matches!(c.args[0], ArgShape::Collection))
        .expect("tuple call found");
    assert!(
        matches!(tuple.args[0], ArgShape::Collection),
        "tuple literal stays a collection"
    );
}

#[test]
fn collection_holding_a_closure_is_a_body() {
    let calls = calls_of(
        "fn f() {\n    register([|dev| mount(dev), |dev| unmount(dev)]);\n    register([open_disk(), close_disk()]);\n}\n",
    );
    let code = calls
        .iter()
        .find(|c| c.callee == "register" && matches!(c.args[0], ArgShape::CodeBody))
        .expect("closure call found");
    assert!(
        matches!(code.args[0], ArgShape::CodeBody),
        "closure collection reads as a body"
    );
    let plain = calls
        .iter()
        .find(|c| c.callee == "register" && matches!(c.args[0], ArgShape::Collection))
        .expect("plain call found");
    assert!(
        matches!(plain.args[0], ArgShape::Collection),
        "call collection stays a collection"
    );
}

#[test]
fn call_and_index_callees_carry_no_argument_text() {
    let calls = calls_of(
        "fn f(cfg: Cfg, input: In, arr: Arr, i: usize) {\n    build(cfg)(input);\n    arr[i](input);\n    build(cfg);\n    (build)(cfg);\n}\n",
    );
    assert!(
        calls.len() == 5,
        "outer and inner calls observed, got {}",
        calls.len()
    );
    let chained: Vec<_> = calls.iter().filter(|c| c.callee.is_empty()).collect();
    assert!(
        chained.len() == 2,
        "call and index callees are empty, got {}",
        chained.len()
    );
    let plain: Vec<_> = calls.iter().filter(|c| c.callee == "build").collect();
    assert!(
        plain.len() == 3,
        "plain and parenthesized callees keep text, got {}",
        plain.len()
    );
    assert!(
        plain.iter().all(|c| c.simple),
        "plain callees are simple paths"
    );
}

#[test]
fn interior_comments_are_not_arguments() {
    let calls = calls_of(
        "fn greet(n: &str, m: &str) -> String {\n    render_greeting(\n        // keep the wording stable\n        n,\n    )\n}\nfn plain(m: &str) -> String {\n    render_greeting(m)\n}\nfn log(x: &str) {\n    info!(/* note */ \"v{}\", x);\n    info!(\"v{}\", x);\n}\n",
    );
    let rendered: Vec<_> = calls
        .iter()
        .filter(|c| c.callee == "render_greeting")
        .collect();
    assert!(
        rendered.len() == 2,
        "both wrappers observed, got {}",
        rendered.len()
    );
    for call in rendered {
        assert!(call.args.len() == 1, "comment is not an argument");
        assert!(
            matches!(call.args[0], ArgShape::Ident(_)),
            "name argument keeps its shape"
        );
    }
    let infos: Vec<_> = calls.iter().filter(|c| c.callee == "info").collect();
    assert!(
        infos.len() == 2,
        "both macros observed, got {}",
        infos.len()
    );
    for info in infos {
        assert!(info.args.len() == 2, "comment adds no group");
        assert!(
            matches!(info.args[0], ArgShape::Str { .. }),
            "message keeps its shape"
        );
        assert!(
            matches!(info.args[1], ArgShape::Ident(_)),
            "value keeps its shape"
        );
    }
}

#[test]
fn reference_and_conversion_arguments_reuse_inner_shapes() {
    let calls = calls_of(
        "fn f(path: &str, list: &[u8]) {\n    logger.error(&format!(\"failed to open {}\", path));\n    register(&vec![1, 2]);\n    logger.error(&path);\n    logger.error(\"failed\".to_string());\n    logger.error(String::from(\"oops\"));\n}\n",
    );
    let errors: Vec<_> = calls
        .iter()
        .filter(|c| c.callee == "logger.error")
        .collect();
    assert!(
        errors.len() == 4,
        "four logger calls observed, got {}",
        errors.len()
    );
    assert!(
        matches!(errors[0].args[0], ArgShape::Concat { longest_static: 15 }),
        "borrowed format measures its static run"
    );
    assert!(
        matches!(errors[1].args[0], ArgShape::Other),
        "borrowed ident stays neutral"
    );
    assert!(
        matches!(errors[2].args[0], ArgShape::Str { longest_static: 6 }),
        "to_string keeps the literal"
    );
    assert!(
        matches!(errors[3].args[0], ArgShape::Str { longest_static: 4 }),
        "String::from keeps the literal"
    );
    let regs: Vec<_> = calls.iter().filter(|c| c.callee == "register").collect();
    assert!(
        regs.len() == 1,
        "register call observed, got {}",
        regs.len()
    );
    assert!(
        matches!(regs[0].args[0], ArgShape::Collection),
        "borrowed vec stays a collection"
    );
}

fn arms_of(source: &str) -> Vec<(bool, u32, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "main.rs",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "rust parses");
    analysis
        .facts
        .wildcard_arms
        .iter()
        .map(|a| (a.enum_like, a.variant_count, a.has_nullable_ctor))
        .collect()
}

#[test]
fn user_enum_ok_variant_counts_as_variant() {
    let arms = arms_of(
        "enum Status { Ok, Pending, Done }\nfn f(s: Status) -> u8 {\n    match s { Status::Ok => 1, Status::Pending => 2, Status::Done => 3, _ => 0 }\n}\n",
    );
    assert!(arms.len() == 1, "one wildcard observed, got {}", arms.len());
    assert!(arms[0].0, "user-enum wildcard is enum-like");
    assert!(arms[0].1 == 3, "three variants counted, got {}", arms[0].1);
    assert!(!arms[0].2, "user-enum Ok is not nullable");
}

#[test]
fn bare_option_ctors_stay_nullable() {
    let arms = arms_of(
        "fn f(o: Option<u8>) -> u8 {\n    match o { Some(x) => x, None => 0, _ => 0 }\n}\n",
    );
    assert!(arms.len() == 1, "one wildcard observed, got {}", arms.len());
    assert!(!arms[0].0, "option match is not enum-like");
    assert!(arms[0].1 == 0, "nullable ctors skipped, got {}", arms[0].1);
    assert!(arms[0].2, "bare Some and None stay nullable");
}

#[test]
fn function_local_glob_stays_wildcard() {
    let imports = imports_of("fn f() {\n    use crate::x::*;\n    use crate::x::Y;\n}\n");
    let glob = imports.iter().find(|i| i.name == "*").expect("glob found");
    assert!(
        glob.kind == ImportKind::Wildcard,
        "function-local glob stays a wildcard"
    );
    assert!(
        glob.scope == "FunctionLocal",
        "function-local scope is recorded"
    );
    assert!(
        glob.root.is_none(),
        "crate-rooted glob has no external root"
    );
    let named = imports
        .iter()
        .find(|i| i.name == "Y")
        .expect("named import found");
    assert!(
        named.kind == ImportKind::Named,
        "function-local named import stays named"
    );
    assert!(
        named.scope == "FunctionLocal",
        "function-local scope is recorded"
    );
}
