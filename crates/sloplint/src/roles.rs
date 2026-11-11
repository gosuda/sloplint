use globset::GlobSet;
use sloplint_core::config::{Config, ConfigError};
use sloplint_core::role::CodeRole;

use crate::globs::{Separators, glob_set};

pub struct RoleSets {
    bench: GlobSet,
    example: GlobSet,
    support: GlobSet,
    entry: GlobSet,
    test: GlobSet,
    data: GlobSet,
}

impl RoleSets {
    pub fn from_config(config: &Config) -> Result<Self, ConfigError> {
        Ok(Self {
            bench: glob_set(
                &config.project.bench_globs,
                "project.bench_globs",
                Separators::Literal,
            )?,
            example: glob_set(
                &config.project.example_globs,
                "project.example_globs",
                Separators::Literal,
            )?,
            support: glob_set(
                &config.project.support_globs,
                "project.support_globs",
                Separators::Literal,
            )?,
            entry: glob_set(
                &config.project.entry_globs,
                "project.entry_globs",
                Separators::Literal,
            )?,
            test: glob_set(
                &config.project.test_globs,
                "project.test_globs",
                Separators::Literal,
            )?,
            data: glob_set(
                &config.project.data_globs,
                "project.data_globs",
                Separators::Literal,
            )?,
        })
    }

    pub fn role(&self, rel: &str) -> CodeRole {
        if self.data.is_match(rel) {
            return CodeRole::Library;
        }
        if self.bench.is_match(rel) {
            return CodeRole::Bench;
        }
        if self.example.is_match(rel) {
            return CodeRole::Example;
        }
        if self.support.is_match(rel) {
            return CodeRole::Support;
        }
        if self.test.is_match(rel) {
            return CodeRole::Test;
        }
        if self.entry.is_match(rel) {
            return CodeRole::Entry;
        }
        CodeRole::Library
    }
}

#[cfg(test)]
mod tests {
    use super::RoleSets;
    use sloplint_core::config::Config;
    use sloplint_core::role::CodeRole;

    #[test]
    fn single_star_stays_inside_one_path_component() {
        let roles =
            RoleSets::from_config(&Config::default()).unwrap_or_else(|e| panic!("defaults: {e}"));
        assert_eq!(
            roles.role("src/bench_runner/cli.py"),
            CodeRole::Library,
            "bench_*.py must not span into bench_runner/"
        );
        assert_eq!(
            roles.role("src/test_runner/cli.py"),
            CodeRole::Library,
            "test_*.py must not span into test_runner/"
        );
        assert_eq!(roles.role("tests/test_x.py"), CodeRole::Test);
        assert_eq!(roles.role("benches/bench_x.py"), CodeRole::Bench);
    }
}
