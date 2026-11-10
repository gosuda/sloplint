use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use sloplint_core::config::ConfigError;

pub enum Separators {
    Literal,
    Any,
}

pub fn glob_set(
    patterns: &[String],
    key: &str,
    separators: Separators,
) -> Result<GlobSet, ConfigError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let mut glob = GlobBuilder::new(pattern);
        if matches!(separators, Separators::Literal) {
            glob.literal_separator(true);
        }
        let glob = glob.build().map_err(|error| ConfigError::Glob {
            key: key.to_string(),
            glob: pattern.clone(),
            message: error.to_string(),
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| ConfigError::Glob {
        key: key.to_string(),
        glob: patterns.join(", "),
        message: error.to_string(),
    })
}
