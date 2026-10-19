//! CLI pipeline: discovery, caching, rule evaluation, merge, and the ratchet gate.

pub mod cli;
pub mod init;
pub mod rules_cmd;

mod cache;
mod discover;
mod project;
mod terminal;

pub use terminal::TerminalText;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

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

use cache::FileOutput;
use discover::Entry;
use project::Overrides;

const GENERATED_WINDOW: usize = 2048;

/// Inputs to one lint run; `Some` fields override the configuration file.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Directory that relative PATHS resolve against.
    pub cwd: PathBuf,
    /// Explicit project root, which skips discovery.
    pub root: Option<PathBuf>,
    /// Explicit configuration file.
    pub config: Option<PathBuf>,
    /// Paths whose findings are reported; empty means the whole root.
    pub paths: Vec<PathBuf>,
    /// Strictness override.
    pub strictness: Option<Strictness>,
    /// Gate mode override.
    pub mode: Option<Mode>,
    /// Fail-on override.
    pub fail_on: Option<FailOn>,
    /// Report only matching rules; an empty filter matches all.
    pub rule_filter: RuleFilter,
    /// Read and write `.sloplint/cache-v1.bin`.
    pub use_cache: bool,
    /// Worker threads; zero means the available parallelism.
    pub jobs: usize,
    /// Record the findings as the baseline instead of gating on them.
    pub write_baseline: bool,
}

/// Why a run could not complete.
#[derive(Debug, Error)]
pub enum RunError {
    /// The configuration or a rule option is invalid.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// A file or directory could not be read or listed.
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    /// A PATH resolves outside the project root.
    #[error("path is outside the project root: {}", path.display())]
    PathOutsideRoot { path: PathBuf },
    /// A PATH does not exist.
    #[error("path not found: {}", path.display())]
    PathNotFound { path: PathBuf },
    /// Ratchet mode found no baseline file.
    #[error("baseline file not found: {}; run 'sloplint baseline'", path.display())]
    BaselineMissing { path: PathBuf },
    /// A baseline path resolves outside the project root.
    #[error("baseline path is outside the project root: {}", path.display())]
    BaselineOutsideRoot { path: PathBuf },
    /// The baseline file is not a valid version-1 baseline.
    #[error("baseline {}: {message}", path.display())]
    BaselineCorrupt { path: PathBuf, message: String },
    /// `init` would overwrite an existing configuration.
    #[error("{} already exists", path.display())]
    ConfigExists { path: PathBuf },
}

impl RunError {
    /// Process exit code: 3 for I/O that stops the run, 2 for usage and configuration errors.
    pub fn exit_code(&self) -> u8 {
        match self {
            RunError::Io { .. } => 3,
            _ => 2,
        }
    }
}

/// Counts of a run, as shown on the summary line.
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    /// Effective strictness profile.
    pub strictness: Strictness,
    /// Analyzed files inside the PATH scope.
    pub files: u32,
    /// Reported findings.
    pub findings: u32,
    /// Reported deny findings.
    pub deny: u32,
    /// Reported warn findings.
    pub warn: u32,
    /// Findings matched by the baseline and hidden.
    pub baselined: u32,
    /// Baseline entries in scope that matched nothing.
    pub stale: u32,
    /// Files in scope skipped by policy: over `max_file_bytes`, generated, not UTF-8 (content or name), or not regular files (symlinks are never followed, FIFOs and devices never opened). Read errors fail the run instead.
    pub skipped: u32,
}

/// The reported findings and their counts.
#[derive(Debug, Clone)]
pub struct Report {
    /// Reported findings, sorted by path, line, column, rule, and message.
    pub findings: Vec<Finding>,
    /// Rule id to rationale for every rule with a reported finding.
    pub rules_fired: BTreeMap<String, String>,
    /// Counts for the summary line.
    pub summary: Summary,
}

/// A finished run: the report plus what the CLI needs to render and exit.
#[derive(Debug, Clone)]
pub struct Execution {
    /// The report shown to the user.
    pub report: Report,
    /// Canonical project root, used to re-read source lines for rendering.
    pub root: PathBuf,
    /// Configured baseline path when `write_baseline` wrote one.
    pub baseline_written: Option<String>,
    /// Whether a reported finding reached `fail_on`.
    pub failed: bool,
    /// Effective `max_file_bytes` limit for re-reading source lines during rendering.
    ///
    /// Carried from the active configuration so the human renderer reuses the
    /// same bound without reloading configuration or using a default.
    pub max_file_bytes: u64,
}

