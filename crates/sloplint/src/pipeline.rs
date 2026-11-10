use std::collections::HashSet;
use std::io;
use std::path::Path;
use std::sync::OnceLock;

use globset::GlobSet;
use rayon::prelude::*;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

use sloplint_core::baseline::Baseline;
use sloplint_core::facts::WorldFacts;
use sloplint_core::finding::{SourceText, line_starts};
use sloplint_core::lang::Lang;
use sloplint_core::rules::{CATALOG, RuleFilter, RuleSet, WorldFile};
use sloplint_core::suppress::{self, RulePattern, Suppression};
use sloplint_core::{Config, Finding};

use crate::RunError;
use crate::cache::{self, Cache, FileOutput, Loaded};
use crate::discover::Entry;
use crate::io_error;
use crate::project;
use crate::store::{read_source, read_within_limit};

const GENERATED_WINDOW: usize = 2048;

pub(crate) struct Pipeline<'a> {
    pub(crate) rules: &'a RuleSet,
    pub(crate) custom: &'a CustomRules,
    pub(crate) config: &'a Config,
    pub(crate) root: &'a Path,
    pub(crate) filter: &'a RuleFilter,
    pub(crate) test_globs: &'a GlobSet,
    pub(crate) cache: &'a Loaded,
    pub(crate) interfaces: &'a HashSet<String>,
    pub(crate) active_rules: OnceLock<ActiveRules>,
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

pub(crate) struct ActiveRules {
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

enum Outcome {
    Skipped(String),
    Analyzed(Box<Analyzed>),
}

pub(crate) struct Analyzed {
    rel: String,
    output: FileOutput,
    from_cache: bool,
    source: Option<String>,
}

struct Probe<'a> {
    text: &'a str,
    hash: [u8; 32],
}

pub(crate) fn run(
    entries: &[Entry],
    pipe: &Pipeline<'_>,
    pool: &rayon::ThreadPool,
    scopes: &project::Scopes,
) -> Result<(Vec<Analyzed>, u32), RunError> {
    let outcomes = analyze_all(entries, pipe, pool)?;
    Ok(split_outcomes(outcomes, scopes))
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
    let text = SourceText {
        path: entry.rel.as_str(),
        source: probe.text,
        line_starts: &analysis.facts.line_starts,
    };
    let (suppressions, mut meta) =
        suppress::parse_suppressions(&text, &analysis.comments, &|id: &str| {
            pipe.rules.is_known(id)
        });
    let mut findings: Vec<Finding> = match analysis.parse_error {
        Some(pos) => pipe.rules.parse_error(&text, pos).into_iter().collect(),
        None => rule_findings(pipe, &analysis),
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

fn rule_findings(pipe: &Pipeline<'_>, analysis: &Analysis<'_>) -> Vec<Finding> {
    let mut found = pipe.rules.check_file(&analysis.facts);
    found.extend(
        pipe.rules
            .custom_findings(&analysis.facts, &analysis.custom_hits),
    );
    found.extend(
        pipe.rules
            .custom_caps(&analysis.facts, &analysis.custom_capped),
    );
    found
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

pub(crate) fn count_in_scope(analyzed: &[Analyzed], scopes: &project::Scopes) -> u32 {
    crate::count(
        analyzed
            .iter()
            .filter(|file| scopes.contains(Path::new(&file.rel)))
            .count(),
    )
}

pub(crate) fn store_if_touched(cache: &Cache, analyzed: &[Analyzed], loaded: &Loaded) {
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

pub(crate) fn merge(
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
    let file = SourceText {
        path: rel,
        source: text,
        line_starts: &line_starts(text),
    };
    Ok(candidates
        .into_iter()
        .filter_map(|(suppression, unused_patterns)| {
            let patterns = describe(&unused_patterns);
            pipe.rules
                .unused_suppression(&file, suppression.start_line, &patterns)
        })
        .collect())
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
