//! Walker and parse-stage contracts observed through `analyze` only.
//!
//! One child module per mechanism, so each fix lands with its own case module and
//! one declaration here, while the crate links a single integration-test executable.

#[path = "syntax_kernel_contracts/declaration_scopes.rs"]
mod declaration_scopes;
#[path = "syntax_kernel_contracts/directive_docs.rs"]
mod directive_docs;
#[path = "syntax_kernel_contracts/else_if_chains.rs"]
mod else_if_chains;
#[path = "syntax_kernel_contracts/header_comment_chains.rs"]
mod header_comment_chains;
#[path = "syntax_kernel_contracts/header_cpp_fallback.rs"]
mod header_cpp_fallback;
#[path = "syntax_kernel_contracts/header_fallbacks.rs"]
mod header_fallbacks;
#[path = "syntax_kernel_contracts/parse_error_position.rs"]
mod parse_error_position;
#[path = "syntax_kernel_contracts/sole_call_forwarding.rs"]
mod sole_call_forwarding;
#[path = "syntax_kernel_contracts/support.rs"]
mod support;
#[path = "syntax_kernel_contracts/swift_throws.rs"]
mod swift_throws;
#[path = "syntax_kernel_contracts/walk_scaling.rs"]
mod walk_scaling;
