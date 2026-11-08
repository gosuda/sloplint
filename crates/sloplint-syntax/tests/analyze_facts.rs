//! Walker and parse-stage contracts observed through `analyze` only.

#[path = "analyze_facts/declaration_scopes.rs"]
mod declaration_scopes;
#[path = "analyze_facts/directive_docs.rs"]
mod directive_docs;
#[path = "analyze_facts/else_if_chains.rs"]
mod else_if_chains;
#[path = "analyze_facts/header_comment_chains.rs"]
mod header_comment_chains;
#[path = "analyze_facts/header_fallbacks.rs"]
mod header_fallbacks;
#[path = "analyze_facts/parse_error_position.rs"]
mod parse_error_position;
#[path = "analyze_facts/sole_call_forwarding.rs"]
mod sole_call_forwarding;
#[path = "analyze_facts/support.rs"]
mod support;
#[path = "analyze_facts/swift_throws.rs"]
mod swift_throws;
#[path = "analyze_facts/walk_scaling.rs"]
mod walk_scaling;
