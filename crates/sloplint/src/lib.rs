pub mod cli;
pub mod init;
pub mod rules_cmd;

mod cache;
mod discover;
mod fileio;
mod project;
mod settings;
mod store;
mod terminal;

pub use store::read_source;
pub use terminal::{TerminalText, next_control};

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use globset::GlobSet;
use rayon::prelude::*;
use serde::Serialize;
use sloplint_core::baseline::Baseline;
use sloplint_core::facts::WorldFacts;
use sloplint_core::lang::Lang;
use sloplint_core::rules::{CATALOG, RuleFilter, RuleSet, WorldFile};
use sloplint_core::suppress::{self, RulePattern, Suppression};
use sloplint_core::{Config, ConfigError, FailOn, Finding, Mode, Severity, Strictness};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};
use thiserror::Error;

use cache::{FileOutput, Loaded};
use discover::Entry;
use project::Overrides;
use store::{load_baseline, read_within_limit, record_baseline};

const GENERATED_WINDOW: usize = 2048;

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub cwd: PathBuf,
    pub root: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub paths: Vec<PathBuf>,
    pub strictness: Option<Strictness>,
    pub mode: Option<Mode>,
    pub fail_on: Option<FailOn>,
    pub rule_filter: RuleFilter,
    /// The cache lives under the per-user cache home, never the checkout: a repository can ship a forged cache.
    pub use_cache: bool,
    pub jobs: usize,
    pub write_baseline: bool,
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("path is outside the project root: {}", path.display())]
    PathOutsideRoot { path: PathBuf },
    #[error("path not found: {}", path.display())]
    PathNotFound { path: PathBuf },
    #[error("baseline file not found: {}; run 'sloplint baseline'", path.display())]
    BaselineMissing { path: PathBuf },
    #[error("baseline path is outside the project root: {}", path.display())]
    BaselineOutsideRoot { path: PathBuf },
    #[error("baseline {}: {message}", path.display())]
    BaselineCorrupt { path: PathBuf, message: String },
    #[error("{} exists and is not a sloplint baseline; refusing to overwrite it (delete the file to record a new baseline)", path.display())]
    NotABaseline { path: PathBuf },
    #[error("{} already exists", path.display())]
    ConfigExists { path: PathBuf },
}

