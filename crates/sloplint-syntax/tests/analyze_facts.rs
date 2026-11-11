//! Walker and parse-stage contracts observed through `analyze` only.

#[path = "analyze_facts/bash_scopes.rs"]
mod bash_scopes;
#[path = "analyze_facts/c_scopes.rs"]
mod c_scopes;
#[path = "analyze_facts/callback_functions.rs"]
mod callback_functions;
#[path = "analyze_facts/cpp_scopes.rs"]
mod cpp_scopes;
#[path = "analyze_facts/csharp_scopes.rs"]
mod csharp_scopes;
#[path = "analyze_facts/dart_scopes.rs"]
mod dart_scopes;
#[path = "analyze_facts/declaration_scopes.rs"]
mod declaration_scopes;
#[path = "analyze_facts/directive_docs.rs"]
mod directive_docs;
#[path = "analyze_facts/else_if_chains.rs"]
mod else_if_chains;
#[path = "analyze_facts/global_write_scope_end.rs"]
mod global_write_scope_end;
#[path = "analyze_facts/global_write_shadow.rs"]
mod global_write_shadow;
#[path = "analyze_facts/go_block_bindings.rs"]
mod go_block_bindings;
#[path = "analyze_facts/has_value_return.rs"]
mod has_value_return;
#[path = "analyze_facts/header_comment_chains.rs"]
mod header_comment_chains;
#[path = "analyze_facts/header_fallbacks.rs"]
mod header_fallbacks;
#[path = "analyze_facts/is_method.rs"]
mod is_method;
#[path = "analyze_facts/java_scopes.rs"]
mod java_scopes;
#[path = "analyze_facts/kotlin_scopes.rs"]
mod kotlin_scopes;
#[path = "analyze_facts/objc_scopes.rs"]
mod objc_scopes;
#[path = "analyze_facts/ocaml_r_scopes.rs"]
mod ocaml_r_scopes;
#[path = "analyze_facts/odin_scopes.rs"]
mod odin_scopes;
#[path = "analyze_facts/parse_error_position.rs"]
mod parse_error_position;
#[path = "analyze_facts/php_scopes.rs"]
mod php_scopes;
#[path = "analyze_facts/powershell_scopes.rs"]
mod powershell_scopes;
#[path = "analyze_facts/python_scopes.rs"]
mod python_scopes;
#[path = "analyze_facts/ruby_scopes.rs"]
mod ruby_scopes;
#[path = "analyze_facts/rust_scopes.rs"]
mod rust_scopes;
#[path = "analyze_facts/scala_scopes.rs"]
mod scala_scopes;
#[path = "analyze_facts/sole_call_forwarding.rs"]
mod sole_call_forwarding;
#[path = "analyze_facts/starlark_scopes.rs"]
mod starlark_scopes;
#[path = "analyze_facts/support.rs"]
mod support;
#[path = "analyze_facts/swift_scopes.rs"]
mod swift_scopes;
#[path = "analyze_facts/swift_throws.rs"]
mod swift_throws;
#[path = "analyze_facts/walk_scaling.rs"]
mod walk_scaling;
#[path = "analyze_facts/web_scopes.rs"]
mod web_scopes;
#[path = "analyze_facts/zig_scopes.rs"]
mod zig_scopes;
