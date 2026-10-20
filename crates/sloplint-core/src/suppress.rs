//! `sloplint-allow` suppression comments: parsing, matching, and application.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::finding::Finding;
use crate::rules::meta;

/// A comment's text with the source lines it spans, recorded by the syntax walk.
#[derive(Debug, Clone, Copy)]
pub struct CommentText<'s> {
    /// First source line of the comment, 1-based.
    pub start_line: u32,
    /// Last source line of the comment, 1-based.
    pub end_line: u32,
    /// Raw comment text including delimiters.
    pub text: &'s str,
}

/// One parsed suppression directive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suppression {
    /// First source line covered, 1-based.
    pub start_line: u32,
    /// Last source line covered before the one extra trailing line.
    pub end_line: u32,
    /// File-level suppressions cover every line of the file.
    pub file_level: bool,
    /// Rule patterns this suppression silences.
    pub rules: Vec<RulePattern>,
    /// Set once the suppression has matched at least one finding.
    pub used: bool,
}

/// A suppression operand: an exact rule id or a whole `family/*`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RulePattern {
    /// Matches exactly this rule id.
    Exact(Box<str>),
    /// Matches every rule whose id starts with this family plus `/`.
    Family(Box<str>),
}

impl RulePattern {
    /// Parses `family/*` into [`RulePattern::Family`] and anything else into [`RulePattern::Exact`].
    pub fn parse(s: &str) -> Result<RulePattern, String> {
        if let Some(family) = s.strip_suffix("/*") {
            if family.is_empty() {
                return Err(format!("empty rule family in `{s}`"));
            }
            return Ok(RulePattern::Family(family.into()));
        }
        if s.is_empty() {
            return Err("empty rule id".to_string());
        }
        Ok(RulePattern::Exact(s.into()))
    }

    /// Whether `rule_id` is covered by this pattern.
    pub fn matches(&self, rule_id: &str) -> bool {
        match self {
            RulePattern::Exact(id) => rule_id == &**id,
            RulePattern::Family(family) => rule_id
                .strip_prefix(&**family)
                .is_some_and(|rest| rest.starts_with('/')),
        }
    }
}

const MARKER: &str = "sloplint-allow";
const FILE_MARKER: &str = "sloplint-allow-file";
const SUPPRESSIBLE_FAMILIES: [&str; 5] = ["complexity", "grep", "slop", "agent", "custom"];

enum Problem {
    Malformed,
    MissingReason,
    UnknownRule(String),
}

impl Problem {
    fn text(&self) -> String {
        match self {
            Problem::Malformed => "malformed syntax".to_string(),
            Problem::MissingReason => "missing reason".to_string(),
            Problem::UnknownRule(id) => format!("unknown rule {id}"),
        }
    }
}

/// Scans comments for suppression directives, returning them plus `meta/invalid-suppression` findings.
pub fn parse_suppressions(
    path: &str,
    source: &str,
    comments: &[CommentText<'_>],
    known: &dyn Fn(&str) -> bool,
) -> (Vec<Suppression>, Vec<Finding>) {
    let mut suppressions = Vec::new();
    let mut findings = Vec::new();
    for comment in comments {
        scan_comment(
            path,
            source,
            comment,
            known,
            &mut suppressions,
            &mut findings,
        );
    }
    (suppressions, findings)
}

/// Drops suppressed findings and marks every matching suppression used; `meta/*` is never suppressible.
pub fn apply(findings: &mut Vec<Finding>, suppressions: &mut [Suppression]) {
    let index = Index::of(suppressions);
    let mut used = vec![false; suppressions.len()];
    let mut dropped = vec![false; findings.len()];
    for (finding, drop) in findings.iter().zip(dropped.iter_mut()) {
        if finding.rule.starts_with("meta/") {
            continue;
        }
        *drop = index.suppresses(&finding.rule, finding.line, &mut used);
    }
    let mut drops = dropped.into_iter();
    findings.retain(|_| !drops.next().unwrap_or(false));
    for (suppression, hit) in suppressions.iter_mut().zip(used) {
        suppression.used |= hit;
    }
}

struct Index<'s> {
    exact: HashMap<&'s str, Vec<usize>>,
    families: HashMap<&'s str, Vec<usize>>,
    suppressions: &'s [Suppression],
}

