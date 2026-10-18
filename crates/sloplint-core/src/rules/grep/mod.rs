//! The greppability rules: text-search recall and precision.

pub(crate) mod contiguous_log_literal;
pub(crate) mod generic_name;
pub(crate) mod homonym_definitions;
pub(crate) mod multiline_argument;
pub(crate) mod no_dynamic_symbol;
pub(crate) mod no_rename_import;
pub(crate) mod no_wildcard_import;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Option table shared by rules that accept no options.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmptyOptions {}

/// Deserializes one merged preset option table into a rule's `Options`.
pub(crate) fn parse_options<T: DeserializeOwned>(table: toml::Table) -> Result<T, String> {
    let value = toml::Value::Table(table);
    value.try_into().map_err(|error| error.to_string())
}

/// Builds a TOML string array for a fixed preset option.
pub(crate) fn string_values(items: &[&str]) -> toml::Value {
    toml::Value::Array(
        items
            .iter()
            .map(|item| toml::Value::String((*item).to_string()))
            .collect(),
    )
}
