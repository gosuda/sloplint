use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/mutable-span",
    family: Family::Complexity,
    summary: "bindings that stay mutable across too many lines",
    why: "A long-lived mutable binding forces the reader to replay every write.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Lower1,
        support: Shift::Keep,
        bench: Shift::Off,
        example: Shift::Lower1,
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

struct MutableSpan {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (Level::Off, 30, 60, 90, 120),
        Strictness::Basic => (Level::Off, 30, 60, 90, 120),
        Strictness::Standard => (Level::Off, 20, 40, 60, 80),
        Strictness::Strict => (Level::At(Severity::Info), 15, 30, 45, 60),
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
    Ok(Box::new(MutableSpan { thresholds }))
}

impl FileRule for MutableSpan {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for binding in &facts.bindings {
            if binding.write_lines < 2 {
                continue;
            }
            let span = binding
                .last_write_line
                .saturating_sub(binding.first_write_line);
            let Some(severity) = self.thresholds.grade(span) else {
                continue;
            };
            let name = binding.name;
            let first = binding.first_write_line;
            let last = binding.last_write_line;
            let minor_at = self.thresholds.minor_at;
            let major_at = self.thresholds.major_at;
            let critical_at = self.thresholds.critical_at;
            let blocker_at = self.thresholds.blocker_at;
            let message = format!(
                "`{name}` stays mutable for {span} lines (writes at lines {first}-{last}) (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})"
            );
            let fix = format!(
                "derive `{name}` in one expression, or move lines {first}-{last} into a helper that returns its final value"
            );
            out.push(Finding::at(
                facts,
                binding.pos,
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
    use crate::facts::{MutBinding, Pos};
    use crate::finding::line_starts;
    use crate::lang::Lang;
    use crate::role::CodeRole;

    fn severities(rule: &dyn FileRule, bindings: Vec<MutBinding<'_>>) -> Vec<Severity> {
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
            functions: Vec::new(),
            calls: Vec::new(),
            bindings,
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

    fn spanning(name: &str) -> MutBinding<'_> {
        MutBinding {
            name,
            pos: Pos { line: 1, byte: 0 },
            first_write_line: 1,
            last_write_line: 20,
            write_lines: 2,
            loop_depth: 0,
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
            severities(rule.as_ref(), vec![spanning("acc")]),
            vec![Severity::Minor],
            "strict still fires; the ruleset ceiling caps the graded rung to info"
        );
    }
}