impl<'s> Index<'s> {
    fn of(suppressions: &'s [Suppression]) -> Self {
        let mut exact: HashMap<&'s str, Vec<usize>> = HashMap::new();
        let mut families: HashMap<&'s str, Vec<usize>> = HashMap::new();
        for (id, suppression) in suppressions.iter().enumerate() {
            for pattern in &suppression.rules {
                match pattern {
                    RulePattern::Exact(rule) => exact.entry(&**rule).or_default().push(id),
                    RulePattern::Family(family) => families.entry(&**family).or_default().push(id),
                }
            }
        }
        Index {
            exact,
            families,
            suppressions,
        }
    }

    fn suppresses(&self, rule: &str, line: u32, used: &mut [bool]) -> bool {
        let exact = self.exact.get(rule).into_iter().flatten();
        let families = rule
            .match_indices('/')
            .filter_map(|(at, _)| self.families.get(&rule[..at]))
            .flatten();
        let mut hit = false;
        for &id in exact.chain(families) {
            if self.covers(id, line) {
                used[id] = true;
                hit = true;
            }
        }
        hit
    }

    fn covers(&self, id: usize, line: u32) -> bool {
        let suppression = &self.suppressions[id];
        suppression.file_level
            || (line >= suppression.start_line && line <= suppression.end_line.saturating_add(1))
    }
}

fn scan_comment(
    path: &str,
    source: &str,
    comment: &CommentText<'_>,
    known: &dyn Fn(&str) -> bool,
    suppressions: &mut Vec<Suppression>,
    findings: &mut Vec<Finding>,
) {
    let text = comment.text;
    let mut offset = 0usize;
    while let Some(at) = text[offset..].find(MARKER) {
        let start = offset + at;
        let tail = &text[start..];
        let file_level = tail.starts_with(FILE_MARKER);
        let keyword = if file_level {
            FILE_MARKER.len()
        } else {
            MARKER.len()
        };
        match directive(&tail[keyword..], known) {
            Ok(rules) => suppressions.push(Suppression {
                start_line: comment.start_line,
                end_line: comment.end_line,
                file_level,
                rules,
                used: false,
            }),
            Err(problem) => findings.push(meta::invalid_suppression_finding(
                path,
                source,
                comment.start_line,
                &problem.text(),
            )),
        }
        offset = start + keyword;
    }
}

fn directive(rest: &str, known: &dyn Fn(&str) -> bool) -> Result<Vec<RulePattern>, Problem> {
    let after_open = rest.strip_prefix('(').ok_or(Problem::Malformed)?;
    let close = after_open.find(')').ok_or(Problem::Malformed)?;
    let list = &after_open[..close];
    let reason = after_open[close + 1..]
        .trim_start()
        .strip_prefix(':')
        .ok_or(Problem::Malformed)?;
    if reason_until_terminator(reason).trim().is_empty() {
        return Err(Problem::MissingReason);
    }
    let mut rules = Vec::new();
    for part in list.split(',') {
        let token = part.trim();
        let pattern = RulePattern::parse(token).map_err(|_| Problem::Malformed)?;
        match &pattern {
            RulePattern::Family(family) if !SUPPRESSIBLE_FAMILIES.contains(&&**family) => {
                return Err(Problem::UnknownRule(token.to_string()));
            }
            RulePattern::Exact(id) if !known(id) => {
                return Err(Problem::UnknownRule(token.to_string()));
            }
            _ => {}
        }
        rules.push(pattern);
    }
    Ok(rules)
}

fn reason_until_terminator(text: &str) -> &str {
    let ends = [text.find('\n'), text.find("*/"), text.find("-->")];
    let end = ends.into_iter().flatten().min().unwrap_or(text.len());
    &text[..end]
}
