use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::unique_patch_lines::{is_comment_line, is_strong_line, is_substantive, normalize_line};
use crate::config::Strictness;
use crate::facts::{FileFacts, Function, Pos};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift, role_at};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, parse_options};

const MIN_STRONG_LINES: usize = 3;
const FIX: &str =
    "fold the repeated block into a loop over a table or one helper so each edit site is unique";

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/unique-patch-anchors",
    family: Family::Agent,
    summary: "identical multi-line blocks repeated across a file",
    why: "Repeated identical blocks make search-and-replace edits ambiguous.",
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    window: u32,
    min_repeats: u32,
}

struct UniquePatchAnchors {
    window: usize,
    min_repeats: usize,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, window) = match strictness {
        Strictness::Minimal | Strictness::Basic | Strictness::Standard => (Level::Off, 8),
        Strictness::Strict => (Level::At(Severity::Info), 6),
    };
    let mut options = toml::Table::new();
    options.insert(
        "window".to_string(),
        toml::Value::Integer(i64::from(window)),
    );
    options.insert("min_repeats".to_string(), toml::Value::Integer(3));
    Preset { level, options }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let parsed: Options = parse_options(options)?;
    if parsed.window < 2 {
        return Err(format!("window {} must be at least 2", parsed.window));
    }
    if parsed.min_repeats < 2 {
        return Err(format!(
            "min_repeats {} must be at least 2",
            parsed.min_repeats
        ));
    }
    Ok(Box::new(UniquePatchAnchors {
        window: usize::try_from(parsed.window).map_err(|error| error.to_string())?,
        min_repeats: usize::try_from(parsed.min_repeats).map_err(|error| error.to_string())?,
    }))
}

