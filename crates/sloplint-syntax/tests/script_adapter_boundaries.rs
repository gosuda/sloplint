//! PHP, Python, and Ruby adapter boundaries, observed through `analyze` only.
//!
//! One child module per mechanism, so each fix lands with its own case module and
//! one declaration here, while the crate links a single integration-test executable.
#[path = "script_adapter_boundaries/php_callee_matching.rs"]
mod php_callee_matching;

#[path = "script_adapter_boundaries/php_test_class.rs"]
mod php_test_class;
