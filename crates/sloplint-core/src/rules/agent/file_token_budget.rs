use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{CodeRole, RolePolicy, RoleRegion, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/file-token-budget",
    family: Family::Agent,
    summary: "files longer than the profile's line budget",
    why: "Large files exceed what a reader or agent can hold and edit reliably.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Off,
        support: Shift::Off,
        bench: Shift::Lower1,
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

struct FileTokenBudget {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (Level::Off, 3000, 6000, 9000, 12000),
        Strictness::Basic => (Level::Off, 2000, 4000, 6000, 8000),
        Strictness::Standard => (Level::Off, 1500, 3000, 4500, 6000),
        Strictness::Strict => (Level::At(Severity::Info), 600, 1000, 1500, 2000),
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
    Ok(Box::new(FileTokenBudget { thresholds }))
}

impl FileRule for FileTokenBudget {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        if facts.is_generated {
            return;
        }
        let billable = billable_lines(facts.line_count, &facts.regions);
        let Some(severity) = self.thresholds.grade(billable) else {
            return;
        };
        let minor_at = self.thresholds.minor_at;
        let major_at = self.thresholds.major_at;
        let critical_at = self.thresholds.critical_at;
        let blocker_at = self.thresholds.blocker_at;
        let fix = format!("split the file by concern into modules under {minor_at} lines");
        out.push(Finding::at_file(
            facts.path,
            INFO.id,
            severity,
            format!("file has {billable} lines (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})"),
            fix,
        ));
    }
}

fn billable_lines(line_count: u32, regions: &[RoleRegion]) -> u32 {
    let mut spans: Vec<(u32, u32)> = Vec::new();
    for region in regions {
        if region.role != CodeRole::Test {
            continue;
        }
        let start = region.start_line.max(1);
        let end = region.end_line.min(line_count);
        if start <= end {
            spans.push((start, end));
        }
    }
    spans.sort();
    let mut covered: u32 = 0;
    let mut open: Option<(u32, u32)> = None;
    for (start, end) in spans {
        match open {
            None => open = Some((start, end)),
            Some((kept_start, kept_end)) if start <= kept_end.saturating_add(1) => {
                open = Some((kept_start, end.max(kept_end)));
            }
            Some((kept_start, kept_end)) => {
                covered =
                    covered.saturating_add(kept_end.saturating_sub(kept_start).saturating_add(1));
                open = Some((start, end));
            }
        }
    }
    if let Some((kept_start, kept_end)) = open {
        covered = covered.saturating_add(kept_end.saturating_sub(kept_start).saturating_add(1));
    }
    line_count.saturating_sub(covered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::line_starts;
    use crate::lang::Lang;
    use crate::role::{CodeRole, RoleRegion};

    fn source_facts(source: &str, line_count: u32) -> FileFacts<'_> {
        FileFacts {
            lang: Lang::Rust,
            path: "src/parser.rs",
            source,
            line_starts: line_starts(source),
            role: crate::role::CodeRole::Library,
            regions: Vec::new(),
            line_count,
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
            dynamic_symbols: Vec::new(),
            ambient: Vec::new(),
            barrel: None,
            has_inline_tests: false,
        }
    }

    fn generated_facts(source: &str, line_count: u32) -> FileFacts<'_> {
        FileFacts {
            lang: Lang::Rust,
            path: "src/table.rs",
            source,
            line_starts: line_starts(source),
            role: crate::role::CodeRole::Library,
            regions: Vec::new(),
            line_count,
            package: String::new(),
            is_generated: true,
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
            dynamic_symbols: Vec::new(),
            ambient: Vec::new(),
            barrel: None,
            has_inline_tests: false,
        }
    }

    fn strict() -> Box<dyn FileRule> {
        let preset = preset(Strictness::Strict);
        build(preset.options, &Settings::default()).expect("strict preset builds")
    }

    #[test]
    fn preset_pins_off_at_standard_and_info_at_strict() {
        assert_eq!(
            INFO.confidence,
            Confidence::Low,
            "file-token-budget ships at low confidence"
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
        let minor = strict_preset
            .options
            .get("minor_at")
            .and_then(|value| value.as_integer());
        assert_eq!(minor, Some(600), "strict keeps the 600-line trigger");
        let mut out = Vec::new();
        strict().check(&source_facts("fn f() {}\n", 1000), &mut out);
        assert_eq!(
            out.len(),
            1,
            "strict flags a 1000-line file on a positive input"
        );
    }

    #[test]
    fn test_regions_are_excluded_from_the_budget() {
        let mut facts = source_facts("fn f() {}\n", 1700);
        facts.regions = vec![
            RoleRegion {
                start_line: 1301,
                end_line: 1700,
                role: CodeRole::Test,
            },
            RoleRegion {
                start_line: 1400,
                end_line: 1500,
                role: CodeRole::Test,
            },
        ];
        let mut out = Vec::new();
        strict().check(&facts, &mut out);
        assert_eq!(out.len(), 1, "the file still flags after excluding tests");
        assert_eq!(
            out[0].severity,
            Severity::Major,
            "1700 lines minus 400 test lines grades as 1300"
        );
        assert!(
            out[0].message.contains("1300 lines"),
            "the message reports the billable count"
        );
    }

    #[test]
    fn generated_table_and_mid_size_module_stay_silent_while_god_file_flags() {
        let rule = strict();
        let source = "fn f() {}\n";
        let mut out = Vec::new();
        rule.check(&generated_facts(source, 1846), &mut out);
        assert!(out.is_empty(), "generated files stay silent");
        let mut out = Vec::new();
        rule.check(&source_facts(source, 599), &mut out);
        assert!(
            out.is_empty(),
            "files under the strict minor trigger stay silent"
        );
        let mut severities = Vec::new();
        for lines in [600, 1000] {
            let mut out = Vec::new();
            rule.check(&source_facts(source, lines), &mut out);
            severities.extend(out.into_iter().map(|finding| finding.severity));
        }
        assert_eq!(
            severities,
            [Severity::Minor, Severity::Major],
            "strict grades 600 lines as minor and 1000 as major"
        );
    }
}
