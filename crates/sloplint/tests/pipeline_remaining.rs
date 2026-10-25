//! Pipeline I/O, scope, cache, and root-resolution boundaries, observed through `run`, `execute`, and the real binary.
//!
//! One child module per mechanism, so each fix lands with its own case module and one
//! declaration here, while the crate links a single integration-test executable.

#[path = "pipeline_remaining/baseline_directory.rs"]
mod baseline_directory;
#[path = "pipeline_remaining/baseline_replace.rs"]
mod baseline_replace;
#[path = "pipeline_remaining/baseline_target.rs"]
mod baseline_target;
#[path = "pipeline_remaining/bounded_read_reserve.rs"]
mod bounded_read_reserve;
#[path = "pipeline_remaining/cache_location.rs"]
mod cache_location;
#[path = "pipeline_remaining/header_cpp_fallback.rs"]
mod header_cpp_fallback;
#[path = "pipeline_remaining/header_fallbacks.rs"]
mod header_fallbacks;
#[path = "pipeline_remaining/implicit_scope.rs"]
mod implicit_scope;
#[path = "pipeline_remaining/init_failure.rs"]
mod init_failure;
#[path = "pipeline_remaining/parse_failed_suppressions.rs"]
mod parse_failed_suppressions;
#[path = "pipeline_remaining/parse_failed_world_facts.rs"]
mod parse_failed_world_facts;
#[path = "pipeline_remaining/path_scope.rs"]
mod path_scope;
#[path = "pipeline_remaining/ratchet_rule_filter.rs"]
mod ratchet_rule_filter;
#[path = "pipeline_remaining/regular_inputs.rs"]
mod regular_inputs;
#[path = "pipeline_remaining/root_resolution.rs"]
mod root_resolution;
#[path = "pipeline_remaining/skipped_names.rs"]
mod skipped_names;
