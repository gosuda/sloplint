//! Walker and parse-stage contracts observed through `analyze` only.
//!
//! One child module per mechanism, so each fix lands with its own case module and
//! one declaration here, while the crate links a single integration-test executable.

#[path = "syntax_kernel_contracts/declaration_scopes.rs"]
mod declaration_scopes;
#[path = "syntax_kernel_contracts/directive_docs.rs"]
mod directive_docs;
#[path = "syntax_kernel_contracts/header_comment_chains.rs"]
mod header_comment_chains;
#[path = "syntax_kernel_contracts/support.rs"]
mod support;
