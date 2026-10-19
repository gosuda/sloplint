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
#[path = "native_adapter_boundaries/cpp_virtual_interface_bound.rs"]
mod cpp_virtual_interface_bound;
#[path = "native_adapter_boundaries/dart_namespace_import_names.rs"]
mod dart_namespace_import_names;
#[path = "native_adapter_boundaries/java_message_first_assertions.rs"]
mod java_message_first_assertions;
#[path = "native_adapter_boundaries/objc_message_arguments.rs"]
mod objc_message_arguments;
#[path = "native_adapter_boundaries/scala_brace_call_arguments.rs"]
mod scala_brace_call_arguments;
#[path = "native_adapter_boundaries/support.rs"]
mod support;
