//! Pipeline I/O, scope, cache, and root-resolution boundaries, observed through `run`, `execute`, and the real binary.
//!
//! One child module per mechanism, so each fix lands with its own case module and one
//! declaration here, while the crate links a single integration-test executable.

#[path = "pipeline_remaining/baseline_replace.rs"]
mod baseline_replace;
#[path = "pipeline_remaining/init_failure.rs"]
mod init_failure;
#[path = "pipeline_remaining/parse_failed_suppressions.rs"]
mod parse_failed_suppressions;
#[path = "pipeline_remaining/path_scope.rs"]
mod path_scope;
#[path = "pipeline_remaining/ratchet_rule_filter.rs"]
mod ratchet_rule_filter;
#[path = "pipeline_remaining/regular_inputs.rs"]
mod regular_inputs;
#[path = "pipeline_remaining/skipped_names.rs"]
mod skipped_names;
