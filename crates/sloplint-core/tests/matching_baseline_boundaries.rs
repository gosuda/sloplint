//! Public-API boundary tests for callee-pattern parsing and matching and for baseline fingerprints and filtering.
//!
//! One child module per mechanism, so each fix lands with its own case module and one
//! declaration here, while the crate links a single integration-test executable.

#[path = "matching_baseline_boundaries/matching_semantics.rs"]
mod matching_semantics;
#[path = "matching_baseline_boundaries/pattern_validation.rs"]
mod pattern_validation;
