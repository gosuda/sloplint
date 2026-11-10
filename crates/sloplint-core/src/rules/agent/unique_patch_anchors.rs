use std::borrow::Cow;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::unique_patch_lines::{is_comment_line, is_strong_line, is_substantive, normalize_line};
use crate::config::Strictness;
use crate::facts::{FileFacts, Function, Pos};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
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
        let accepted: Vec<Occ> = non_overlapping(&run.starts, run.span)
            .into_iter()
            .filter(|occ| {
                let first = segments[occ.seg][occ.at].line;
                let last = segments[occ.seg][occ.at + run.span - 1].line;
                body_contains(&facts.functions, first, last)
            })
            .collect();
        if accepted.len() < self.min_repeats {
            return;
        }
        let lines = accepted
            .iter()
            .map(|occ| segments[occ.seg][occ.at].line.to_string())
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
                line: segments[accepted[0].seg][accepted[0].at].line,
                byte: 0,
            },
            INFO.id,
            Severity::Minor,
            message,
            FIX.to_string(),
        ));
    }
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
mod tests {
    use super::*;
    use crate::facts::Visibility;
    use crate::finding::line_starts;
    use crate::lang::Lang;

    const BLOCK: [&str; 8] = [
        "let alpha_value = compute_first(input);",
        "let beta_value = compute_second(alpha_value);",
        "let gamma_value = compute_third(beta_value);",
        "let delta_value = compute_fourth(gamma_value);",
        "let epsilon_value = compute_fifth(delta_value);",
        "let zeta_value = compute_sixth(epsilon_value);",
        "let eta_value = compute_seventh(zeta_value);",
        "let theta_value = compute_eighth(eta_value);",
    ];

    const TAIL: [&str; 8] = [
        "name: alpha,",
        "value: compute_alpha(input),",
        "mode: Mode::Fast,",
        "limit: 4096,",
        "retry: policy.retry,",
        "label: make_label(name),",
        "flags: flags | Flags::EXTRA,",
        "owner: current_owner(),",
    ];

