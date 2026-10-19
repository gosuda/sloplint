//! Rust, TypeScript, and JavaScript adapter boundaries, observed through `analyze` only.
//!
//! One child module per mechanism, so each fix lands with its own case module and
//! one declaration here, while the crate links a single integration-test executable.

#[path = "rust_web_boundaries/rust_enum_like_match.rs"]
mod rust_enum_like_match;
#[path = "rust_web_boundaries/rust_grouped_use_modules.rs"]
mod rust_grouped_use_modules;
#[path = "rust_web_boundaries/rust_self_barrel.rs"]
mod rust_self_barrel;
#[path = "rust_web_boundaries/rust_thread_local_name.rs"]
mod rust_thread_local_name;
#[path = "rust_web_boundaries/typescript_callee_root.rs"]
mod typescript_callee_root;
#[path = "rust_web_boundaries/typescript_import_require.rs"]
mod typescript_import_require;
#[path = "rust_web_boundaries/typescript_object_literal_methods.rs"]
mod typescript_object_literal_methods;
#[path = "rust_web_boundaries/typescript_tagged_template_args.rs"]
mod typescript_tagged_template_args;
