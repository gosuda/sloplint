//! Rust, TypeScript, and JavaScript adapter boundaries, observed through `analyze` only.
//!
//! One child module per mechanism, so each fix lands with its own case module and
//! one declaration here, while the crate links a single integration-test executable.

#[path = "rust_web_boundaries/rust_grouped_use_modules.rs"]
mod rust_grouped_use_modules;
#[path = "rust_web_boundaries/rust_self_barrel.rs"]
mod rust_self_barrel;