/// Lints the project root and returns the reported findings.
pub fn run(opts: &RunOptions) -> Result<Report, RunError> {
    execute(opts).map(|execution| execution.report)
}

/// The PATH scope for a run given no PATHS: `cwd` when it lies strictly inside the project root, else empty (the whole root).
///
/// Resolves the root exactly as [`execute`] does (same `root` and `config`
/// handling) but loads no configuration and discovers no files, so a command
/// line without PATHS can scope to the working directory without failing when
/// `--config` places the root elsewhere.
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

/// Lints the project root and returns the report with its render and exit state.
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
    let discovery = discover::discover(&root, &config)?;
    let key = cache::config_key(&config, &loaded.custom_sources)
        .map_err(|source| io_error(&root, source))?;
    let cached = if opts.use_cache {
        cache::load(&root, &key)
    } else {
        HashMap::new()
    };
    let test_globs = project::glob_set(&config.project.test_globs, "project.test_globs")?;
    let interfaces: HashSet<String> = discovery
        .entries
        .iter()
        .filter(|entry| entry.lang == Lang::OcamlInterface)
        .map(|entry| entry.rel.clone())
        .collect();
    let pipe = Pipeline {
        rules: &rules,
        custom: &custom,
        config: &config,
        root: &root,
        filter: &opts.rule_filter,
        test_globs: &test_globs,
        cache: &cached,
        interfaces: &interfaces,
    };
    let outcomes = analyze_all(&discovery.entries, &pipe, jobs(opts.jobs))?;
    let mut skipped = count(
        discovery
            .skipped
            .iter()
            .filter(|rel| scopes.contains(rel.as_path()))
            .count(),
    );
    let mut analyzed = Vec::new();
    for outcome in outcomes {
        match outcome {
            Outcome::Skipped(rel) => {
                if scopes.contains(Path::new(&rel)) {
                    skipped += 1;
                }
            }
            Outcome::Analyzed(file) => analyzed.push(file),
        }
    }
    let files = count(
        analyzed
            .iter()
            .filter(|file| scopes.contains(Path::new(&file.rel)))
            .count(),
    );
    if opts.use_cache {
        let view: Vec<(&str, &FileOutput)> = analyzed
            .iter()
            .map(|file| (file.rel.as_str(), &file.output))
            .collect();
        if let Err(error) = cache::store(&root, &key, &view) {
            let _ = writeln!(
                io::stderr(),
                "sloplint: warning: cache not written in {}: {}",
                TerminalText::line(&root.to_string_lossy()),
                TerminalText::line(&error.to_string())
            );
        }
    }
    let mut all = merge(&pipe, analyzed)?;
    all.retain(|finding| scopes.contains(Path::new(&*finding.path)));
    let (reported, baselined, stale) = if opts.write_baseline {
        (all, 0, 0)
    } else {
        apply_gate(&pipe, &scopes, all)?
    };
    let baseline_written = if opts.write_baseline {
        let path = project::resolve_within_root(&root, &config.project.baseline)?;
        write_replacing(
            &path,
            Baseline::from_findings(&reported).to_json().as_bytes(),
        )
        .map_err(|source| io_error(&path, source))?;
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
    let rules_fired: BTreeMap<String, String> = reported
        .iter()
        .map(|finding| {
            let why = rules.why(&finding.rule).unwrap_or_default();
            (finding.rule.to_string(), why.to_string())
        })
        .collect();
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
    })
}

struct Pipeline<'a> {
    rules: &'a RuleSet,
    custom: &'a CustomRules,
    config: &'a Config,
    root: &'a Path,
    filter: &'a RuleFilter,
    test_globs: &'a GlobSet,
    cache: &'a HashMap<Box<str>, FileOutput>,
    interfaces: &'a HashSet<String>,
}

enum Outcome {
    Skipped(String),
    Analyzed(Analyzed),
}

struct Analyzed {
    rel: String,
    output: FileOutput,
}

struct Probe<'a> {
    text: &'a str,
    hash: [u8; 32],
    interface: bool,
}

