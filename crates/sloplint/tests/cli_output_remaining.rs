//! CLI output, default scope, and stdout buffering, observed through the real binary only.
//!
//! One child module per mechanism, so each fix lands with its own case module and one
//! declaration here, while the crate links a single integration-test executable.

#[path = "cli_output_remaining/default_scope.rs"]
mod default_scope;
#[path = "cli_output_remaining/human_source_lines.rs"]
mod human_source_lines;
#[path = "cli_output_remaining/output_stream.rs"]
mod output_stream;
#[path = "cli_output_remaining/terminal_escapes.rs"]
mod terminal_escapes;
