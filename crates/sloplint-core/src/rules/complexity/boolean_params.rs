use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::{FileFacts, Function, Param, PrimitiveKind};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/boolean-params",
    family: Family::Complexity,
    summary: "public functions taking boolean parameters",
    why: "Boolean arguments are unreadable at the call site and double the paths per flag.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Lower1,
        test: Shift::Off,
        support: Shift::Lower1,
        bench: Shift::Off,
        example: Shift::Off,
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

struct BooleanParams {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (Level::Off, 2, 3, 5, 6),
        Strictness::Basic => (Level::Off, 2, 3, 5, 6),
        Strictness::Standard => (Level::Off, 2, 3, 5, 6),
        Strictness::Strict => (Level::At(Severity::Info), 2, 3, 5, 6),
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
    Ok(Box::new(BooleanParams { thresholds }))
}

fn counted(param: &Param<'_>) -> bool {
    if param.is_variadic || param.keyword_only {
        return false;
    }
    let untyped_flag = param.ty.is_none() && param.bool_default;
    param.primitive_kind == PrimitiveKind::Bool || untyped_flag
}

fn flagged_params<'s>(function: &Function<'s>) -> Vec<&'s str> {
    function
        .params
        .iter()
        .filter(|param| counted(param))
        .map(|param| param.name)
        .collect()
}

impl FileRule for BooleanParams {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            if !function.actionable_api() || function.is_overload {
                continue;
            }
            let flagged = flagged_params(function);
            if flagged.is_empty() {
                continue;
            }
            let count = u32::try_from(flagged.len()).unwrap_or(u32::MAX);
            let Some(severity) = self.thresholds.grade(count) else {
                continue;
            };
            let names = flagged.join(", ");
            let name = function.name;
            let minor_at = self.thresholds.minor_at;
            let major_at = self.thresholds.major_at;
            let critical_at = self.thresholds.critical_at;
            let blocker_at = self.thresholds.blocker_at;
            let message = format!(
                "`{name}` takes {count} boolean parameter(s): {names} (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})"
            );
            let fix = format!(
                "replace {names} with a two-variant enum or with separately named functions"
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
    use crate::facts::{Function, Param, Pos, PrimitiveKind, Visibility};
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

    fn flag(name: &str) -> Param<'_> {
        Param {
            name,
            ty: None,
            primitive_kind: PrimitiveKind::Bool,
            has_default: false,
            bool_default: false,
            keyword_only: false,
            is_variadic: false,
        }
    }

    fn with_flags(name: &str) -> Function<'_> {
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
            params: vec![flag("verbose"), flag("force")],
            return_type: None,
            untyped_throws: false,
            missing_annotations: Vec::new(),
            cognitive: 0,
            max_depth: 0,
            deepest_line: 1,
            end_line: 3,
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
            severities(rule.as_ref(), vec![with_flags("serve")]),
            vec![Severity::Minor],
            "strict still fires; the ruleset ceiling caps the graded rung to info"
        );
    }
}
