pub mod cli;
pub mod init;
pub mod rules_cmd;

mod cache;
mod discover;
mod error;
mod fileio;
mod globs;
mod pipeline;
mod project;
mod roles;
mod scopes;
pub mod settings;
mod skips;
mod store;
mod terminal;
pub use error::RunError;
pub use store::read_source;
pub use terminal::{TerminalText, next_control};

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use sloplint_core::baseline::{Baseline, SkippedEntry};
use sloplint_core::finding::{Confidence, Severity};
use sloplint_core::rules::{RuleFilter, RuleSet};
use sloplint_core::{FailOn, Finding, Mode, Strictness};
use sloplint_syntax::{CustomRules, shadowed_sources};

use cache::Loaded;
use error::io_error;
use pipeline::Pipeline;
use project::Overrides;
use roles::RoleSets;
use scopes::{Scopes, scopes};
use store::{load_baseline, record_baseline};
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub cwd: PathBuf,
    pub root: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub paths: Vec<PathBuf>,
    pub strictness: Option<Strictness>,
    pub mode: Option<Mode>,
    pub fail_on: Option<FailOn>,
    pub min_confidence: Option<Confidence>,
    pub fail_on_skipped: bool,
    pub rule_filter: RuleFilter,
    pub cache_root: Option<PathBuf>,
    /// The cache lives under the per-user cache home, never the checkout: a repository can ship a forged cache.
    pub use_cache: bool,
    pub jobs: usize,
    pub write_baseline: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub strictness: Strictness,
    pub files: u32,
    pub findings: u32,
    pub blocker: u32,
    pub critical: u32,
    pub major: u32,
    pub minor: u32,
    pub info: u32,
    pub baselined: u32,
    pub stale: u32,
    pub skipped: u32,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub rules_fired: BTreeMap<String, String>,
    pub summary: Summary,
    pub skipped_gate: Vec<SkippedEntry>,
}

#[derive(Debug, Clone)]
pub struct Execution {
    pub report: Report,
    pub root: PathBuf,
    pub baseline_written: Option<String>,
    pub failed: bool,
    pub max_file_bytes: u64,
    pool: Arc<rayon::ThreadPool>,
}

impl Execution {
    pub fn install<R: Send>(&self, op: impl FnOnce() -> R + Send) -> R {
        self.pool.install(op)
    }
}

pub fn run(opts: &RunOptions) -> Result<Report, RunError> {
    execute(opts).map(|execution| execution.report)
}

pub fn implicit_scope(
    cwd: &Path,
    root: Option<&Path>,
    config: Option<&Path>,
) -> Result<Vec<PathBuf>, RunError> {
    let explicit = root.is_some();
    let location = project::locate(cwd, root, config);
    let root = fs::canonicalize(&location.root).map_err(|source| {
        if (source.kind() == io::ErrorKind::NotFound
            || source.kind() == io::ErrorKind::NotADirectory)
            && (explicit || config.is_some())
        {
            RunError::PathNotFound {
                path: location.root.clone(),
            }
        } else {
            io_error(&location.root, source)
        }
    })?;
    if explicit && !root.is_dir() {
        return Err(RunError::NotADirectory { path: root });
    }
    let here = fs::canonicalize(cwd).map_err(|source| io_error(cwd, source))?;
    Ok(if here != root && here.starts_with(&root) {
        vec![cwd.to_path_buf()]
    } else {
        Vec::new()
    })
}

