//! `agent/unique-patch-anchors`: repeated identical line blocks.

use std::collections::BTreeMap;

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
        for starts in self.grouped_windows(&kept) {
            self.report(facts, &kept, &starts, out);
        }
    }
}

impl UniquePatchAnchors {
    fn grouped_windows(&self, kept: &[(u32, String)]) -> Vec<Vec<usize>> {
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for start in 0..=kept.len() - self.window {
            let block: Vec<&str> = kept[start..start + self.window]
                .iter()
                .map(|(_, line)| line.as_str())
                .collect();
            if strong_lines(&block) < MIN_STRONG_LINES {
                continue;
            }
            groups.entry(block.join("\n")).or_default().push(start);
        }
        groups.into_values().collect()
    }

    fn report(
        &self,
        facts: &FileFacts<'_>,
        kept: &[(u32, String)],
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

fn kept_lines(source: &str) -> Vec<(u32, String)> {
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

fn normalize_line(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strong_lines(block: &[&str]) -> usize {
    block
        .iter()
        .filter(|line| line.chars().filter(|c| !c.is_whitespace()).count() >= STRONG_LINE_CHARS)
        .count()
}
