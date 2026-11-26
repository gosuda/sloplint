//! Grammar-gap regression tests: valid source the published grammar rejects,
//! observed through `analyze` plus a raw-tree check in each module's
//! `run_clean`, because a hidden MISSING recovery node sets `has_error`
//! without producing a cursor-visible error position.
//!
//! One child module per vendored grammar, so each fix lands with its own case
//! module and one declaration here, while the crate links a single
//! integration-test executable. Each module names the published-grammar
//! version it fails against, so a re-vendor that drops a patch shows up here.

#[path = "grammar_gaps/alloy.rs"]
mod alloy;
#[path = "grammar_gaps/bash.rs"]
mod bash;
#[path = "grammar_gaps/c.rs"]
mod c;
#[path = "grammar_gaps/cpp.rs"]
mod cpp;
#[path = "grammar_gaps/cpp_macros.rs"]
mod cpp_macros;
#[path = "grammar_gaps/csharp.rs"]
mod csharp;
#[path = "grammar_gaps/dafny.rs"]
mod dafny;
#[path = "grammar_gaps/dafny_match.rs"]
mod dafny_match;
#[path = "grammar_gaps/dart.rs"]
mod dart;
#[path = "grammar_gaps/go.rs"]
mod go;
#[path = "grammar_gaps/java.rs"]
mod java;
#[path = "grammar_gaps/javascript_typescript.rs"]
mod javascript_typescript;
#[path = "grammar_gaps/javascript_typescript_type_syntax.rs"]
mod javascript_typescript_type_syntax;
#[path = "grammar_gaps/kotlin.rs"]
mod kotlin;
#[path = "grammar_gaps/kotlin_round_notes.rs"]
mod kotlin_round_notes;
#[path = "grammar_gaps/lean.rs"]
mod lean;
#[path = "grammar_gaps/logos.rs"]
mod logos;
#[path = "grammar_gaps/objc.rs"]
mod objc;
#[path = "grammar_gaps/ocaml.rs"]
mod ocaml;
#[path = "grammar_gaps/ocaml_arms.rs"]
mod ocaml_arms;
#[path = "grammar_gaps/ocaml_cppo.rs"]
mod ocaml_cppo;
#[path = "grammar_gaps/odin.rs"]
mod odin;
#[path = "grammar_gaps/php.rs"]
mod php;
#[path = "grammar_gaps/powershell.rs"]
mod powershell;
#[path = "grammar_gaps/powershell_continuations.rs"]
mod powershell_continuations;
#[path = "grammar_gaps/powershell_literal_runs.rs"]
mod powershell_literal_runs;
#[path = "grammar_gaps/powershell_plain_escapes.rs"]
mod powershell_plain_escapes;
#[path = "grammar_gaps/powershell_separators.rs"]
mod powershell_separators;
#[path = "grammar_gaps/powershell_syntax.rs"]
mod powershell_syntax;
#[path = "grammar_gaps/powershell_words.rs"]
mod powershell_words;
#[path = "grammar_gaps/python.rs"]
mod python;
#[path = "grammar_gaps/quint.rs"]
mod quint;
#[path = "grammar_gaps/racket.rs"]
mod racket;
#[path = "grammar_gaps/ruby.rs"]
mod ruby;
#[path = "grammar_gaps/rust.rs"]
mod rust;
#[path = "grammar_gaps/scala.rs"]
mod scala;
#[path = "grammar_gaps/scala_ops.rs"]
mod scala_ops;
#[path = "grammar_gaps/scala_softkw.rs"]
mod scala_softkw;
#[path = "grammar_gaps/starlark.rs"]
mod starlark;
#[path = "grammar_gaps/swift.rs"]
mod swift;
#[path = "grammar_gaps/tlaplus.rs"]
mod tlaplus;
#[path = "grammar_gaps/zig.rs"]
mod zig;