pub fn execute(opts: &RunOptions) -> Result<Execution, RunError> {
    let location = project::locate(&opts.cwd, opts.root.as_deref(), opts.config.as_deref());
    let root = fs::canonicalize(&location.root).map_err(|source| {
        if (source.kind() == io::ErrorKind::NotFound
            || source.kind() == io::ErrorKind::NotADirectory)
            && (opts.root.is_some() || opts.config.is_some())
        {
            RunError::PathNotFound {
                path: location.root.clone(),
            }
        } else {
            io_error(&location.root, source)
        }
    })?;
    if opts.root.is_some() && !root.is_dir() {
        return Err(RunError::NotADirectory { path: root });
    }
    let loaded = project::load(
        &location,
        Overrides {
            strictness: opts.strictness,
            mode: opts.mode,
            fail_on: opts.fail_on,
            min_confidence: opts.min_confidence,
        },
    )?;
    let config = loaded.config;
    let rules = RuleSet::new(&config, &opts.rule_filter)?;
    let custom = CustomRules::compile(&config.custom_rules)?;
    let scopes = scopes(&root, &opts.cwd, &opts.paths)?;
    let workers = jobs(opts.jobs);
    let pool = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map_err(|error| io_error(&root, io::Error::other(error)))?,
    );
    let discovery = discover::discover(&root, &config, workers)?;
    let cache = if opts.use_cache {
        cache::open(
            opts.cache_root.clone(),
            &root,
            &config,
            &loaded.custom_sources,
        )
    } else {
        None
    };
    let loaded = cache
        .as_ref()
        .map_or_else(Loaded::empty, |cache| pool.install(|| cache.load()));
    let roles = RoleSets::from_config(&config)?;
    let shadowed: HashSet<String> =
        shadowed_sources(discovery.entries.iter().map(|entry| entry.rel.as_str()));
    let pipe = Pipeline {
        rules: &rules,
        custom: &custom,
        config: &config,
        root: &root,
        roles: &roles,
        cache: &loaded,
        manifests: &discovery.manifests,
        shadowed: &shadowed,
        active_rules: OnceLock::new(),
    };
    let (analyzed, pipe_skipped) = pipeline::run(&discovery.entries, &pipe, &pool)?;
    let skipped_entries = if opts.write_baseline {
        skips::collect(pipe_skipped, &discovery.skipped, None)
    } else {
        skips::collect(pipe_skipped, &discovery.skipped, Some(&scopes))
    };
    let skipped = count(skipped_entries.len());
    let files = pipeline::count_in_scope(&analyzed, &scopes);
    if let Some(cache) = &cache {
        pool.install(|| pipeline::store_if_touched(cache, &analyzed, &loaded));
    }
    let mut all = pipeline::merge(&pipe, analyzed, &pool)?;
    all.retain(|finding| scopes.contains(Path::new(&*finding.path)));
    let (reported, baselined, stale, baseline) = if opts.write_baseline {
        (all, 0, 0, Baseline::empty())
    } else {
        apply_gate(&pipe, &scopes, all)?
    };
    let baseline_written = if opts.write_baseline {
        let path = project::resolve_within_root(&root, &config.project.baseline)?;
        record_baseline(&path, &reported, &skipped_entries)?;
        Some(config.project.baseline.clone())
    } else {
        None
    };
    let rung = |severity: Severity| {
        count(
            reported
                .iter()
                .filter(|finding| finding.severity == severity)
                .count(),
        )
    };
    let mut rules_fired: BTreeMap<String, String> = BTreeMap::new();
    for finding in &reported {
        if !rules_fired.contains_key(&*finding.rule) {
            let why = rules.why(&finding.rule).unwrap_or_default();
            rules_fired.insert(finding.rule.to_string(), why.to_string());
        }
    }
    let minimum = config.project.min_confidence;
    let gate_on = !opts.write_baseline && (config.project.fail_on_skipped || opts.fail_on_skipped);
    let skipped_gate: Vec<SkippedEntry> = if !gate_on {
        Vec::new()
    } else if pipe.config.project.mode != Mode::Ratchet {
        skipped_entries.clone()
    } else {
        baseline.newly_skipped(&skipped_entries)
    };
    let failed = !opts.write_baseline
        && (reported.iter().any(|finding| {
            config.project.fail_on.fails(finding.severity) && finding.confidence >= minimum
        }) || !skipped_gate.is_empty());
    let summary = Summary {
        strictness: config.project.strictness,
        files,
        findings: count(reported.len()),
        blocker: rung(Severity::Blocker),
        critical: rung(Severity::Critical),
        major: rung(Severity::Major),
        minor: rung(Severity::Minor),
        info: rung(Severity::Info),
        baselined,
        stale,
        skipped,
    };
    Ok(Execution {
        report: Report {
            findings: reported,
            rules_fired,
            summary,
            skipped_gate,
        },
        root,
        baseline_written,
        failed,
        max_file_bytes: config.project.max_file_bytes,
        pool,
    })
}

fn apply_gate(
    pipe: &Pipeline<'_>,
    scopes: &Scopes,
    all: Vec<Finding>,
) -> Result<(Vec<Finding>, u32, u32, Baseline), RunError> {
    if pipe.config.project.mode != Mode::Ratchet {
        return Ok((all, 0, 0, Baseline::empty()));
    }
    let mut baseline = load_baseline(pipe.root, &pipe.config.project.baseline)?;
    baseline
        .entries
        .retain(|entry| pipe.rules.selects(&entry.rule));
    let (kept, baselined, stale) =
        baseline.filter(all, &|path: &str| scopes.contains(Path::new(path)));
    Ok((kept, baselined, stale, baseline))
}

const MAX_JOBS: usize = 512;

fn jobs(requested: usize) -> usize {
    if requested == 0 {
        std::thread::available_parallelism()
            .map_or(1, NonZeroUsize::get)
            .min(MAX_JOBS)
    } else {
        requested.min(MAX_JOBS)
    }
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{MAX_JOBS, jobs};

    #[test]
    fn absurd_job_requests_clamp_to_the_documented_ceiling() {
        assert_eq!(
            jobs(usize::MAX),
            MAX_JOBS,
            "a library request above the ceiling must clamp instead of asking rayon for that many threads"
        );
        assert_eq!(
            jobs(7),
            7,
            "an in-range request must pass through unchanged"
        );
    }
}
