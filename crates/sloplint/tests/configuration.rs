//! Configuration loading and custom-rule coverage, one module per mechanism.

#[path = "configuration/ceilings.rs"]
mod ceilings;
#[path = "configuration/dispatch.rs"]
mod dispatch;
#[path = "configuration/findings.rs"]
mod findings;
#[path = "configuration/language_tables.rs"]
mod language_tables;
#[path = "configuration/load_errors.rs"]
mod load_errors;
#[path = "configuration/max_file_bytes.rs"]
mod max_file_bytes;
#[path = "configuration/placeholders.rs"]
mod placeholders;
#[path = "configuration/test_exemption.rs"]
mod test_exemption;
#[path = "configuration/toml_location.rs"]
mod toml_location;

fn numbers(text: &str) -> Vec<u64> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}