impl RunError {
    pub fn exit_code(&self) -> u8 {
        match self {
            RunError::Io { .. } => 3,
            _ => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub strictness: Strictness,
    pub files: u32,
    pub findings: u32,
    pub deny: u32,
    pub warn: u32,
    pub baselined: u32,
    pub stale: u32,
    pub skipped: u32,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub rules_fired: BTreeMap<String, String>,
    pub summary: Summary,
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
    let location = project::locate(cwd, root, config);
    let root =
        fs::canonicalize(&location.root).map_err(|source| io_error(&location.root, source))?;
    let here = fs::canonicalize(cwd).map_err(|source| io_error(cwd, source))?;
    Ok(if here != root && here.starts_with(&root) {
        vec![cwd.to_path_buf()]
    } else {
        Vec::new()
    })
}

pub fn execute(opts: &RunOptions) -> Result<Execution, RunError> {
    let location = project::locate(&opts.cwd, opts.root.as_deref(), opts.config.as_deref());
    let root =
        fs::canonicalize(&location.root).map_err(|source| io_error(&location.root, source))?;
    let loaded = project::load(
        &location,
        Overrides {
            strictness: opts.strictness,
            mode: opts.mode,
            fail_on: opts.fail_on,
        },
    )?;
    let config = loaded.config;
    RuleSet::validate_filter(&config, &opts.rule_filter)?;
    let rules = RuleSet::new(&config, &RuleFilter::default())?;
    let custom = CustomRules::compile(&config.custom_rules)?;
    let scopes = project::scopes(&root, &opts.cwd, &opts.paths)?;
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
            settings::cache_base(),
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
    let test_globs = project::glob_set(&config.project.test_globs, "project.test_globs")?;
    let interfaces: HashSet<String> = discovery
        .entries
        .iter()
        .filter(|entry| entry.lang == Lang::OcamlInterface)
        .filter_map(|entry| entry.rel.strip_suffix('i').map(str::to_string))
        .collect();
    let pipe = Pipeline {
        rules: &rules,
        custom: &custom,
        config: &config,
        root: &root,
        filter: &opts.rule_filter,
        test_globs: &test_globs,
        cache: &loaded,
        interfaces: &interfaces,
        active_rules: OnceLock::new(),
    };
    let outcomes = analyze_all(&discovery.entries, &pipe, &pool)?;
    let (analyzed, skipped_outcomes) = split_outcomes(outcomes, &scopes);
    let skipped = count(
        discovery
            .skipped
            .iter()
            .filter(|rel| scopes.contains(rel.as_path()))
            .count(),
    ) + skipped_outcomes;
    let files = count(
        analyzed
            .iter()
            .filter(|file| scopes.contains(Path::new(&file.rel)))
            .count(),
    );
    if let Some(cache) = &cache {
        store_if_touched(cache, &analyzed, &loaded);
    }
    let mut all = merge(&pipe, analyzed, &pool)?;
    all.retain(|finding| scopes.contains(Path::new(&*finding.path)));
    let (reported, baselined, stale) = if opts.write_baseline {
        (all, 0, 0)
    } else {
        apply_gate(&pipe, &scopes, all)?
    };
    let baseline_written = if opts.write_baseline {
        let path = project::resolve_within_root(&root, &config.project.baseline)?;
        record_baseline(&path, &reported)?;
        Some(config.project.baseline.clone())
    } else {
        None
    };
    let deny = count(
        reported
            .iter()
            .filter(|finding| finding.severity == Severity::Deny)
            .count(),
    );
    let warn = count(
        reported
            .iter()
            .filter(|finding| finding.severity == Severity::Warn)
            .count(),
    );
    let mut rules_fired: BTreeMap<String, String> = BTreeMap::new();
    for finding in &reported {
        if !rules_fired.contains_key(&*finding.rule) {
            let why = rules.why(&finding.rule).unwrap_or_default();
            rules_fired.insert(finding.rule.to_string(), why.to_string());
        }
    }
    let failed = !opts.write_baseline
        && threshold(config.project.fail_on)
            .is_some_and(|minimum| reported.iter().any(|finding| finding.severity >= minimum));
    let summary = Summary {
        strictness: config.project.strictness,
        files,
        findings: count(reported.len()),
        deny,
        warn,
        baselined,
        stale,
        skipped,
    };
    Ok(Execution {
        report: Report {
            findings: reported,
            rules_fired,
            summary,
        },
        root,
        baseline_written,
        failed,
        max_file_bytes: config.project.max_file_bytes,
        pool,
    })
}

struct ActiveRules {
    ids: HashSet<String>,
    families: HashSet<String>,
}

fn build_active_rules(rules: &RuleSet, config: &Config, filter: &RuleFilter) -> ActiveRules {
    let mut ids = HashSet::with_capacity(CATALOG.len() + config.custom_rules.len());
    let mut families = HashSet::with_capacity(CATALOG.len() + config.custom_rules.len());
    for id in CATALOG
        .iter()
        .map(|info| info.id)
        .chain(config.custom_rules.iter().map(|spec| spec.id.as_str()))
    {
        if rules.is_active(id) && filter.matches(id) {
            for (at, _) in id.match_indices('/') {
                families.insert(id[..at].to_string());
            }
            ids.insert(id.to_string());
        }
    }
    ActiveRules { ids, families }
}

struct Pipeline<'a> {
    rules: &'a RuleSet,
    custom: &'a CustomRules,
    config: &'a Config,
    root: &'a Path,
    filter: &'a RuleFilter,
    test_globs: &'a GlobSet,
    cache: &'a Loaded,
    interfaces: &'a HashSet<String>,
    active_rules: OnceLock<ActiveRules>,
}

impl Pipeline<'_> {
    fn active_rules(&self) -> &ActiveRules {
        self.active_rules
            .get_or_init(|| build_active_rules(self.rules, self.config, self.filter))
    }

    fn is_operand_active(&self, pattern: &RulePattern) -> bool {
        let active = self.active_rules();
        match pattern {
            RulePattern::Exact(id) => active.ids.contains(&**id),
            RulePattern::Family(family) => active.families.contains(&**family),
        }
    }

    fn reports_unused(&self) -> bool {
        self.rules.is_active("meta/unused-suppression")
            && self.filter.matches("meta/unused-suppression")
    }
}

enum Outcome {
    Skipped(String),
    Analyzed(Box<Analyzed>),
}

struct Analyzed {
    rel: String,
    output: FileOutput,
    from_cache: bool,
    source: Option<String>,
}

struct Probe<'a> {
    text: &'a str,
    hash: [u8; 32],
}

fn analyze_all(
    entries: &[Entry],
    pipe: &Pipeline<'_>,
    pool: &rayon::ThreadPool,
) -> Result<Vec<Outcome>, RunError> {
    let results: Vec<Result<Outcome, RunError>> = pool.install(|| {
        entries
            .par_iter()
            .map_init(
                || Parsers::with_ocaml_interfaces(pipe.interfaces),
                |parsers, entry| process(parsers, entry, pipe),
            )
            .collect()
    });
    results.into_iter().collect()
}