    fn function(name: &str, first: u32, last: u32) -> Function<'_> {
        Function {
            name,
            pos: Pos {
                line: first,
                byte: 0,
            },
            doc: Vec::new(),
            doc_pos: None,
            visibility: Visibility::Private,
            signature_forced: false,
            entry_point: false,
            in_test: false,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            is_method: false,
            receiver: None,
            params: Vec::new(),
            return_type: None,
            untyped_throws: false,
            missing_annotations: Vec::new(),
            has_value_return: true,
            cognitive: 0,
            max_depth: 0,
            deepest_line: last,
            end_line: last,
            sole_call: None,
        }
    }

    fn facts<'s>(source: &'s str, functions: Vec<Function<'s>>) -> FileFacts<'s> {
        FileFacts {
            lang: Lang::Rust,
            path: "src/lib.rs",
            source,
            line_starts: line_starts(source),
            role: crate::role::CodeRole::Library,
            regions: Vec::new(),
            line_count: 0,
            package: String::new(),
            is_generated: false,
            functions,
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

    fn check(source: &str, functions: Vec<Function<'_>>) -> Vec<String> {
        let rule = UniquePatchAnchors {
            window: 8,
            min_repeats: 3,
        };
        let mut out = Vec::new();
        rule.check(&facts(source, functions), &mut out);
        out.into_iter().map(|finding| finding.message).collect()
    }

    fn bannered_block_source() -> (String, Vec<Function<'static>>) {
        let mut source = String::new();
        for name in ["doc_a", "doc_b", "doc_c"] {
            for index in 0..8 {
                source.push_str(&format!("// banner {name} line {index} filler text here\n"));
            }
            source.push('\n');
        }
        let mut functions = Vec::new();
        for name in ["alpha", "beta", "gamma"] {
            let first = u32::try_from(source.lines().count() + 1).unwrap_or(u32::MAX);
            source.push_str(&format!("fn {name}() {{\n"));
            for line in BLOCK {
                source.push_str(line);
                source.push('\n');
            }
            source.push_str("}\n\n");
            let last = u32::try_from(source.lines().count()).unwrap_or(u32::MAX);
            functions.push(function(name, first, last));
        }
        (source, functions)
    }

    #[test]
    fn rustdoc_banners_stay_silent_while_shared_block_flags() {
        let (source, functions) = bannered_block_source();
        let found = check(&source, functions);
        assert_eq!(found.len(), 1);
        assert!(found[0].starts_with("3 identical 9-line blocks"));
    }

    #[test]
    fn blank_split_copies_stay_silent() {
        let mut source = String::new();
        let mut functions = Vec::new();
        for name in ["alpha", "beta", "gamma"] {
            let first = u32::try_from(source.lines().count() + 1).unwrap_or(u32::MAX);
            source.push_str(&format!("fn {name}() {{\n"));
            for line in &BLOCK[..3] {
                source.push_str(line);
                source.push('\n');
            }
            source.push('\n');
            for line in &BLOCK[3..6] {
                source.push_str(line);
                source.push('\n');
            }
            source.push_str("}\n\n");
            let last = u32::try_from(source.lines().count()).unwrap_or(u32::MAX);
            functions.push(function(name, first, last));
        }
        assert!(check(&source, functions).is_empty());
    }

    #[test]
    fn struct_tails_without_executable_lines_stay_silent() {
        let mut source = String::new();
        let mut functions = Vec::new();
        for name in ["alpha", "beta", "gamma"] {
            let first = u32::try_from(source.lines().count() + 1).unwrap_or(u32::MAX);
            source.push_str(&format!("fn {name}() {{\n"));
            source.push_str(&format!("let rendered_{name} = Widget {{\n"));
            for line in TAIL {
                source.push_str(line);
                source.push('\n');
            }
            source.push_str("};\n}\n\n");
            let last = u32::try_from(source.lines().count()).unwrap_or(u32::MAX);
            functions.push(function(name, first, last));
        }
        assert!(check(&source, functions).is_empty());
    }

    #[test]
    fn shared_attribute_line_keeps_copies_distinguishable() {
        const STEPS: [&str; 7] = [
            "let step_zero = begin_phase(zero_input);",
            "let step_one = advance_phase(step_zero);",
            "let step_two = advance_phase(step_one);",
            "let step_three = advance_phase(step_two);",
            "let step_four = advance_phase(step_three);",
            "let step_five = advance_phase(step_four);",
            "let step_six = finish_phase(step_five);",
        ];
        let mut source = String::new();
        let mut functions = Vec::new();
        for name in ["alpha", "beta", "gamma"] {
            let first = u32::try_from(source.lines().count() + 1).unwrap_or(u32::MAX);
            source.push_str(&format!("fn {name}() {{\n"));
            source.push_str("#[must_use]\n");
            for line in STEPS {
                source.push_str(line);
                source.push('\n');
            }
            source.push_str("}\n\n");
            let last = u32::try_from(source.lines().count()).unwrap_or(u32::MAX);
            functions.push(function(name, first, last));
        }
        let found = check(&source, functions);
        assert_eq!(found.len(), 1);
        assert!(found[0].starts_with("3 identical 9-line blocks"));
    }

    #[test]
    fn top_level_runs_without_functions_stay_silent() {
        let mut source = String::new();
        for _ in 0..3 {
            for line in BLOCK {
                source.push_str(line);
                source.push('\n');
            }
            source.push('\n');
        }
        assert!(check(&source, Vec::new()).is_empty());
    }
    #[test]
    fn preset_pins_off_at_standard_and_info_at_strict() {
        assert_eq!(
            INFO.confidence,
            Confidence::Low,
            "unique-patch-anchors ships at low confidence"
        );
        assert_eq!(
            preset(Strictness::Minimal).level,
            Level::Off,
            "minimal stays off"
        );
        assert_eq!(
            preset(Strictness::Basic).level,
            Level::Off,
            "basic stays off"
        );
        assert_eq!(
            preset(Strictness::Standard).level,
            Level::Off,
            "standard stays off"
        );
        let strict_preset = preset(Strictness::Strict);
        assert_eq!(
            strict_preset.level,
            Level::At(Severity::Info),
            "strict reports at info"
        );
        let rule =
            build(strict_preset.options, &Settings::default()).expect("strict preset builds");
        let (source, functions) = bannered_block_source();
        let mut out = Vec::new();
        rule.check(&facts(&source, functions), &mut out);
        assert_eq!(
            out.len(),
            1,
            "strict flags the shared block on a positive input"
        );
    }
}
