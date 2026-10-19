//! Pipeline I/O, scope, cache, and root-resolution boundaries, observed through `run`, `execute`, and the real binary.
//!
//! One child module per mechanism, so each fix lands with its own case module and one
//! declaration here, while the crate links a single integration-test executable.

#[path = "pipeline_remaining/path_scope.rs"]
mod path_scope;