fn process(
    parsers: &mut Parsers<'_>,
    entry: &Entry,
    pipe: &Pipeline<'_>,
) -> Result<Outcome, RunError> {
    let skipped = || Ok(Outcome::Skipped(entry.rel.clone()));
    let bytes = match read_within_limit(&entry.abs, pipe.config.project.max_file_bytes) {
        Err(source) if source.kind() == io::ErrorKind::NotFound => return skipped(),
        Err(source) => return Err(io_error(&entry.abs, source)),
        Ok(None) => return skipped(),
        Ok(Some(bytes)) => bytes,
    };
    let Ok(mut owned) = String::from_utf8(bytes) else {
        return skipped();
    };
    if is_generated(owned.as_bytes()) {
        return skipped();
    }
    if owned.starts_with('\u{feff}') {
        owned.drain(..'\u{feff}'.len_utf8());
    }
    let interface = entry.lang == Lang::Ocaml && pipe.interfaces.contains(entry.rel.as_str());
    let hash = if interface {
        cache::interface_hash(owned.as_str())
    } else {
        cache::source_hash(owned.as_str())
    };
    let (output, from_cache) = {
        let probe = Probe {
            text: owned.as_str(),
            hash,
        };
        match pipe.cache.take(entry.rel.as_str(), &probe.hash) {
            Some(output) => (output, true),
            None => (analyze_file(parsers, entry, &probe, pipe), false),
        }
    };
    let source = (pipe.reports_unused() && !output.suppressions.is_empty()).then_some(owned);
    Ok(Outcome::Analyzed(Box::new(Analyzed {
        rel: entry.rel.clone(),
        output,
        from_cache,
        source,
    })))
}

fn analyze_file(
    parsers: &mut Parsers<'_>,
    entry: &Entry,
    probe: &Probe<'_>,
    pipe: &Pipeline<'_>,
) -> FileOutput {
    let input = FileInput {
        lang: entry.lang,
        path: entry.rel.as_str(),
        source: probe.text,
        is_test_file: pipe.test_globs.is_match(entry.rel.as_str()),
    };
    let analysis = analyze(parsers, &input, pipe.custom);
    let world = WorldFacts::from_facts(&analysis.facts);
    let (suppressions, mut meta) =
        suppress::parse_suppressions(&entry.rel, probe.text, &analysis.comments, &|id: &str| {
            pipe.rules.is_known(id)
        });
    let mut findings: Vec<Finding> = match analysis.parse_error {
        Some(pos) => pipe
            .rules
            .parse_error(&entry.rel, probe.text, pos)
            .into_iter()
            .collect(),
        None => {
            let mut found = pipe.rules.check_file(&analysis.facts);
            found.extend(
                pipe.rules
                    .custom_findings(&analysis.facts, &analysis.custom_hits),
            );
            found
        }
    };
    pipe.rules.cap_levels(&mut meta);
    meta.retain(|finding| pipe.rules.is_active(&finding.rule));
    findings.append(&mut meta);
    FileOutput {
        hash: probe.hash,
        parsed: analysis.parse_error.is_none(),
        findings,
        suppressions,
        world,
    }
}

fn merge(
    pipe: &Pipeline<'_>,
    mut analyzed: Vec<Analyzed>,
    pool: &rayon::ThreadPool,
) -> Result<Vec<Finding>, RunError> {
    let mut orphans = Vec::new();
    for finding in pool.install(|| world_findings(pipe, &analyzed)) {
        match analyzed.binary_search_by(|file| file.rel.as_str().cmp(&*finding.path)) {
            Ok(index) => analyzed[index].output.findings.push(finding),
            Err(_) => orphans.push(finding),
        }
    }
    let orphans = fingerprinted(orphans);
    let capacity: usize = analyzed
        .iter()
        .map(|file| file.output.findings.len() + file.output.suppressions.len())
        .sum::<usize>()
        + orphans.len();
    let finalized: Vec<Result<Vec<Finding>, RunError>> = pool.install(|| {
        analyzed
            .into_par_iter()
            .map(|file| finalize(pipe, file))
            .collect()
    });
    let mut all = Vec::with_capacity(capacity);
    for findings in finalized {
        all.extend(findings?);
    }
    all.extend(orphans);
    all.retain(|finding| pipe.filter.matches(&finding.rule));
    pool.install(|| all.par_sort_by(report_order));
    Ok(all)
}

fn report_order(a: &Finding, b: &Finding) -> std::cmp::Ordering {
    (&a.path, a.line, a.column, &a.rule, &a.message)
        .cmp(&(&b.path, b.line, b.column, &b.rule, &b.message))
}

