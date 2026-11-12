use std::collections::{HashMap, HashSet};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::WorldFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::lang::Lang;
use crate::role::{CodeRole, RolePolicy, Shift};
use crate::rules::{Build, Family, Preset, RuleInfo, WorldFile, WorldRule};
use crate::rules::{parse_options, string_values};

const EXCLUDE: &[&str] = &[
    "**/_*example/**",
    "**/ossfuzz/**",
    "**/main.rs",
    "**/main.go",
    "**/bin/**",
];

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/colocated-tests",
    family: Family::Agent,
    summary: "modules with public functions and no tests nearby",
    why: "A module without nearby tests gives no executable specification to check an edit against.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Off,
        test: Shift::Off,
        support: Shift::Off,
        bench: Shift::Off,
        example: Shift::Off,
    },
    preset,
    build: Build::World(build),
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    exclude: Vec<String>,
}

struct ColocatedTests {
    exclude: GlobSet,
}

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal | Strictness::Basic | Strictness::Standard => Level::Off,
        Strictness::Strict => Level::At(Severity::Info),
    };
    let mut options = toml::Table::new();
    options.insert("exclude".to_string(), string_values(EXCLUDE));
    Preset { level, options }
}

fn build(options: toml::Table) -> Result<Box<dyn WorldRule>, String> {
    let parsed: Options = parse_options(options)?;
    let mut builder = GlobSetBuilder::new();
    for pattern in &parsed.exclude {
        let glob =
            Glob::new(pattern).map_err(|error| format!("invalid glob `{pattern}`: {error}"))?;
        builder.add(glob);
    }
    let exclude = builder.build().map_err(|error| error.to_string())?;
    Ok(Box::new(ColocatedTests { exclude }))
}

impl WorldRule for ColocatedTests {
    fn check(&self, files: &[WorldFile<'_>], out: &mut Vec<Finding>) {
        let imported = import_covered(files);
        let mut evidenced: HashSet<(&str, &str)> = HashSet::new();
        for file in files {
            if is_evidence(file) {
                evidenced.insert((coverage_scope(file.facts.lang), file.facts.package.as_str()));
            }
        }
        for file in files {
            if self.skipped(file) {
                continue;
            }
            let scope = (coverage_scope(file.facts.lang), file.facts.package.as_str());
            if !evidenced.contains(&scope) {
                continue;
            }
            if imported.contains(file.path) {
                continue;
            }
            out.push(self.finding(file));
        }
    }
}

impl ColocatedTests {
    fn skipped(&self, file: &WorldFile<'_>) -> bool {
        let facts: &WorldFacts = file.facts;
        if facts.role != CodeRole::Library
            || facts.public_functions == 0
            || self.exclude.is_match(file.path)
        {
            return true;
        }
        crate::module_keys::library_keys(facts.lang, file.path).is_empty()
    }

    fn finding(&self, file: &WorldFile<'_>) -> Finding {
        let package = file.facts.package.as_str();
        let scope = if package.is_empty() {
            String::new()
        } else {
            format!(" `{package}`")
        };
        Finding::at_file(
            file.path,
            INFO.id,
            Severity::Info,
            format!(
                "module with {} public functions is not named by any test in its package{scope} (no test file or test import shares its module name)",
                file.facts.public_functions
            ),
            if package.is_empty() {
                "add tests covering this module".to_string()
            } else {
                format!("add tests covering package `{package}`")
            },
        )
    }
}

fn is_evidence(file: &WorldFile<'_>) -> bool {
    file.facts.has_inline_tests
        || matches!(file.facts.role, CodeRole::Test | CodeRole::Bench)
        || file
            .facts
            .regions
            .iter()
            .any(|region| matches!(region.role, CodeRole::Test | CodeRole::Bench))
}

#[derive(Default)]
struct PackageKeys<'a> {
    any: Option<usize>,
    packages: HashMap<&'a str, usize>,
}

impl PackageKeys<'_> {
    fn covers(&self, package: Option<&str>, depth: usize, longest: usize) -> bool {
        self.need(package)
            .is_some_and(|need| need <= depth || depth == longest)
    }

    fn need(&self, package: Option<&str>) -> Option<usize> {
        let scoped = match package {
            Some(package) => self.packages.get(package).copied(),
            None => self.packages.values().copied().min(),
        };
        self.any.into_iter().chain(scoped).min()
    }
}

#[derive(Default)]
struct RustCrate<'a> {
    modules: HashSet<String>,
    root: Option<(&'a str, &'a WorldFacts)>,
}

impl RustCrate<'_> {
    fn item_keys(&self, crate_key: &str, name: &str) -> Vec<Vec<String>> {
        let Some((root_path, root)) = self.root else {
            return Vec::new();
        };
        if self.modules.contains(&format!("{crate_key}/{name}")) {
            return Vec::new();
        }
        let root_keys = || vec![crate::module_keys::library_keys(Lang::Rust, root_path)];
        if root.definitions.iter().any(|site| &*site.name == name) {
            return root_keys();
        }
        if let Some(named) = root.reexports.iter().find(|item| &*item.name == name) {
            return vec![crate::module_keys::rust_module_keys(
                crate_key,
                &named.module,
            )];
        }
        let globs: Vec<Vec<String>> = root
            .reexports
            .iter()
            .filter(|item| &*item.name == "*")
            .map(|item| crate::module_keys::rust_module_keys(crate_key, &item.module))
            .collect();
        if globs.is_empty() { root_keys() } else { globs }
    }
}

