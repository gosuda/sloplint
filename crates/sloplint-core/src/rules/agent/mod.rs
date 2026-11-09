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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmptyOptions {}

pub(crate) fn string_values(items: &[&str]) -> toml::Value {
    toml::Value::Array(
        items
            .iter()
            .map(|item| toml::Value::String((*item).to_string()))
            .collect(),
    )
}
