//! CLI output, default scope, and stdout buffering, observed through the real binary only.

#[path = "cli_output/agent_render_order.rs"]
mod agent_render_order;
#[path = "cli_output/broken_pipe.rs"]
mod broken_pipe;
#[path = "cli_output/default_scope.rs"]
mod default_scope;
#[path = "cli_output/human_source_lines.rs"]
mod human_source_lines;
#[path = "cli_output/output_stream.rs"]
mod output_stream;
#[path = "cli_output/terminal_escapes.rs"]
mod terminal_escapes;
