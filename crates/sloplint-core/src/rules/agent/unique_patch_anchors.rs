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

/// One repeated region: the starts of the occurrences its full span still covers, and the kept
/// lines each spans.
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
    /// later window of that run, so only the run's first window is kept. Later windows may cover
    /// fewer occurrences than the first one: a tail copy's next window can hold different lines
    /// or pass the file's end, and `slides_into` keeps the occurrences those windows still cover
    /// as a prefix of the first window's starts. A run of `n` windows spans `window + n - 1` kept
    /// lines, so only that prefix truly heads a block of the full span. The longer span can also
    /// push occurrences closer than one span apart, leaving fewer than `min_repeats`
    /// non-overlapping ones, so a run keeps the longest span whose surviving occurrences still
    /// fit `min_repeats` of them instead of suppressing repeats that already qualify as plain
    /// windows.
    fn repeated_runs(&self, kept: &[(u32, Cow<'_, str>)]) -> Vec<Run> {
        let repeated = self.repeated_windows(kept);
        let mut runs = Vec::new();
        for (index, group) in repeated.iter().enumerate() {
            if continues_run(&repeated, index) {
                continue;
            }
            // Sizes of the run's window groups, first window first. Each later group drops the
            // tail occurrences whose next window no longer matches or no longer fits, so the
            // occurrences a span of `n` windows covers are the first `lengths[n - 1]` starts of
            // the first window.
            let mut lengths = vec![group.len()];
            let mut current = index;
            while let Some(next) = group_starting_at(&repeated, repeated[current][0] + 1)
                .filter(|&next| slides_into(&repeated[current], &repeated[next]))
            {
                lengths.push(repeated[next].len());
                current = next;
            }
            let first = group;
            // Spans are tried longest first down to a single window, so a run never reports a
            // shorter block than its first window.
            if let Some(run) = (1..=lengths.len()).rev().find_map(|windows| {
                let span = self.window + windows - 1;
                let survivors = &first[..lengths[windows - 1]];
                (non_overlapping_starts(survivors, span).len() >= self.min_repeats).then(|| Run {
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
        kept: &[(u32, Cow<'_, str>)],
        run: &Run,
        out: &mut Vec<Finding>,
    ) {
        let accepted = non_overlapping_starts(&run.starts, run.span);
        if accepted.len() < self.min_repeats {
            return;
        }
        let lines = accepted
            .iter()
            .map(|&start| kept[start].0.to_string())
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
                line: kept[accepted[0]].0,
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

/// True when `later` lists the same starts as `earlier`, each one kept line further on, with
/// `later` allowed to miss the tail occurrences whose next window no longer matches or would
/// pass the file's end.
fn slides_into(earlier: &[usize], later: &[usize]) -> bool {
    later.len() <= earlier.len()
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

/// Occurrences a span admits without overlap: each must start at or after the previous accepted
/// start plus `span`; `starts` are ascending. The span search and `report` must count the same
/// acceptance, so both share this greedy.
fn non_overlapping_starts(starts: &[usize], span: usize) -> Vec<usize> {
    let mut accepted = Vec::new();
    let mut next_free = 0;
    for &start in starts {
        if start < next_free {
            continue;
        }
        accepted.push(start);
        next_free = start + span;
    }
    accepted
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

    #[test]
    fn a_run_whose_coalesced_span_drops_below_min_repeats_keeps_a_qualifying_span() {
        let block: Vec<String> = (0..8)
            .map(|k| format!("let value_{k} = compute_stage_{k}(alpha_input, beta_input);"))
            .collect();
        let copy = block.join("\n");
        let source = format!("{copy}\n{copy}\n{copy}\n{}\n{}\n", block[0], block[1]);
        assert_eq!(
            findings(8, 3, &source),
            [(1, "3 identical 8-line blocks (lines 1, 9, 17)".to_string())],
            "the coalesced span-10 run fits only two non-overlapping occurrences, so the run \
             must keep its span-8 window rather than suppress three qualifying repeats"
        );
    }
}