impl FileRule for UniquePatchAnchors {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        let segments = kept_segments(facts.source);
        for run in self.repeated_runs(&segments) {
            self.report(facts, &segments, &run, out);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Occ {
    seg: usize,
    at: usize,
}

struct Entry<'s> {
    line: u32,
    text: Cow<'s, str>,
    strong: bool,
    substantive: bool,
}

enum Starts {
    One(Occ),
    Many(Vec<Occ>),
}

impl Starts {
    fn push(&mut self, occ: Occ) {
        match self {
            Starts::Many(occs) => occs.push(occ),
            Starts::One(first) => {
                let first = *first;
                *self = Starts::Many(vec![first, occ]);
            }
        }
    }
}

struct Run {
    starts: Vec<Occ>,
    span: usize,
}

impl UniquePatchAnchors {
    fn repeated_windows(&self, segments: &[Vec<Entry<'_>>]) -> HashMap<Vec<usize>, Starts> {
        let mut interned: HashMap<&str, usize> = HashMap::new();
        let mut groups: HashMap<Vec<usize>, Starts> = HashMap::new();
        for (seg, entries) in segments.iter().enumerate() {
            if entries.len() < self.window {
                continue;
            }
            let ids: Vec<usize> = entries
                .iter()
                .map(|entry| {
                    let next = interned.len();
                    *interned.entry(entry.text.as_ref()).or_insert(next)
                })
                .collect();
            let mut strong = entries[..self.window]
                .iter()
                .filter(|entry| entry.strong)
                .count();
            let mut kept = entries[..self.window]
                .iter()
                .filter(|entry| entry.substantive)
                .count();
            for at in 0..=entries.len() - self.window {
                if at > 0 {
                    strong += usize::from(entries[at + self.window - 1].strong);
                    strong -= usize::from(entries[at - 1].strong);
                    kept += usize::from(entries[at + self.window - 1].substantive);
                    kept -= usize::from(entries[at - 1].substantive);
                }
                if strong < MIN_STRONG_LINES || kept == 0 {
                    continue;
                }
                groups
                    .entry(ids[at..at + self.window].to_vec())
                    .and_modify(|starts| starts.push(Occ { seg, at }))
                    .or_insert(Starts::One(Occ { seg, at }));
            }
        }
        groups
    }

    fn repeated_runs(&self, segments: &[Vec<Entry<'_>>]) -> Vec<Run> {
        let mut repeated: Vec<Vec<Occ>> = self
            .repeated_windows(segments)
            .into_values()
            .filter_map(|starts| match starts {
                Starts::Many(starts) if starts.len() >= self.min_repeats => Some(starts),
                _ => None,
            })
            .collect();
        for group in &mut repeated {
            group.sort_unstable();
        }
        repeated.sort_unstable_by_key(|starts| starts[0]);
        let mut runs = Vec::new();
        for (index, group) in repeated.iter().enumerate() {
            if continues_run(&repeated, index) {
                continue;
            }
            let mut lengths = vec![group.len()];
            let mut current = index;
            while let Some(next) = group_starting_at(&repeated, shifted(repeated[current][0]))
                .filter(|&next| slides_into(&repeated[current], &repeated[next]))
            {
                lengths.push(repeated[next].len());
                current = next;
            }
            let first = group;
            if let Some(run) = (1..=lengths.len()).rev().find_map(|windows| {
                let span = self.window + windows - 1;
                let survivors = &first[..lengths[windows - 1]];
                (non_overlapping(survivors, span).len() >= self.min_repeats).then(|| Run {
                    starts: survivors.to_vec(),
                    span,
                })
            }) {
                runs.push(run);
            }
        }
        runs
    }

    fn report(
        &self,
        facts: &FileFacts<'_>,
        segments: &[Vec<Entry<'_>>],
        run: &Run,
        out: &mut Vec<Finding>,
    ) {
        let starts: Vec<u32> = non_overlapping(&run.starts, run.span)
            .into_iter()
            .filter(|occ| {
                let first = segments[occ.seg][occ.at].line;
                let last = segments[occ.seg][occ.at + run.span - 1].line;
                body_contains(&facts.functions, first, last)
            })
            .map(|occ| segments[occ.seg][occ.at].line)
            .collect();
        if starts.len() < self.min_repeats {
            return;
        }
        let Some(least_lowered) = starts
            .iter()
            .copied()
            .min_by_key(|&line| Reverse(severity_at(facts, line)))
        else {
            return;
        };
        let lines = starts
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let message = format!(
            "{} identical {}-line blocks (lines {lines})",
            starts.len(),
            run.span
        );
        out.push(Finding::at(
            facts,
            Pos {
                line: least_lowered,
                byte: 0,
            },
            INFO.id,
            Severity::Minor,
            message,
            FIX.to_string(),
        ));
    }
}

fn severity_at(facts: &FileFacts<'_>, line: u32) -> Option<Severity> {
    INFO.roles
        .shift(role_at(facts.role, &facts.regions, line))
        .apply(Severity::Minor)
}

fn shifted(occ: Occ) -> Occ {
    Occ {
        seg: occ.seg,
        at: occ.at + 1,
    }
}

fn group_starting_at(repeated: &[Vec<Occ>], target: Occ) -> Option<usize> {
    repeated
        .binary_search_by_key(&target, |starts| starts[0])
        .ok()
}

fn slides_into(earlier: &[Occ], later: &[Occ]) -> bool {
    later.len() <= earlier.len()
        && earlier
            .iter()
            .zip(later)
            .all(|(before, after)| before.seg == after.seg && before.at + 1 == after.at)
}

fn continues_run(repeated: &[Vec<Occ>], index: usize) -> bool {
    let starts = &repeated[index];
    starts[0]
        .at
        .checked_sub(1)
        .and_then(|at| {
            group_starting_at(
                repeated,
                Occ {
                    seg: starts[0].seg,
                    at,
                },
            )
        })
        .is_some_and(|earlier| slides_into(&repeated[earlier], starts))
}

fn non_overlapping(starts: &[Occ], span: usize) -> Vec<Occ> {
    let mut accepted = Vec::new();
    let mut free_in = usize::MAX;
    let mut next_free = 0;
    for &occ in starts {
        if occ.seg != free_in {
            free_in = occ.seg;
            next_free = 0;
        }
        if occ.at < next_free {
            continue;
        }
        accepted.push(occ);
        next_free = occ.at + span;
    }
    accepted
}

fn body_contains(functions: &[Function<'_>], first: u32, last: u32) -> bool {
    functions
        .iter()
        .any(|function| function.pos.line <= first && last <= function.end_line)
}

fn kept_segments(source: &str) -> Vec<Vec<Entry<'_>>> {
    let mut segments = vec![Vec::new()];
    for (index, line) in source.lines().enumerate() {
        let normalized = normalize_line(line);
        if normalized.is_empty() {
            segments.push(Vec::new());
            continue;
        }
        if is_comment_line(&normalized) {
            continue;
        }
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let strong = is_strong_line(&normalized);
        let substantive = is_substantive(&normalized);
        if let Some(segment) = segments.last_mut() {
            segment.push(Entry {
                line: number,
                text: normalized,
                strong,
                substantive,
            });
        }
    }
    segments
}

#[cfg(test)]
#[path = "unique_patch_anchors_tests.rs"]
mod tests;
