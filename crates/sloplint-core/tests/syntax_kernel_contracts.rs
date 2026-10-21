//! Line-index and error-boundary contracts observed through the public fact and rule API.
//!
//! One child module per mechanism, so each fix lands with its own case module and
//! one declaration here, while the crate links a single integration-test executable.

#[path = "syntax_kernel_contracts/line_index.rs"]
mod line_index;
#[path = "syntax_kernel_contracts/support.rs"]
mod support;
#[path = "syntax_kernel_contracts/typed_error_rule.rs"]
mod typed_error_rule;
