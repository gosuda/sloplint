//! C, C++, Objective-C, Dart, Java, and Scala adapter boundaries, observed through `analyze` only.
//!
//! One child module per mechanism, so each fix lands with its own case module and
//! one declaration here, while the crate links a single integration-test executable.

#[path = "native_adapter_boundaries/c_family_const_pointer_bindings.rs"]
mod c_family_const_pointer_bindings;
#[path = "native_adapter_boundaries/c_family_param_declarators.rs"]
mod c_family_param_declarators;
#[path = "native_adapter_boundaries/cpp_operator_functions.rs"]
mod cpp_operator_functions;
#[path = "native_adapter_boundaries/support.rs"]
mod support;