fn analyze_all(
    entries: &[Entry],
    pipe: &Pipeline<'_>,
    jobs: usize,
) -> Result<Vec<Outcome>, RunError> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build()
        .map_err(|error| io_error(pipe.root, io::Error::other(error)))?;
    let results: Vec<Result<Outcome, RunError>> = pool.install(|| {
        entries
            .par_iter()
            .map_init(Parsers::new, |parsers, entry| process(parsers, entry, pipe))
            .collect()
    });
    results.into_iter().collect()
}

fn process(parsers: &mut Parsers, entry: &Entry, pipe: &Pipeline<'_>) -> Result<Outcome, RunError> {
    let skipped = || Ok(Outcome::Skipped(entry.rel.clone()));
    let read = read_within_limit(&entry.abs, pipe.config.project.max_file_bytes);
    let Some(bytes) = read.map_err(|source| io_error(&entry.abs, source))? else {
        return skipped();
    };
    let Ok(raw) = std::str::from_utf8(&bytes) else {
        return skipped();
    };
    if is_generated(&bytes) {
        return skipped();
    }
    let text = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let interface =
        entry.lang == Lang::Ocaml && pipe.interfaces.contains(&format!("{}i", entry.rel));
    let hash = if interface {
        cache::interface_hash(text)
    } else {
        cache::source_hash(text)
    };
    let probe = Probe {
        text,
        hash,
        interface,
    };
    let output = match pipe.cache.get(entry.rel.as_str()) {
        Some(cached) if cached.hash == probe.hash => cached.clone(),
        _ => analyze_file(parsers, entry, &probe, pipe),
    };
    Ok(Outcome::Analyzed(Analyzed {
        rel: entry.rel.clone(),
        output,
    }))
}

fn analyze_file(
    parsers: &mut Parsers,
    entry: &Entry,
    probe: &Probe<'_>,
    pipe: &Pipeline<'_>,
) -> FileOutput {
    let input = FileInput {
        lang: entry.lang,
        path: entry.rel.as_str(),
        source: probe.text,
        is_test_file: pipe.test_globs.is_match(entry.rel.as_str()),
        has_interface: probe.interface,
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

fn merge(pipe: &Pipeline<'_>, mut analyzed: Vec<Analyzed>) -> Result<Vec<Finding>, RunError> {
    let world = world_findings(pipe, &analyzed);
    let mut orphans = Vec::new();
    for finding in world {
        match analyzed.binary_search_by(|file| file.rel.as_str().cmp(&*finding.path)) {
            Ok(index) => analyzed[index].output.findings.push(finding),
            Err(_) => orphans.push(finding),
        }
    }
    let mut all = Vec::new();
    for file in analyzed {
        all.extend(finalize(pipe, file)?);
    }
    all.extend(orphans);
    all.retain(|finding| pipe.filter.matches(&finding.rule));
    all.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.column.cmp(&b.column))
            .then_with(|| a.rule.cmp(&b.rule))
            .then_with(|| a.message.cmp(&b.message))
    });
    Ok(all)
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
    findings.sort_by(|a, b| {
        (a.line, a.column, &a.rule, &a.message).cmp(&(b.line, b.column, &b.rule, &b.message))
    });
    Baseline::assign_fingerprints(&mut findings);
    suppress::apply(&mut findings, &mut suppressions);
    if parsed
        && (pipe.filter.0.is_empty()
            || pipe
                .filter
                .0
                .iter()
                .any(|p| p.matches("meta/unused-suppression")))
    {
        let mut unused = unused_findings(pipe, &file.rel, &suppressions)?;
        unused.sort_by(|a, b| {
            (a.line, a.column, &a.rule, &a.message).cmp(&(b.line, b.column, &b.rule, &b.message))
        });
        Baseline::assign_fingerprints(&mut unused);
        findings.extend(unused);
    }
    Ok(findings)
}

fn unused_findings(
    pipe: &Pipeline<'_>,
    rel: &str,
    suppressions: &[Suppression],
) -> Result<Vec<Finding>, RunError> {
    let unused: Vec<&Suppression> = suppressions
        .iter()
        .filter(|suppression| !suppression.used && suppression_active(pipe, suppression))
        .collect();
    if unused.is_empty() {
        return Ok(Vec::new());
    }
    let source = read_source(&pipe.root.join(rel), pipe.config.project.max_file_bytes)?;
    Ok(unused
        .into_iter()
        .filter_map(|suppression| {
            let patterns = describe(&suppression.rules);
            pipe.rules
                .unused_suppression(rel, &source, suppression.start_line, &patterns)
        })
        .collect())
}

