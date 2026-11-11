use super::{EmptyOptions, parse_options};
use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/no-dynamic-symbol",
    family: Family::Grep,
    summary: "symbols resolved by runtime-built names",
    why: "A name built at runtime appears nowhere in the source, so search and static analysis miss it.",
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

struct NoDynamicSymbol;

fn preset(strictness: Strictness) -> Preset {
    Preset {
        level: match strictness {
            Strictness::Minimal | Strictness::Basic | Strictness::Standard => Level::Off,
            Strictness::Strict => Level::At(Severity::Info),
        },
        options: toml::Table::new(),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    parse_options::<EmptyOptions>(options)?;
    Ok(Box::new(NoDynamicSymbol))
}

impl FileRule for NoDynamicSymbol {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for symbol in &facts.dynamic_symbols {
            out.push(Finding::at(
                facts,
                symbol.pos,
                INFO.id,
                Severity::Major,
                format!("{} builds a name at runtime", symbol.what),
                "dispatch through a static table that maps literal keys to functions, so every target name appears in the source"
                    .to_string(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{DynamicSymbol, Pos};
    use crate::lang::Lang;
    use crate::role::CodeRole;

    fn facts() -> FileFacts<'static> {
        FileFacts {
            lang: Lang::Python,
            path: "pkg/mod.py",
            source: "",
            line_starts: vec![0],
            role: CodeRole::Library,
            regions: Vec::new(),
            line_count: 1,
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
            assertions: Vec::new(),
            field_copies: Vec::new(),
            dynamic_symbols: vec![DynamicSymbol {
                pos: Pos { line: 1, byte: 0 },
                what: "getattr",
            }],
            ambient: Vec::new(),
            barrel: None,
            has_inline_tests: false,
        }
    }

    #[test]
    fn off_at_standard_info_at_strict() {
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
            "strict stays informational"
        );
        assert_eq!(INFO.confidence, Confidence::Low, "confidence stays low");
        let facts = facts();
        let mut out = Vec::new();
        NoDynamicSymbol.check(&facts, &mut out);
        assert_eq!(
            out.len(),
            1,
            "strict flags a runtime-built name once: {out:?}"
        );
    }
}
