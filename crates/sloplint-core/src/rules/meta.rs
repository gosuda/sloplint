use crate::config::Strictness;
use crate::facts::Pos;
use crate::finding::{Confidence, Finding, Level, Severity, SourceText};
use crate::role::RolePolicy;
use crate::rules::{Build, Family, Preset, RuleInfo};

pub(crate) const PARSE_ERROR: RuleInfo = RuleInfo {
    id: "meta/parse-error",
    family: Family::Meta,
    summary: "syntax errors that stop analysis of a file",
    why: "Code with syntax errors cannot be analyzed reliably.",
    confidence: Confidence::High,
    roles: RolePolicy::KEEP,
    preset: parse_error_preset,
    build: Build::Meta,
};

pub(crate) const CUSTOM_CAP: RuleInfo = RuleInfo {
    id: "meta/custom-cap",
    family: Family::Meta,
    summary: "custom rule queries that hit the per-file work cap",
    why: "A custom query that matches unbounded node combinations was stopped early, so its findings for the file are partial.",
    confidence: Confidence::High,
    roles: RolePolicy::KEEP,
    preset: custom_cap_preset,
    build: Build::Meta,
};

pub(crate) const INVALID_SUPPRESSION: RuleInfo = RuleInfo {
    id: "meta/invalid-suppression",
    family: Family::Meta,
    summary: "suppression directives that name unknown rules or lack a reason",
    why: "A suppression must name known rules and give a reason.",
    confidence: Confidence::High,
    roles: RolePolicy::KEEP,
    preset: invalid_suppression_preset,
    build: Build::Meta,
};

pub(crate) const UNUSED_SUPPRESSION: RuleInfo = RuleInfo {
    id: "meta/unused-suppression",
    family: Family::Meta,
    summary: "suppression directives that match no finding",
    why: "A suppression that suppresses nothing is stale noise.",
    confidence: Confidence::High,
    roles: RolePolicy::KEEP,
    preset: unused_suppression_preset,
    build: Build::Meta,
};

fn parse_error_preset(_strictness: Strictness) -> Preset {
    Preset {
        level: Level::At(Severity::Minor),
        options: toml::Table::new(),
    }
}

fn custom_cap_preset(_strictness: Strictness) -> Preset {
    Preset {
        level: Level::At(Severity::Major),
        options: toml::Table::new(),
    }
}

fn invalid_suppression_preset(_strictness: Strictness) -> Preset {
    Preset {
        level: Level::At(Severity::Major),
        options: toml::Table::new(),
    }
}

fn unused_suppression_preset(_strictness: Strictness) -> Preset {
    Preset {
        level: Level::At(Severity::Minor),
        options: toml::Table::new(),
    }
}

pub(crate) fn parse_error_finding(text: &SourceText, pos: Pos) -> Finding {
    Finding::at_source(
        text,
        pos,
        PARSE_ERROR.id,
        Severity::Minor,
        "syntax error; other rules skipped for this file".to_string(),
        "fix the syntax error at this position".to_string(),
    )
}

pub(crate) fn invalid_suppression_finding(text: &SourceText, line: u32, problem: &str) -> Finding {
    Finding::at_source(
        text,
        Pos { line, byte: 0 },
        INVALID_SUPPRESSION.id,
        Severity::Major,
        format!("invalid suppression: {problem}"),
        "use sloplint-allow(<rule-id>): <reason>".to_string(),
    )
}

pub fn unused_suppression_finding(text: &SourceText, line: u32, rules: &str) -> Finding {
    Finding::at_source(
        text,
        Pos { line, byte: 0 },
        UNUSED_SUPPRESSION.id,
        Severity::Minor,
        format!("suppression for {rules} matches no finding"),
        "delete this suppression".to_string(),
    )
}
