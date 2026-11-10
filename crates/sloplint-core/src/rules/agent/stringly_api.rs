use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::{FileFacts, Param, PrimitiveKind};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

const FIX: &str = "wrap these parameters in distinct newtypes or one named struct";

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/stringly-api",
    family: Family::Agent,
    summary: "runs of consecutive primitive parameters of swappable types",
    why: "Adjacent primitive parameters of one type can be swapped without a compile error.",
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

struct StringlyApi {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (Level::Off, 3, 4, 6, 8),
        Strictness::Basic => (Level::Off, 3, 4, 6, 8),
        Strictness::Standard => (Level::Off, 3, 4, 6, 8),
        Strictness::Strict => (Level::At(Severity::Info), 3, 4, 6, 8),
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
    Ok(Box::new(StringlyApi { thresholds }))
}

impl FileRule for StringlyApi {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            if !function.actionable_api() || function.is_overload {
                continue;
            }
            let Some((start, len)) = longest_run(&function.params) else {
                continue;
            };
            let Some(severity) = self
                .thresholds
                .grade(u32::try_from(len).unwrap_or(u32::MAX))
            else {
                continue;
            };
            let names = function.params[start..start + len]
                .iter()
                .map(|param| param.name)
                .collect::<Vec<_>>()
                .join(", ");
            let name = function.name;
            let minor_at = self.thresholds.minor_at;
            let major_at = self.thresholds.major_at;
            let critical_at = self.thresholds.critical_at;
            let blocker_at = self.thresholds.blocker_at;
            let message = format!(
                "`{name}` takes {len} consecutive primitive parameters ({names}) that callers can swap (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})"
            );
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                severity,
                message,
                FIX.to_string(),
            ));
        }
    }
}

fn longest_run(params: &[Param<'_>]) -> Option<(usize, usize)> {
    let mut best = (0usize, 0usize);
    let mut start = 0usize;
    let mut kind = PrimitiveKind::Other;
    let mut open = false;
    for (index, param) in params.iter().enumerate() {
        if counts(param) && open && param.primitive_kind == kind {
            continue;
        }
        if open {
            best = longer(best, (start, index - start));
        }
        if counts(param) {
            start = index;
            kind = param.primitive_kind;
            open = true;
        } else {
            open = false;
        }
    }
    if open {
        best = longer(best, (start, params.len() - start));
    }
    (best.1 > 0).then_some(best)
}

fn longer(current: (usize, usize), candidate: (usize, usize)) -> (usize, usize) {
    if candidate.1 > current.1 {
        candidate
    } else {
        current
    }
}

fn counts(param: &Param<'_>) -> bool {
    !param.is_variadic
        && !param.keyword_only
        && !param.has_default
        && param.primitive_kind != PrimitiveKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Function, Pos, Visibility};
    use crate::finding::line_starts;
    use crate::lang::Lang;

    fn param(
        name: &str,
        primitive_kind: PrimitiveKind,
        has_default: bool,
        keyword_only: bool,
        is_variadic: bool,
    ) -> Param<'_> {
        Param {
            name,
            ty: None,
            primitive_kind,
            has_default,
            bool_default: has_default,
            keyword_only,
            is_variadic,
        }
    }

    fn plain(name: &str, primitive_kind: PrimitiveKind) -> Param<'_> {
        param(name, primitive_kind, false, false, false)
    }

    fn function<'s>(name: &'s str, params: Vec<Param<'s>>) -> Function<'s> {
        Function {
            name,
            pos: Pos { line: 9, byte: 0 },
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
            params,
            return_type: None,
            untyped_throws: false,
            has_value_return: false,
            missing_annotations: Vec::new(),
            cognitive: 0,
            max_depth: 0,
            deepest_line: 12,
            end_line: 12,
            sole_call: None,
        }
    }

    fn strict() -> Box<dyn FileRule> {
        let preset = preset(Strictness::Strict);
        build(preset.options, &Settings::default()).expect("strict preset builds")
    }

    fn names(rule: &dyn FileRule, functions: Vec<Function<'_>>) -> Vec<String> {
        let source = "fn f() {}\n";
        let facts = FileFacts {
            lang: Lang::Rust,
            path: "src/lib.rs",
            source,
            line_starts: line_starts(source),
            role: crate::role::CodeRole::Library,
            regions: Vec::new(),
            line_count: 0,
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
        out.into_iter().map(|finding| finding.message).collect()
    }

    fn triple() -> Vec<Param<'static>> {
        vec![
            plain("a", PrimitiveKind::Int),
            plain("b", PrimitiveKind::Int),
            plain("c", PrimitiveKind::Int),
        ]
    }

    #[test]
    fn same_kind_triple_flags_while_mixed_and_pairs_stay_silent() {
        let rule = strict();
        let found = names(
            rule.as_ref(),
            vec![
                function(
                    "mixed",
                    vec![
                        plain("a", PrimitiveKind::Str),
                        plain("b", PrimitiveKind::Str),
                        plain("c", PrimitiveKind::Int),
                    ],
                ),
                function(
                    "pair",
                    vec![
                        plain("message", PrimitiveKind::Str),
                        plain("fix", PrimitiveKind::Str),
                    ],
                ),
                function("triple", triple()),
            ],
        );
        assert_eq!(found.len(), 1);
        assert!(found[0].starts_with("`triple` takes 3 consecutive"));
    }

    #[test]
    fn defaulted_keyword_and_overload_params_break_the_run() {
        let rule = strict();
        let mut defaulted: Vec<Param<'_>> = vec![plain("prog", PrimitiveKind::Str)];
        defaulted.push(param("var", PrimitiveKind::Str, true, false, false));
        defaulted.push(param("instruction", PrimitiveKind::Str, true, false, false));
        let mut keyword: Vec<Param<'_>> = vec![plain("a", PrimitiveKind::Str)];
        keyword.push(param("b", PrimitiveKind::Str, false, true, false));
        keyword.push(param("c", PrimitiveKind::Str, false, true, false));
        let mut overloaded = function("overloaded", triple());
        overloaded.is_overload = true;
        let found = names(
            rule.as_ref(),
            vec![
                function("defaulted", defaulted),
                function("keyword", keyword),
                overloaded,
            ],
        );
        assert!(found.is_empty());
    }
    #[test]
    fn preset_pins_off_at_standard_and_info_at_strict() {
        assert_eq!(
            INFO.confidence,
            Confidence::Low,
            "stringly-api ships at low confidence"
        );
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
            "strict reports at info"
        );
        let rule = strict();
        let found = names(rule.as_ref(), vec![function("triple", triple())]);
        assert_eq!(
            found.len(),
            1,
            "strict flags the triple on a positive input"
        );
    }
}
