//! The complexity family: findings about working-memory load.

pub(crate) mod boolean_params;
pub(crate) mod cognitive_depth;
pub(crate) mod import_fan_out;
pub(crate) mod mutable_span;
pub(crate) mod nested_mutation;
pub(crate) mod nesting_depth;

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
