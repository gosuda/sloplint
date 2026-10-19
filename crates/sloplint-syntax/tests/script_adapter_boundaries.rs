//! PHP, Python, and Ruby adapter boundaries, observed through `analyze` only.
//!
//! One child module per mechanism, so each fix lands with its own case module and
//! one declaration here, while the crate links a single integration-test executable.
#[path = "script_adapter_boundaries/php_callee_matching.rs"]
mod php_callee_matching;

#[path = "script_adapter_boundaries/php_field_copy.rs"]
mod php_field_copy;
#[path = "script_adapter_boundaries/php_test_class.rs"]
mod php_test_class;
#[path = "script_adapter_boundaries/python_callee_root.rs"]
mod python_callee_root;
#[path = "script_adapter_boundaries/python_nested_def.rs"]
mod python_nested_def;
#[path = "script_adapter_boundaries/ruby_hash_keys.rs"]
mod ruby_hash_keys;
#[path = "script_adapter_boundaries/ruby_keyword_booleans.rs"]
mod ruby_keyword_booleans;