fn rust_crates<'a>(files: &[WorldFile<'a>]) -> HashMap<String, RustCrate<'a>> {
    let mut crates: HashMap<String, RustCrate<'a>> = HashMap::new();
    for file in files {
        if file.facts.lang != Lang::Rust || file.facts.role != CodeRole::Library {
            continue;
        }
        let Some(crate_key) = crate::module_keys::rust_crate_key(file.path) else {
            continue;
        };
        let entry = crates.entry(crate_key).or_default();
        if crate::module_keys::rust_is_crate_root(file.path) {
            entry.root = Some((file.path, file.facts));
        }
        entry
            .modules
            .extend(crate::module_keys::library_keys(Lang::Rust, file.path));
    }
    crates
}

fn import_covered<'a>(files: &[WorldFile<'a>]) -> HashSet<&'a str> {
    let mut keys: HashMap<(&'a str, String), PackageKeys<'a>> = HashMap::new();
    let crates = rust_crates(files);
    for file in files {
        if !is_evidence(file) {
            continue;
        }
        record_library_keys(&mut keys, file);
        for spec in &file.facts.imports {
            record_import_keys(&mut keys, file, spec);
            record_root_item_keys(&mut keys, file, spec, &crates);
        }
    }
    if keys.is_empty() {
        return HashSet::new();
    }
    let mut covered: HashSet<&'a str> = HashSet::new();
    for file in files {
        if file.facts.role != CodeRole::Library || file.facts.public_functions == 0 {
            continue;
        }
        if is_covered(&keys, file) {
            covered.insert(file.path);
        }
    }
    covered
}

fn key_len(key: &str) -> usize {
    key.split('/').count()
}

fn coverage_scope(lang: Lang) -> &'static str {
    lang.namespace().config_key()
}

fn scoped_package<'a>(file: &WorldFile<'a>) -> Option<&'a str> {
    if file.facts.package_from_manifest {
        Some(file.facts.package.as_str())
    } else {
        None
    }
}

fn record_library_keys<'a>(
    keys: &mut HashMap<(&'a str, String), PackageKeys<'a>>,
    file: &WorldFile<'a>,
) {
    record(
        keys,
        file,
        crate::module_keys::test_keys(file.facts.lang, file.path),
    );
}

fn record_import_keys<'a>(
    keys: &mut HashMap<(&'a str, String), PackageKeys<'a>>,
    file: &WorldFile<'a>,
    spec: &str,
) {
    record(
        keys,
        file,
        crate::module_keys::import_keys(file.facts.lang, file.path, spec),
    );
}

fn record_root_item_keys<'a>(
    keys: &mut HashMap<(&'a str, String), PackageKeys<'a>>,
    file: &WorldFile<'a>,
    spec: &str,
    crates: &HashMap<String, RustCrate<'a>>,
) {
    if file.facts.lang != Lang::Rust {
        return;
    }
    let Some((crate_key, name)) = crate::module_keys::rust_root_item(file.path, spec) else {
        return;
    };
    if let Some(owner) = crates.get(&crate_key) {
        for owned in owner.item_keys(&crate_key, name) {
            record(keys, file, owned);
        }
    }
}

fn record<'a>(
    keys: &mut HashMap<(&'a str, String), PackageKeys<'a>>,
    file: &WorldFile<'a>,
    owned: Vec<String>,
) {
    let longest = owned.iter().map(|key| key_len(key)).max().unwrap_or(0);
    if longest == 0 {
        return;
    }
    let scope = coverage_scope(file.facts.lang);
    let package = scoped_package(file);
    for key in owned {
        let slot = keys.entry((scope, key)).or_default();
        if let Some(package) = package {
            slot.packages
                .entry(package)
                .and_modify(|need| *need = (*need).min(longest))
                .or_insert(longest);
        } else {
            slot.any = Some(slot.any.map_or(longest, |need| need.min(longest)));
        }
    }
}

fn is_covered<'a>(
    keys: &HashMap<(&'a str, String), PackageKeys<'a>>,
    file: &WorldFile<'a>,
) -> bool {
    let owned = crate::module_keys::library_keys(file.facts.lang, file.path);
    let longest = owned.iter().map(|key| key_len(key)).max().unwrap_or(0);
    let scope = coverage_scope(file.facts.lang);
    let package = scoped_package(file);
    owned.iter().any(|key| {
        keys.get(&(scope, key.clone()))
            .is_some_and(|slot| slot.covers(package, key_len(key), longest))
    })
}

#[cfg(test)]
#[path = "colocated_tests_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "colocated_tests_scope_tests.rs"]
mod scope_tests;
