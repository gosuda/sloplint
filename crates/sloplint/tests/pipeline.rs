//! Pipeline I/O, scope, cache, and root-resolution boundaries, observed through `run`, `execute`, and the real binary.

#[path = "pipeline/baseline_directory.rs"]
mod baseline_directory;
#[path = "pipeline/baseline_replace.rs"]
mod baseline_replace;
#[path = "pipeline/baseline_target.rs"]
mod baseline_target;
#[path = "pipeline/bounded_read_reserve.rs"]
mod bounded_read_reserve;
#[path = "pipeline/cache_location.rs"]
mod cache_location;
#[path = "pipeline/custom_rule_cap.rs"]
mod custom_rule_cap;
#[path = "pipeline/gitignore_boundary.rs"]
mod gitignore_boundary;
#[path = "pipeline/header_fallbacks.rs"]
mod header_fallbacks;
#[path = "pipeline/implicit_scope.rs"]
mod implicit_scope;
#[path = "pipeline/init_failure.rs"]
mod init_failure;
#[path = "pipeline/nofollow_sources.rs"]
mod nofollow_sources;
#[path = "pipeline/parse_failed_suppressions.rs"]
mod parse_failed_suppressions;
#[path = "pipeline/parse_failed_world_facts.rs"]
mod parse_failed_world_facts;
#[path = "pipeline/path_scope.rs"]
mod path_scope;
#[path = "pipeline/ratchet_rule_filter.rs"]
mod ratchet_rule_filter;
#[path = "pipeline/regular_inputs.rs"]
mod regular_inputs;
#[path = "pipeline/root_resolution.rs"]
mod root_resolution;
#[path = "pipeline/skipped_names.rs"]
mod skipped_names;
