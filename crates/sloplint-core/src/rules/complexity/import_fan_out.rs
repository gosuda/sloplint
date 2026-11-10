use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::{FileFacts, ImportScope, Pos};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, threshold_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "complexity/import-fan-out",
    family: Family::Complexity,
    summary: "files importing from too many distinct modules",
    why: "Many imported modules mean many files to open before an edit is safe.",
    confidence: Confidence::Medium,
    roles: RolePolicy {
        entry: Shift::Lower1,
        test: Shift::Lower1,
        support: Shift::Keep,
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

struct ImportFanOut {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, minor_at, major_at, critical_at, blocker_at) = match strictness {
        Strictness::Minimal => (Level::Off, 15, 25, 38, 50),
        Strictness::Basic => (Level::Off, 15, 25, 38, 50),
        Strictness::Standard => (Level::At(Severity::Minor), 15, 25, 38, 50),
        Strictness::Strict => (Level::At(Severity::Major), 15, 25, 38, 50),
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
    Ok(Box::new(ImportFanOut { thresholds }))
}

fn collect<'s>(facts: &FileFacts<'s>) -> Option<(BTreeSet<&'s str>, Pos)> {
    if facts.imports.iter().all(|import| import.is_reexport) {
        return None;
    }
    let mut roots: BTreeSet<&'s str> = BTreeSet::new();
    let mut pos: Option<Pos> = None;
    for import in &facts.imports {
        let Some(root) = import.external_root else {
            continue;
        };
        if import.is_reexport || import.scope != ImportScope::Module {
            continue;
        }
        if pos.is_none() {
            pos = Some(import.pos);
        }
        roots.insert(root);
    }
    pos.map(|pos| (roots, pos))
}

impl FileRule for ImportFanOut {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        let Some((roots, pos)) = collect(facts) else {
            return;
        };
        let count = u32::try_from(roots.len()).unwrap_or(u32::MAX);
        let Some(severity) = self.thresholds.grade(count) else {
            return;
        };
        let minor_at = self.thresholds.minor_at;
        let major_at = self.thresholds.major_at;
        let critical_at = self.thresholds.critical_at;
        let blocker_at = self.thresholds.blocker_at;
        let message = format!(
            "file imports from {count} distinct external packages (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})"
        );
        let fix = "split orchestration from domain logic: move code that needs only some of these packages into its own module"
            .to_string();
        out.push(Finding::at(facts, pos, INFO.id, severity, message, fix));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Import, ImportKind, ImportScope, Pos};
    use crate::lang::Lang;
    use crate::role::CodeRole;
    use std::borrow::Cow;
    fn check(imports: Vec<Import<'_>>) -> Vec<Finding> {
        let rule = ImportFanOut {
            thresholds: Thresholds {
                minor_at: 2,
                major_at: 3,
                critical_at: 4,
                blocker_at: 5,
            },
        };
        let facts = FileFacts {
            lang: Lang::Rust,
            path: "src/lib.rs",
            source: "",
            line_starts: Vec::new(),
            role: CodeRole::Library,
            regions: Vec::new(),
            line_count: 0,
            package: String::new(),
            is_generated: false,
            functions: Vec::new(),
            calls: Vec::new(),
            bindings: Vec::new(),
            imports,
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
        out
    }
    fn import<'s>(module: &'s str, root: Option<&'s str>) -> Import<'s> {
        Import {
            pos: Pos { line: 1, byte: 0 },
            module: Cow::Borrowed(module),
            name: "item",
            alias: None,
            kind: ImportKind::Named,
            scope: ImportScope::Module,
            in_test: false,
            is_reexport: false,
            external_root: root,
        }
    }
    #[test]
    fn shared_root_counts_once() {
        let out = check(vec![
            import("serde::de", Some("serde")),
            import("serde::ser", Some("serde")),
        ]);
        assert!(out.is_empty());
    }
    #[test]
    fn stdlib_none_roots_stay_silent() {
        let out = check(vec![import("std::fs", None), import("std::io", None)]);
        assert!(out.is_empty());
    }
    #[test]
    fn distinct_roots_flag() {
        let out = check(vec![
            import("serde", Some("serde")),
            import("tokio", Some("tokio")),
        ]);
        assert_eq!(out.len(), 1);
    }
    #[test]
    fn test_region_roots_still_count() {
        let mut tested = import("serde", Some("serde"));
        tested.in_test = true;
        let out = check(vec![tested, import("tokio", Some("tokio"))]);
        assert_eq!(out.len(), 1);
    }
}
