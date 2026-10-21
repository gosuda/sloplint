//! OCaml adapter boundaries, observed through `analyze` only.
//!
//! One child module per mechanism, so each fix lands with its own case module and
//! one declaration here, while the crate links a single integration-test executable.

#[path = "ocaml_adapter_boundaries/ocaml_interface_visibility.rs"]
mod ocaml_interface_visibility;
#[path = "ocaml_adapter_boundaries/support.rs"]
mod support;
