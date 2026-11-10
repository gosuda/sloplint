use crate::config::Strictness;
use crate::facts::{AssertShape, FileFacts};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::lang::Lang;
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};
use crate::rules::{EmptyOptions, parse_options};

const MESSAGE: &str = "assertion without a failure message";
const RUST_COMPARISON_FIX: &str = "use assert_eq!/assert_ne! or add a message naming the invariant";
const GENERIC_FIX: &str = "add a message that names the violated invariant and the observed value";

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/informative-assertions",
    family: Family::Agent,
    summary: "assertions without a failure message",
    why: "A bare assertion failure says nothing about the broken invariant or the values involved.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Keep,
        support: Shift::Keep,
        bench: Shift::Off,
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
    Ok(Box::new(InformativeAssertions))
}

struct InformativeAssertions;

impl FileRule for InformativeAssertions {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for assertion in &facts.assertions {
            if assertion.has_message || assertion.in_typecheck {
                continue;
            }
            if !needs_message(facts.lang, assertion.shape, assertion.value_is_bool_literal) {
                continue;
            }
            let fix = if facts.lang == Lang::Rust && assertion.shape == AssertShape::TopEqNe {
                RUST_COMPARISON_FIX
            } else {
                GENERIC_FIX
            };
            out.push(Finding::at(
                facts,
                assertion.pos,
                INFO.id,
                Severity::Major,
                MESSAGE.to_string(),
                fix.to_string(),
            ));
        }
    }
}

fn needs_message(lang: Lang, shape: AssertShape, value_is_bool_literal: bool) -> bool {
    match lang {
        Lang::Rust => shape == AssertShape::BareName || shape == AssertShape::TopEqNe,
        Lang::Python => shape == AssertShape::BareName,
        Lang::Go => value_is_bool_literal,
        _ => shape != AssertShape::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Assertion, Pos};
    use crate::finding::line_starts;

    fn facts<'s>(lang: Lang, source: &'s str, assertions: Vec<Assertion>) -> FileFacts<'s> {
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
            wildcard_arms: Vec::new(),
            handlers: Vec::new(),
            assertions,
            field_copies: Vec::new(),
            dynamic_symbols: Vec::new(),
            ambient: Vec::new(),
            barrel: None,
            has_inline_tests: false,
        }
    }
    fn assertion(shape: AssertShape) -> Assertion {
        Assertion {
            pos: Pos { line: 7, byte: 0 },
            has_message: false,
            comparison: shape == AssertShape::TopEqNe,
            shape,
            in_typecheck: false,
            value_is_bool_literal: false,
        }
    }

    fn literal_assertion() -> Assertion {
        Assertion {
            pos: Pos { line: 7, byte: 0 },
            has_message: false,
            comparison: false,
            shape: AssertShape::Other,
            in_typecheck: false,
            value_is_bool_literal: true,
        }
    }

    fn lines(
        rule: &InformativeAssertions,
        lang: Lang,
        assertions: Vec<Assertion>,
    ) -> Vec<(u32, String)> {
        let source = "fn f() {}\n";
        let mut out = Vec::new();
        rule.check(&facts(lang, source, assertions), &mut out);
        out.into_iter()
            .map(|finding| (finding.line, finding.fix))
            .collect()
    }

    #[test]
    fn expression_conditions_stay_silent_while_bare_names_flag() {
        let rule = InformativeAssertions;
        let found = lines(
            &rule,
            Lang::Rust,
            vec![
                assertion(AssertShape::Other),
                assertion(AssertShape::BareName),
                assertion(AssertShape::TopEqNe),
            ],
        );
        assert_eq!(
            found,
            [
                (7, GENERIC_FIX.to_string()),
                (7, RUST_COMPARISON_FIX.to_string()),
            ]
        );
    }

    #[test]
    fn python_calls_and_go_variables_stay_silent() {
        let rule = InformativeAssertions;
        let found = lines(
            &rule,
            Lang::Python,
            vec![
                assertion(AssertShape::Other),
                assertion(AssertShape::BareName),
                assertion(AssertShape::TopEqNe),
            ],
        );
        assert_eq!(found, [(7, GENERIC_FIX.to_string())]);
        let found = lines(
            &rule,
            Lang::Go,
            vec![assertion(AssertShape::Other), literal_assertion()],
        );
        assert_eq!(found, [(7, GENERIC_FIX.to_string())]);
        let found = lines(
            &rule,
            Lang::C,
            vec![
                assertion(AssertShape::Other),
                assertion(AssertShape::BareName),
            ],
        );
        assert_eq!(found, [(7, GENERIC_FIX.to_string())]);
    }
    #[test]
    fn preset_pins_off_at_standard_and_info_at_strict() {
        assert_eq!(
            INFO.confidence,
            Confidence::Low,
            "informative-assertions ships at low confidence"
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
                vec![assertion(AssertShape::BareName)],
            ),
            &mut out,
        );
        assert_eq!(
            out.len(),
            1,
            "strict flags the bare assert on a positive input"
        );
    }
}