fn suppression_active(pipe: &Pipeline<'_>, suppression: &Suppression) -> bool {
    suppression.rules.iter().all(|pattern| {
        CATALOG
            .iter()
            .map(|info| info.id)
            .chain(pipe.config.custom_rules.iter().map(|spec| spec.id.as_str()))
            .any(|id| pattern.matches(id) && pipe.rules.is_active(id))
    })
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

fn load_baseline(root: &Path, name: &str) -> Result<Baseline, RunError> {
    let path = project::resolve_within_root(root, name)?;
    let text = match project::read_regular_string(&path) {
        Ok(text) => text,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Err(RunError::BaselineMissing { path });
        }
        Err(source) => return Err(RunError::Io { path, source }),
    };
    Baseline::from_json(&text).map_err(|message| RunError::BaselineCorrupt { path, message })
}

fn read_within_limit(path: &Path, max_bytes: u64) -> io::Result<Option<Vec<u8>>> {
    let (file, len) = project::open_regular(path)?;
    if len > max_bytes {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok((u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= max_bytes).then_some(bytes))
}

/// Reads a whole regular file as UTF-8 text, bounded by `max_bytes`.
///
/// Opens the path without blocking and refuses FIFOs and devices;
/// returns an error when the file grows past the bound or is not valid UTF-8.
/// A leading byte-order mark is removed in place when present.
pub fn read_source(path: &Path, max_bytes: u64) -> Result<String, RunError> {
    let invalid =
        |message: &'static str| io_error(path, io::Error::new(io::ErrorKind::InvalidData, message));
    let bytes = read_within_limit(path, max_bytes)
        .map_err(|source| io_error(path, source))?
        .ok_or_else(|| invalid("file grew past max_file_bytes while being linted"))?;
    let mut text =
        String::from_utf8(bytes).map_err(|_| invalid("stream did not contain valid UTF-8"))?;
    if text.starts_with('\u{feff}') {
        text.drain(..'\u{feff}'.len_utf8());
    }
    Ok(text)
}

/// Replaces `path` with `bytes` so no reader or crash ever sees a partial file.
///
/// The bytes go to an exclusively created sibling file that is renamed over
/// `path`; on any failure the sibling is removed and `path` is untouched. An
/// existing target must be a regular file writable by this user (a FIFO or
/// device is refused without blocking) and keeps its permissions. The
/// name is replaced, not followed, so a symlink swapped in after
/// `resolve_within_root` cannot redirect the write.
fn write_replacing(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let existing = fs::metadata(path).map(Some).or_else(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(error)
        }
    })?;
    if let Some(meta) = &existing {
        if !meta.is_file() && !meta.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a regular file",
            ));
        }
        probe_writable(path)?;
    }
    let permissions = existing.map(|meta| meta.permissions());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.{}.{stamp}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    let written = file
        .write_all(bytes)
        .and_then(|()| match permissions {
            Some(permissions) => file.set_permissions(permissions),
            None => Ok(()),
        })
        .and_then(|()| file.sync_all());
    drop(file);
    written
        .and_then(|()| fs::rename(&tmp, path))
        .inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
}

/// Probes that an existing baseline target is writable without blocking on a FIFO.
///
/// Unix opens the path nonblocking with no controlling terminal, so a FIFO swapped in
/// after the metadata check fails at once instead of waiting for a reader; the opened
/// handle is then refused unless it is a regular file. A read-only regular file still
/// fails here, preserving the read-only rejection before any temporary file is made.
#[cfg(unix)]
fn probe_writable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn probe_writable(path: &Path) -> io::Result<()> {
    fs::OpenOptions::new().write(true).open(path).map(|_| ())
}

fn is_generated(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(GENERATED_WINDOW)]);
    head.contains("@generated") || (head.contains("Code generated") && head.contains("DO NOT EDIT"))
}

fn describe(patterns: &[RulePattern]) -> String {
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

fn jobs(requested: usize) -> usize {
    if requested == 0 {
        std::thread::available_parallelism().map_or(1, NonZeroUsize::get)
    } else {
        requested
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
