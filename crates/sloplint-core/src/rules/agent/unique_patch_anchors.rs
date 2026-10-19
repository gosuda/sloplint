//! `agent/unique-patch-anchors`: repeated identical line blocks.

use std::borrow::Cow;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::{FileFacts, Pos};
use crate::finding::{Finding, Level, Severity};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

const MIN_STRONG_LINES: usize = 3;
const STRONG_LINE_CHARS: usize = 8;
const FIX: &str =
    "fold the repeated block into a loop over a table or one helper so each edit site is unique";

/// `agent/unique-patch-anchors`: flags identical line blocks that repeat too often.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/unique-patch-anchors",
    family: Family::Agent,
    summary: "identical multi-line blocks repeated across a file",
    why: "Repeated identical blocks make search-and-replace edits ambiguous.",
    exempt_tests: false,
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
        Strictness::Minimal | Strictness::Basic => (Level::Off, 8),
        Strictness::Standard => (Level::Warn, 8),
        Strictness::Strict => (Level::Warn, 6),
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
    let parsed: Options = toml::Value::Table(options)
        .try_into()
        .map_err(|error| error.to_string())?;
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
        let kept = kept_lines(facts.source);
        if kept.len() < self.window {
            return;
        }
        for starts in self.repeated_windows(&kept) {
            self.report(facts, &kept, &starts, out);
        }
    }
}

/// Window starts of one block: a lone start stays inline until the block repeats.
enum Starts {
    One(usize),
    Many(Vec<usize>),
}

impl Starts {
    fn push(&mut self, start: usize) {
        match self {
            Starts::Many(starts) => starts.push(start),
            Starts::One(first) => {
                let first = *first;
                *self = Starts::Many(vec![first, start]);
            }
        }
    }
}

impl UniquePatchAnchors {
    /// Start indices, ascending, of every strong window that occurs at least `min_repeats` times.
    ///
    /// Each distinct normalized line gets one id, and a window is keyed by its id slice. The map
    /// compares those slices on equality, so equal hashes never merge different blocks.
    fn repeated_windows(&self, kept: &[(u32, Cow<'_, str>)]) -> Vec<Vec<usize>> {
        let mut interned: HashMap<&str, usize> = HashMap::new();
        let mut line_ids: Vec<usize> = Vec::with_capacity(kept.len());
        let mut strong: Vec<bool> = Vec::with_capacity(kept.len());
        for (_, line) in kept {
            let next = interned.len();
            line_ids.push(*interned.entry(line).or_insert(next));
            strong.push(is_strong_line(line));
        }
        let mut groups: HashMap<&[usize], Starts> = HashMap::new();
        let mut strong_in_window = strong[..self.window].iter().filter(|&&flag| flag).count();
        for start in 0..=kept.len() - self.window {
            if start > 0 {
                strong_in_window += usize::from(strong[start + self.window - 1]);
                strong_in_window -= usize::from(strong[start - 1]);
            }
            if strong_in_window < MIN_STRONG_LINES {
                continue;
            }
            groups
                .entry(&line_ids[start..start + self.window])
                .and_modify(|starts| starts.push(start))
                .or_insert(Starts::One(start));
        }
        let mut repeated: Vec<Vec<usize>> = groups
            .into_values()
            .filter_map(|starts| match starts {
                Starts::Many(starts) if starts.len() >= self.min_repeats => Some(starts),
                _ => None,
            })
            .collect();
        repeated.sort_unstable_by_key(|starts| starts[0]);
        repeated
    }

    fn report(
        &self,
        facts: &FileFacts<'_>,
        kept: &[(u32, Cow<'_, str>)],
        starts: &[usize],
        out: &mut Vec<Finding>,
    ) {
        let mut accepted: Vec<u32> = Vec::new();
        let mut next_free = 0usize;
        for &start in starts {
            if start < next_free {
                continue;
            }
            accepted.push(kept[start].0);
            next_free = start + self.window;
        }
        if accepted.len() < self.min_repeats {
            return;
        }
        let lines = accepted
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let message = format!(
            "{} identical {}-line blocks (lines {lines})",
            accepted.len(),
            self.window
        );
        out.push(Finding::at(
            facts,
            Pos {
                line: accepted[0],
                byte: 0,
            },
            INFO.id,
            Severity::Warn,
            message,
            FIX.to_string(),
        ));
    }
}

fn kept_lines(source: &str) -> Vec<(u32, Cow<'_, str>)> {
    let mut kept = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let normalized = normalize_line(line);
        if normalized.is_empty() {
            continue;
        }
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        kept.push((number, normalized));
    }
    kept
}

/// Trims the line and collapses whitespace runs to one space, borrowing when nothing changes.
fn normalize_line(line: &str) -> Cow<'_, str> {
    let trimmed = line.trim();
    let mut after_space = false;
    let already_normal = trimmed.chars().all(|c| {
        let accepted = !c.is_whitespace() || (c == ' ' && !after_space);
        after_space = c == ' ';
        accepted
    });
    if already_normal {
        Cow::Borrowed(trimmed)
    } else {
        Cow::Owned(trimmed.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}

fn is_strong_line(line: &str) -> bool {
    line.chars()
        .filter(|c| !c.is_whitespace())
        .nth(STRONG_LINE_CHARS - 1)
        .is_some()
}
