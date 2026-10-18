//! The meta rules emitted outside the regular rule run.

use crate::config::Strictness;
use crate::facts::Pos;
use crate::finding::{Finding, Level, Severity};
use crate::rules::{Build, Family, Preset, RuleInfo};

/// `meta/parse-error`: emitted by `RuleSet::parse_error` for files that fail to parse.
pub(crate) const PARSE_ERROR: RuleInfo = RuleInfo {
    id: "meta/parse-error",
    family: Family::Meta,
    summary: "syntax errors that stop analysis of a file",
    why: "Code with syntax errors cannot be analyzed reliably.",
    exempt_tests: false,
    preset: parse_error_preset,
    build: Build::Meta,
};

/// `meta/invalid-suppression`: emitted for malformed `sloplint-allow` directives.
pub(crate) const INVALID_SUPPRESSION: RuleInfo = RuleInfo {
    id: "meta/invalid-suppression",
    family: Family::Meta,
    summary: "suppression directives that name unknown rules or lack a reason",
    why: "A suppression must name known rules and give a reason.",
    exempt_tests: false,
    preset: invalid_suppression_preset,
    build: Build::Meta,
};

/// `meta/unused-suppression`: emitted in the merge stage for suppressions that match nothing.
pub(crate) const UNUSED_SUPPRESSION: RuleInfo = RuleInfo {
    id: "meta/unused-suppression",
    family: Family::Meta,
    summary: "suppression directives that match no finding",
    why: "A suppression that suppresses nothing is stale noise.",
    exempt_tests: false,
    preset: unused_suppression_preset,
    build: Build::Meta,
};

fn parse_error_preset(_strictness: Strictness) -> Preset {
    Preset {
        level: Level::Warn,
        options: toml::Table::new(),
    }
}

fn invalid_suppression_preset(_strictness: Strictness) -> Preset {
    Preset {
        level: Level::Deny,
        options: toml::Table::new(),
    }
}

fn unused_suppression_preset(_strictness: Strictness) -> Preset {
    Preset {
        level: Level::Warn,
        options: toml::Table::new(),
    }
}

/// Builds the `meta/parse-error` finding at a syntax-error position.
pub(crate) fn parse_error_finding(path: &str, source: &str, pos: Pos) -> Finding {
    Finding::at_source(
        path,
        source,
        pos,
        PARSE_ERROR.id,
        Severity::Warn,
        "syntax error; other rules skipped for this file".to_string(),
        "fix the syntax error at this position".to_string(),
    )
}

/// Builds the `meta/invalid-suppression` finding at a directive's comment start line.
pub(crate) fn invalid_suppression_finding(
    path: &str,
    source: &str,
    line: u32,
    problem: &str,
) -> Finding {
    Finding::at_source(
        path,
        source,
        Pos { line, byte: 0 },
        INVALID_SUPPRESSION.id,
        Severity::Deny,
        format!("invalid suppression: {problem}"),
        "use sloplint-allow(<rule-id>): <reason>".to_string(),
    )
}

/// Builds the `meta/unused-suppression` finding at a suppression's comment start line.
pub fn unused_suppression_finding(path: &str, source: &str, line: u32, rules: &str) -> Finding {
    Finding::at_source(
        path,
        source,
        Pos { line, byte: 0 },
        UNUSED_SUPPRESSION.id,
        Severity::Warn,
        format!("suppression for {rules} matches no finding"),
        "delete this suppression".to_string(),
    )
}
