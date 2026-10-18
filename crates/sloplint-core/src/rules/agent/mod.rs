//! The agent family: code an autonomous agent can discover, load, patch, and verify.

pub(crate) mod ambient_global_state;
pub(crate) mod catch_all_match;
pub(crate) mod colocated_tests;
pub(crate) mod explicit_boundary_types;
pub(crate) mod file_token_budget;
pub(crate) mod informative_assertions;
pub(crate) mod stringly_api;
pub(crate) mod typed_error_boundary;
pub(crate) mod unique_patch_anchors;

use serde::{Deserialize, Serialize};

/// Option table shared by rules that accept no options.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmptyOptions {}

/// Builds a TOML string array for a fixed preset option.
pub(crate) fn string_values(items: &[&str]) -> toml::Value {
    toml::Value::Array(
        items
            .iter()
            .map(|item| toml::Value::String((*item).to_string()))
            .collect(),
    )
}

/// Builds the `{ warn_at, deny_at }` option table every metric preset carries.
pub(crate) fn threshold_options(warn_at: u32, deny_at: u32) -> toml::Table {
    let mut options = toml::Table::new();
    options.insert(
        "warn_at".to_string(),
        toml::Value::Integer(i64::from(warn_at)),
    );
    options.insert(
        "deny_at".to_string(),
        toml::Value::Integer(i64::from(deny_at)),
    );
    options
}
