use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/cognitive-depth",
    family: Family::Complexity,
    summary: "functions whose cognitive complexity exceeds the thresholds",
    why: "Each nested branch multiplies the paths a reader must hold in working memory.",
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

struct CognitiveDepth {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (Level::Off, 20, 30, 45, 60),
        Strictness::Basic => (Level::Off, 15, 25, 38, 50),
        Strictness::Standard => (Level::Off, 12, 20, 30, 40),
        Strictness::Strict => (Level::At(Severity::Info), 10, 15, 23, 30),
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
    Ok(Box::new(CognitiveDepth { thresholds }))
}

impl FileRule for CognitiveDepth {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            if function.max_depth < 2 {
                continue;
            }
            let Some(severity) = self.thresholds.grade(function.cognitive) else {
                continue;
            };
            let value = function.cognitive;
            let name = function.name;
            let minor_at = self.thresholds.minor_at;
            let major_at = self.thresholds.major_at;
            let critical_at = self.thresholds.critical_at;
            let blocker_at = self.thresholds.blocker_at;
            let message = format!(
                "cognitive complexity {value} in `{name}` (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})"
            );
            let fix = format!(
                "flatten `{name}`: turn nested conditions into early returns, then move the deepest block into a named helper"
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

    fn deep(name: &str) -> Function<'_> {
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
            params: Vec::new(),
            return_type: None,
            untyped_throws: false,
            missing_annotations: Vec::new(),
            cognitive: 12,
            max_depth: 3,
            deepest_line: 3,
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
            severities(rule.as_ref(), vec![deep("router")]),
            vec![Severity::Minor],
            "strict still fires; the ruleset ceiling caps the graded rung to info"
        );
    }
}
