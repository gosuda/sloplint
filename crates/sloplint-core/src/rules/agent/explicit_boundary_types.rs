use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::lang::Lang;
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};
use crate::rules::{EmptyOptions, parse_options};

const APPLICABLE: &[Lang] = &[
    Lang::TypeScript,
    Lang::Tsx,
    Lang::Python,
    Lang::Kotlin,
    Lang::Scala,
    Lang::Dart,
];

const FIX: &str = "annotate every parameter and the return type";

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/explicit-boundary-types",
    family: Family::Agent,
    summary: "public functions with unannotated parameters or return types",
    why: "Unannotated public signatures force callers to read the body to learn the contract.",
    confidence: Confidence::Medium,
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

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal => Level::Off,
        Strictness::Basic | Strictness::Standard | Strictness::Strict => Level::At(Severity::Minor),
    };
    Preset {
        level,
        options: toml::Table::new(),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let _: EmptyOptions = parse_options(options)?;
    Ok(Box::new(ExplicitBoundaryTypes))
}

struct ExplicitBoundaryTypes;

impl FileRule for ExplicitBoundaryTypes {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        if !APPLICABLE.contains(&facts.lang) {
            return;
        }
        for function in &facts.functions {
            if !function.actionable_api() {
                continue;
            }
            let missing: Vec<&str> = function
                .missing_annotations
                .iter()
                .copied()
                .filter(|annotation| *annotation != "return" || function.has_value_return)
                .collect();
            if missing.is_empty() {
                continue;
            }
            let name = function.name;
            let message = format!(
                "public `{name}` lacks type annotations for: {}",
                missing.join(", ")
            );
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                Severity::Minor,
                message,
                FIX.to_string(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Function, Param, Pos, Visibility};
    use crate::finding::line_starts;

    fn facts<'s>(source: &'s str, functions: Vec<Function<'s>>) -> FileFacts<'s> {
        FileFacts {
            lang: Lang::Python,
            path: "pkg/mod.py",
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
        }
    }

    fn valued<'s>(name: &'s str, missing: Vec<&'s str>) -> Function<'s> {
        function(name, missing, true)
    }

    fn voided<'s>(name: &'s str, missing: Vec<&'s str>) -> Function<'s> {
        function(name, missing, false)
    }

    fn function<'s>(name: &'s str, missing: Vec<&'s str>, has_value_return: bool) -> Function<'s> {
        Function {
            name,
            pos: Pos { line: 2, byte: 0 },
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
            params: Vec::<Param<'s>>::new(),
            return_type: None,
            untyped_throws: false,
            missing_annotations: missing,
            has_value_return,
            cognitive: 0,
            max_depth: 0,
            deepest_line: 3,
            end_line: 3,
            sole_call: None,
        }
    }

    #[test]
    fn valueless_body_stays_silent_while_untyped_body_flags() {
        let rule = ExplicitBoundaryTypes;
        let source = "def main():\n    print(1)\n";
        let mut out = Vec::new();
        rule.check(
            &facts(
                source,
                vec![
                    voided("main", vec!["return"]),
                    valued("process", vec!["data", "count", "return"]),
                ],
            ),
            &mut out,
        );
        let found: Vec<String> = out.into_iter().map(|finding| finding.message).collect();
        assert_eq!(
            found,
            ["public `process` lacks type annotations for: data, count, return".to_string()]
        );
    }
}
