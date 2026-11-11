use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/nesting-depth",
    family: Family::Complexity,
    summary: "functions whose nesting depth exceeds the thresholds",
    why: "Deep nesting hides the condition that guards each line.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Lower1,
        support: Shift::Keep,
        bench: Shift::Lower1,
        example: Shift::Keep,
    },
    preset,
    build: Build::File(build),
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    minor_at: u32,
    major_at: u32,
    critical_at: u32,
    blocker_at: u32,
}

struct NestingDepth {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (Level::Off, 6, 8, 12, 16),
        Strictness::Basic => (Level::Off, 5, 7, 11, 14),
        Strictness::Standard => (Level::Off, 5, 6, 9, 12),
        Strictness::Strict => (Level::At(Severity::Info), 5, 6, 9, 12),
    };
    Preset {
        level,
        options: threshold_options(minor_at, major_at, critical_at, blocker_at),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let parsed: Options = parse_options(options)?;
    let thresholds = Thresholds::new(
        parsed.minor_at,
        parsed.major_at,
        parsed.critical_at,
        parsed.blocker_at,
    )?;
    Ok(Box::new(NestingDepth { thresholds }))
}

impl FileRule for NestingDepth {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            let Some(severity) = self.thresholds.grade(function.max_depth) else {
                continue;
            };
            let value = function.max_depth;
            let name = if function.name.is_empty() {
                format!("anonymous function at line {}", function.pos.line)
            } else {
                format!("`{}`", function.name)
            };
            let minor_at = self.thresholds.minor_at;
            let major_at = self.thresholds.major_at;
            let critical_at = self.thresholds.critical_at;
            let blocker_at = self.thresholds.blocker_at;
            let deepest_line = function.deepest_line;
            let message = format!(
                "nesting depth {value} in {name} (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})"
            );
            let fix = format!(
                "reduce nesting in {name} below {minor_at}: use guard clauses or extract the block at line {deepest_line} into a helper"
            );
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                severity,
                message,
                fix,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Function, Pos, Visibility};
    use crate::finding::line_starts;
    use crate::lang::Lang;
    use crate::role::CodeRole;

    fn severities(rule: &dyn FileRule, functions: Vec<Function<'_>>) -> Vec<Severity> {
        let source = "fn f() {}\n";
        let facts = FileFacts {
            lang: Lang::Rust,
            path: "src/lib.rs",
            source,
            line_starts: line_starts(source),
            role: CodeRole::Library,
            regions: Vec::new(),
            line_count: 1,
            package: String::new(),
            is_generated: false,
            functions,
            calls: Vec::new(),
            bindings: Vec::new(),
            imports: Vec::new(),
            types: Vec::new(),
            impls: Vec::new(),
            wildcard_arms: Vec::new(),
            handlers: Vec::new(),
            assertions: Vec::new(),
            field_copies: Vec::new(),
            dynamic_symbols: Vec::new(),
            ambient: Vec::new(),
            barrel: None,
            has_inline_tests: false,
        };
        let mut out = Vec::new();
        rule.check(&facts, &mut out);
        out.into_iter().map(|finding| finding.severity).collect()
    }

    fn nested(name: &str) -> Function<'_> {
        Function {
            name,
            pos: Pos { line: 1, byte: 0 },
            doc: Vec::new(),
            doc_pos: None,
            visibility: Visibility::Exported,
            signature_forced: false,
            entry_point: false,
            in_test: false,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            is_method: false,
            receiver: None,
            params: Vec::new(),
            return_type: None,
            untyped_throws: false,
            missing_annotations: Vec::new(),
            cognitive: 0,
            max_depth: 5,
            deepest_line: 5,
            end_line: 9,
            has_value_return: false,
            sole_call: None,
        }
    }

    #[test]
    fn off_through_standard_then_info_at_strict() {
        assert_eq!(
            preset(Strictness::Minimal).level,
            Level::Off,
            "minimal stays off"
        );
        assert_eq!(
            preset(Strictness::Basic).level,
            Level::Off,
            "basic stays off"
        );
        assert_eq!(
            preset(Strictness::Standard).level,
            Level::Off,
            "standard stays off"
        );
        assert_eq!(
            preset(Strictness::Strict).level,
            Level::At(Severity::Info),
            "strict caps at info"
        );
        assert_eq!(INFO.confidence, Confidence::Low, "confidence stays low");
        let strict_preset = preset(Strictness::Strict);
        let rule =
            build(strict_preset.options, &Settings::default()).expect("strict preset builds");
        assert_eq!(
            severities(rule.as_ref(), vec![nested("router")]),
            vec![Severity::Minor],
            "strict still fires; the ruleset ceiling caps the graded rung to info"
        );
    }

    #[test]
    fn anonymous_callback_reports_a_line_label() {
        let source = "fn f() {}\n";
        let facts = FileFacts {
            lang: Lang::Rust,
            path: "src/lib.rs",
            source,
            line_starts: line_starts(source),
            role: CodeRole::Library,
            regions: Vec::new(),
            line_count: 1,
            package: String::new(),
            is_generated: false,
            functions: vec![nested("")],
            calls: Vec::new(),
            bindings: Vec::new(),
            imports: Vec::new(),
            types: Vec::new(),
            impls: Vec::new(),
            wildcard_arms: Vec::new(),
            handlers: Vec::new(),
            assertions: Vec::new(),
            field_copies: Vec::new(),
            dynamic_symbols: Vec::new(),
            ambient: Vec::new(),
            barrel: None,
            has_inline_tests: false,
        };
        let strict = preset(Strictness::Strict);
        let rule = build(strict.options, &Settings::default()).expect("strict preset builds");
        let mut out = Vec::new();
        rule.check(&facts, &mut out);
        assert_eq!(
            out.len(),
            1,
            "an unnamed deep function still reports: {out:?}"
        );
        assert!(
            out[0].message.contains("anonymous function at line 1"),
            "empty names print a line label, not empty backticks: {}",
            out[0].message
        );
    }
}
