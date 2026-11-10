use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::{FileFacts, Import, ImportKind};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};
use crate::rules::{parse_options, string_values};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/no-rename-import",
    family: Family::Grep,
    summary: "import aliases that hide the original name from text search",
    why: "An alias removes the original name from text search at every use site.",
    confidence: Confidence::High,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    allow: Vec<String>,
}

struct NoRenameImport {
    allow: Vec<String>,
}

fn preset(strictness: Strictness) -> Preset {
    Preset {
        level: match strictness {
            Strictness::Minimal => Level::Off,
            Strictness::Basic | Strictness::Standard | Strictness::Strict => {
                Level::At(Severity::Minor)
            }
        },
        options: fixed_options(),
    }
}

fn fixed_options() -> toml::Table {
    let mut options = toml::Table::new();
    options.insert("allow".to_string(), string_values(&[]));
    options
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let options: Options = parse_options(options)?;
    let allow = options.allow;
    Ok(Box::new(NoRenameImport { allow }))
}

impl FileRule for NoRenameImport {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for (index, import) in facts.imports.iter().enumerate() {
            let Some(alias) = import.alias else { continue };
            if import.kind != ImportKind::Named {
                continue;
            }
            if self.resolves_collision(facts, index) || self.allowed(import, alias) {
                continue;
            }
            out.push(Finding::at(
                facts,
                import.pos,
                INFO.id,
                Severity::Major,
                format!("import alias `{alias}` hides `{}` from text search", import.name),
                format!(
                    "import `{}` under its own name and qualify it with its module where names collide",
                    import.name
                ),
            ));
        }
    }
}

impl NoRenameImport {
    fn resolves_collision(&self, facts: &FileFacts<'_>, index: usize) -> bool {
        let name = facts.imports[index].name;
        for (other, import) in facts.imports.iter().enumerate() {
            if other != index && import.name == name {
                return true;
            }
        }
        false
    }

    fn allowed(&self, import: &Import<'_>, alias: &str) -> bool {
        let bare = format!("{} as {alias}", import.name);
        if self.in_allow(&bare) {
            return true;
        }
        for separator in ["::", ".", "/"] {
            let qualified = format!("{}{separator}{} as {alias}", import.module, import.name);
            if self.in_allow(&qualified) {
                return true;
            }
        }
        let module = format!("{} as {alias}", import.module);
        self.in_allow(&module)
    }

    fn in_allow(&self, candidate: &str) -> bool {
        self.allow.iter().any(|entry| entry.as_str() == candidate)
    }
}