fn fingerprinted(mut findings: Vec<Finding>) -> Vec<Finding> {
    findings.sort_by(report_order);
    Baseline::assign_fingerprints(&mut findings);
    findings
}

fn split_outcomes(outcomes: Vec<Outcome>, scopes: &project::Scopes) -> (Vec<Analyzed>, u32) {
    let mut analyzed = Vec::with_capacity(outcomes.len());
    let mut skipped = 0;
    for outcome in outcomes {
        match outcome {
            Outcome::Skipped(rel) => skipped += u32::from(scopes.contains(Path::new(&rel))),
            Outcome::Analyzed(file) => analyzed.push(*file),
        }
    }
    (analyzed, skipped)
}

fn store_if_touched(cache: &cache::Cache, analyzed: &[Analyzed], loaded: &Loaded) {
    let touched =
        analyzed.len() != loaded.entry_count() || analyzed.iter().any(|file| !file.from_cache);
    if touched {
        let view: Vec<(&str, &FileOutput)> = analyzed
            .iter()
            .map(|file| (file.rel.as_str(), &file.output))
            .collect();
        cache.store(&view, loaded);
    }
}

fn world_findings(pipe: &Pipeline<'_>, analyzed: &[Analyzed]) -> Vec<Finding> {
    let files: Vec<WorldFile<'_>> = analyzed
        .iter()
        .map(|file| WorldFile {
            path: file.rel.as_str(),
            facts: &file.output.world,
        })
        .collect();
    pipe.rules.check_world(&files)
}

fn finalize(pipe: &Pipeline<'_>, file: Analyzed) -> Result<Vec<Finding>, RunError> {
    let FileOutput {
        mut findings,
        mut suppressions,
        parsed,
        ..
    } = file.output;
    suppress::apply(&mut findings, &mut suppressions);
    if parsed && pipe.reports_unused() {
        findings.extend(unused_findings(
            pipe,
            &file.rel,
            &suppressions,
            file.source.as_deref(),
        )?);
    }
    Ok(fingerprinted(findings))
}

fn unused_findings(
    pipe: &Pipeline<'_>,
    rel: &str,
    suppressions: &[Suppression],
    source: Option<&str>,
) -> Result<Vec<Finding>, RunError> {
    let candidates: Vec<(&Suppression, Vec<&RulePattern>)> = suppressions
        .iter()
        .filter_map(|suppression| {
            let unused_active: Vec<&RulePattern> = suppression
                .rules
                .iter()
                .zip(&suppression.used)
                .filter(|(pattern, used)| !**used && pipe.is_operand_active(pattern))
                .map(|(pattern, _)| pattern)
                .collect();
            (!unused_active.is_empty()).then_some((suppression, unused_active))
        })
        .collect();
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let fallback;
    let text = match source {
        Some(text) => text,
        None => {
            fallback = read_source(&pipe.root.join(rel), pipe.config.project.max_file_bytes)?;
            &fallback
        }
    };
    Ok(candidates
        .into_iter()
        .filter_map(|(suppression, unused_patterns)| {
            let patterns = describe(&unused_patterns);
            pipe.rules
                .unused_suppression(rel, text, suppression.start_line, &patterns)
        })
        .collect())
}

fn apply_gate(
    pipe: &Pipeline<'_>,
    scopes: &project::Scopes,
    all: Vec<Finding>,
) -> Result<(Vec<Finding>, u32, u32), RunError> {
    if pipe.config.project.mode != Mode::Ratchet {
        return Ok((all, 0, 0));
    }
    let mut baseline = load_baseline(pipe.root, &pipe.config.project.baseline)?;
    baseline
        .entries
        .retain(|entry| pipe.filter.matches(&entry.rule));
    Ok(baseline.filter(all, &|path: &str| scopes.contains(Path::new(path))))
}

fn is_generated(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(GENERATED_WINDOW)]);
    head.contains("@generated") || (head.contains("Code generated") && head.contains("DO NOT EDIT"))
}

fn describe(patterns: &[&RulePattern]) -> String {
    patterns
        .iter()
        .map(|pattern| match pattern {
            RulePattern::Exact(id) => id.to_string(),
            RulePattern::Family(family) => format!("{family}/*"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn threshold(fail_on: FailOn) -> Option<Severity> {
    match fail_on {
        FailOn::Warn => Some(Severity::Warn),
        FailOn::Deny => Some(Severity::Deny),
        FailOn::Never => None,
    }
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

fn io_error(path: &Path, source: io::Error) -> RunError {
    RunError::Io {
        path: path.to_path_buf(),
        source,
    }
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
