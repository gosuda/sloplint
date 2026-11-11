use crate::config::Strictness;
use crate::facts::{FileFacts, ScrutineeKind};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::lang::Lang;
use crate::role::{RolePolicy, Shift};
use crate::rules::EmptyOptions;
use crate::rules::parse_options;
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

const MESSAGE: &str = "wildcard arm in a match over enum-like patterns";
const FIX: &str =
    "list the remaining variants explicitly so a new variant fails the exhaustiveness check here";

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/catch-all-match",
    family: Family::Agent,
    summary: "wildcard match arms over enum-like patterns",
    why: "A wildcard arm hides new variants from exhaustiveness checking.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Lower1,
        support: Shift::Keep,
        bench: Shift::Lower1,
        example: Shift::Lower1,
    },
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal | Strictness::Basic | Strictness::Standard => Level::Off,
        Strictness::Strict => Level::At(Severity::Info),
    };
    Preset {
        level,
        options: toml::Table::new(),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let _: EmptyOptions = parse_options(options)?;
    Ok(Box::new(CatchAllMatch))
}

struct CatchAllMatch;

impl FileRule for CatchAllMatch {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        if facts.lang == Lang::Go {
            return;
        }
        for arm in &facts.wildcard_arms {
            if !arm.enum_like || arm.has_nullable_ctor || arm.variant_count == 1 {
                continue;
            }
            if !matches!(
                arm.scrutinee,
                ScrutineeKind::Ident | ScrutineeKind::Field | ScrutineeKind::Path
            ) {
                continue;
            }
            out.push(Finding::at(
                facts,
                arm.pos,
                INFO.id,
                Severity::Minor,
                MESSAGE.to_string(),
                FIX.to_string(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Pos, WildcardArm};
    use crate::finding::line_starts;

    fn facts<'s>(lang: Lang, source: &'s str, arms: Vec<WildcardArm>) -> FileFacts<'s> {
        FileFacts {
            lang,
            path: "src/lib.rs",
            source,
            line_starts: line_starts(source),
            role: crate::role::CodeRole::Library,
            regions: Vec::new(),
            line_count: 0,
            package: String::new(),
            is_generated: false,
            functions: Vec::new(),
            calls: Vec::new(),
            bindings: Vec::new(),
            imports: Vec::new(),
            types: Vec::new(),
            impls: Vec::new(),
            wildcard_arms: arms,
            handlers: Vec::new(),
            assertions: Vec::new(),
            field_copies: Vec::new(),
            dynamic_symbols: Vec::new(),
            ambient: Vec::new(),
            barrel: None,
            has_inline_tests: false,
        }
    }

    fn arm(scrutinee: ScrutineeKind, variant_count: u32, has_nullable_ctor: bool) -> WildcardArm {
        WildcardArm {
            pos: Pos { line: 4, byte: 0 },
            enum_like: true,
            scrutinee,
            variant_count,
            has_nullable_ctor,
        }
    }

    fn lines(rule: &CatchAllMatch, lang: Lang, arms: Vec<WildcardArm>) -> Vec<u32> {
        let source = "fn f() {}\n";
        let mut out = Vec::new();
        rule.check(&facts(lang, source, arms), &mut out);
        out.into_iter().map(|finding| finding.line).collect()
    }

    #[test]
    fn option_and_single_variant_extracts_stay_silent_while_status_flags() {
        let rule = CatchAllMatch;
        let found = lines(
            &rule,
            Lang::Rust,
            vec![
                arm(ScrutineeKind::Ident, 0, true),
                arm(ScrutineeKind::Path, 1, false),
                arm(ScrutineeKind::Path, 2, false),
            ],
        );
        assert_eq!(found, [4]);
    }

    #[test]
    fn non_shape_scrutinee_and_go_switches_stay_silent() {
        let rule = CatchAllMatch;
        let source = "fn f() {}\n";
        let mut out = Vec::new();
        rule.check(
            &facts(
                Lang::Rust,
                source,
                vec![arm(ScrutineeKind::Other, 3, false)],
            ),
            &mut out,
        );
        assert!(out.is_empty());
        let mut out = Vec::new();
        rule.check(
            &facts(Lang::Go, source, vec![arm(ScrutineeKind::Ident, 3, false)]),
            &mut out,
        );
        assert!(out.is_empty());
    }
    #[test]
    fn preset_pins_off_at_standard_and_info_at_strict() {
        assert_eq!(
            INFO.confidence,
            Confidence::Low,
            "catch-all-match ships at low confidence"
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
        let strict_preset = preset(Strictness::Strict);
        assert_eq!(
            strict_preset.level,
            Level::At(Severity::Info),
            "strict reports at info"
        );
        let rule =
            build(strict_preset.options, &Settings::default()).expect("strict preset builds");
        let mut out = Vec::new();
        rule.check(
            &facts(
                Lang::Rust,
                "fn f() {}\n",
                vec![arm(ScrutineeKind::Path, 2, false)],
            ),
            &mut out,
        );
        assert_eq!(
            out.len(),
            1,
            "strict flags the wildcard on a positive input"
        );
    }
}
