//! Grammar-gap regression tests: valid source the published grammar rejects,
//! observed through `analyze` only.
//!
//! One child module per vendored grammar, so each fix lands with its own case
//! module and one declaration here, while the crate links a single
//! integration-test executable. Each module names the published-grammar
//! version it fails against, so a re-vendor that drops a patch shows up here.

#[path = "grammar_gaps/bash.rs"]
mod bash;
#[path = "grammar_gaps/c.rs"]
mod c;
#[path = "grammar_gaps/cpp.rs"]
mod cpp;
#[path = "grammar_gaps/go.rs"]
mod go;
#[path = "grammar_gaps/java.rs"]
mod java;
#[path = "grammar_gaps/javascript_typescript.rs"]
mod javascript_typescript;
#[path = "grammar_gaps/powershell.rs"]
mod powershell;
#[path = "grammar_gaps/python.rs"]
mod python;
#[path = "grammar_gaps/ruby.rs"]
mod ruby;
#[path = "grammar_gaps/rust.rs"]
mod rust;
#[path = "grammar_gaps/starlark.rs"]
mod starlark;
