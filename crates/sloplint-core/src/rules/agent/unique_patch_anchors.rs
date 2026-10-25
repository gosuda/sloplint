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
        for run in self.repeated_runs(&kept) {
            self.report(facts, &kept, &run, out);
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

/// One repeated region: the first window starts of its occurrences and the kept lines each spans.
struct Run {
    starts: Vec<usize>,
    span: usize,
}

impl UniquePatchAnchors {
    /// The start lists of every strong window content that occurs at least `min_repeats` times,
    /// ordered by first start.
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

    /// Each repeated region once, ascending by its first start.
    ///
    /// A window group whose starts each sit one kept line after the previous group's starts is a
    /// later window of that run, so only the run's first window is kept. A run of `n` windows
    /// spans `window + n - 1` kept lines, which is the block length reported.
    fn repeated_runs(&self, kept: &[(u32, Cow<'_, str>)]) -> Vec<Run> {
        let repeated = self.repeated_windows(kept);
        (0..repeated.len())
            .filter(|&index| !continues_run(&repeated, index))
            .map(|index| Run {
                starts: repeated[index].clone(),
                span: self.window + run_windows(&repeated, index) - 1,
            })
            .collect()
    }

    fn report(
        &self,
        facts: &FileFacts<'_>,
        kept: &[(u32, Cow<'_, str>)],
        run: &Run,
        out: &mut Vec<Finding>,
    ) {
        let mut accepted: Vec<u32> = Vec::new();
        let mut next_free = 0usize;
        for &start in &run.starts {
            if start < next_free {
                continue;
            }
            accepted.push(kept[start].0);
            next_free = start + run.span;
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
            run.span
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

/// Index of the repeated group whose first window starts at `start`.
fn group_starting_at(repeated: &[Vec<usize>], start: usize) -> Option<usize> {
    repeated
        .binary_search_by_key(&start, |starts| starts[0])
        .ok()
}

/// True when `later` lists the same starts as `earlier`, each one kept line further on.
fn slides_into(earlier: &[usize], later: &[usize]) -> bool {
    earlier.len() == later.len()
        && earlier
            .iter()
            .zip(later)
            .all(|(&before, &after)| before + 1 == after)
}

/// True when group `index` is the next window of an earlier group's run.
fn continues_run(repeated: &[Vec<usize>], index: usize) -> bool {
    let starts = &repeated[index];
    starts[0]
        .checked_sub(1)
        .and_then(|first| group_starting_at(repeated, first))
        .is_some_and(|earlier| slides_into(&repeated[earlier], starts))
}

/// Windows in the run that begins at group `index`, counting that group.
fn run_windows(repeated: &[Vec<usize>], index: usize) -> usize {
    let mut current = index;
    let mut windows = 1;
    while let Some(next) = group_starting_at(repeated, repeated[current][0] + 1)
        .filter(|&next| slides_into(&repeated[current], &repeated[next]))
    {
        current = next;
        windows += 1;
    }
    windows
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::line_starts;
    use crate::lang::Lang;

    const COMMENT_BLOCK: [&str; 7] = [
        "// ========================================",
        "// Section divider for the module layout",
        "// Line three of repeated comment block here",
        "// Line four of repeated comment block here",
        "// Line five of repeated comment block here",
        "// Line six of repeated comment block here",
        "// ========================================",
    ];

    fn facts(source: &str) -> FileFacts<'_> {
        FileFacts {
            lang: Lang::Rust,
            path: "src/lib.rs",
            source,
            line_starts: line_starts(source),
            is_test_file: false,
            is_entry_file: false,
            line_count: 0,
            functions: Vec::new(),
            calls: Vec::new(),
            bindings: Vec::new(),
            imports: Vec::new(),
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
        }
    }

    /// The `(line, message)` of every finding the rule reports for `source`.
    fn findings(window: usize, min_repeats: usize, source: &str) -> Vec<(u32, String)> {
        let rule = UniquePatchAnchors {
            window,
            min_repeats,
        };
        let mut out = Vec::new();
        rule.check(&facts(source), &mut out);
        out.into_iter()
            .map(|finding| (finding.line, finding.message))
            .collect()
    }

    /// One copy of the comment block per name, each followed by its own `fn` line and a blank line.
    fn comment_copies(names: &[&str]) -> String {
        names
            .iter()
            .map(|name| format!("{}\npub fn {name}() {{}}\n\n", COMMENT_BLOCK.join("\n")))
            .collect()
    }

    /// Ten strong lines whose fifth line is `middle`, preceded by a marker line unique to the copy.
    fn marked_block(marker: usize, middle: &str) -> String {
        let lines = [
            "let alpha_value = compute_first(input);",
            "let beta_value = compute_second(alpha_value);",
            "let gamma_value = compute_third(beta_value);",
            "let delta_value = compute_fourth(gamma_value);",
            middle,
            "let zeta_value = compute_sixth(epsilon_value);",
            "let eta_value = compute_seventh(zeta_value);",
            "let theta_value = compute_eighth(eta_value);",
            "let iota_value = compute_ninth(theta_value);",
            "let kappa_value = compute_tenth(iota_value);",
        ];
        format!("// copy {marker}\n{}\n", lines.join("\n"))
    }

    #[test]
    fn region_repeated_three_times_reports_once_at_first_line_with_true_length() {
        for window in 3..=7 {
            let found = findings(window, 3, &comment_copies(&["a", "b", "c"]));
            assert_eq!(
                found,
                [(1, "3 identical 7-line blocks (lines 1, 10, 19)".to_string())],
                "window {window}: a seven-line region must be one finding, not one per offset"
            );
        }
    }

    #[test]
    fn region_repeated_twice_is_below_min_repeats() {
        assert!(
            findings(6, 3, &comment_copies(&["a", "b"])).is_empty(),
            "two copies must not reach min_repeats of three"
        );
    }

    #[test]
    fn changed_middle_line_reports_only_the_parts_that_repeat_three_times() {
        let source = format!(
            "{}{}{}",
            marked_block(0, "let epsilon_value = compute_fifth(delta_value);"),
            marked_block(1, "let epsilon_value = compute_fifth_changed(delta_value);"),
            marked_block(2, "let epsilon_value = compute_fifth(delta_value);"),
        );
        assert_eq!(
            findings(4, 3, &source),
            [
                (2, "3 identical 4-line blocks (lines 2, 13, 24)".to_string()),
                (7, "3 identical 5-line blocks (lines 7, 18, 29)".to_string()),
            ],
            "only the unchanged prefix and suffix of the block may be reported"
        );
    }
}
