//! Configuration loading and custom-rule coverage, one module per mechanism.

#[path = "config_custom_remaining/language_tables.rs"]
mod language_tables;
#[path = "config_custom_remaining/max_file_bytes.rs"]
mod max_file_bytes;
#[path = "config_custom_remaining/placeholders.rs"]
mod placeholders;
#[path = "config_custom_remaining/test_exemption.rs"]
mod test_exemption;
#[path = "config_custom_remaining/toml_location.rs"]
mod toml_location;
