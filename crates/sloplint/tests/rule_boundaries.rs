//! Boundaries of the core rules, one module per mechanism under `rule_boundaries/`.

#[path = "rule_boundaries/colocation_rust_index.rs"]
mod colocation_rust_index;
#[path = "rule_boundaries/log_missing_message.rs"]
mod log_missing_message;
#[path = "rule_boundaries/patch_anchor_windows.rs"]
mod patch_anchor_windows;
#[path = "rule_boundaries/single_impl_applicability.rs"]
mod single_impl_applicability;
#[path = "rule_boundaries/single_impl_impl_index.rs"]
mod single_impl_impl_index;
