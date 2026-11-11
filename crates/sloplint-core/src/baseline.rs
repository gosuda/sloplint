use std::borrow::Cow;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt::{self, Write as _};

use serde::de::{IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::finding::Finding;

/// 2x headroom over the ~502k findings an audit of a 100k-file Go corpus produced.
pub const MAX_BASELINE_ENTRIES: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    pub entries: BTreeSet<BaselineEntry>,
    pub skipped: BTreeSet<SkippedEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SkippedEntry {
    /// Repo-relative path with `/` separators.
    pub path: Box<str>,
    pub reason: Box<str>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BaselineEntry {
    /// Repo-relative path with `/` separators.
    pub path: Box<str>,
    pub rule: Box<str>,
    pub fingerprint: Box<str>,
}

#[derive(Debug, Error)]
pub enum BaselineError {
    #[error("invalid baseline JSON: {0}")]
    Json(String),
    #[error("unsupported baseline version {0}")]
    Version(u32),
    #[error("has {count} entries, more than the limit of {limit}")]
    TooManyEntries { count: usize, limit: usize },
}

#[derive(Serialize)]
struct BaselineFile<'a> {
    version: u32,
    findings: &'a BTreeSet<BaselineEntry>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    skipped: &'a BTreeSet<SkippedEntry>,
}

#[derive(Deserialize)]
struct BaselineDocument {
    version: u32,
    #[serde(deserialize_with = "capped_findings")]
    findings: Findings,
    #[serde(default, deserialize_with = "capped_skipped")]
    skipped: SkippedList,
}

enum Findings {
    Within(Vec<BaselineEntry>),
    Over(usize),
}

enum SkippedList {
    Within(Vec<SkippedEntry>),
    Over(usize),
}

impl Default for SkippedList {
    fn default() -> SkippedList {
        SkippedList::Within(Vec::new())
    }
}

/// Past the ceiling, remaining elements are counted but not kept, so the stored baseline holds at most the entry limit.
fn capped_findings<'de, D>(deserializer: D) -> Result<Findings, D::Error>
where
    D: Deserializer<'de>,
{
    struct Capped;

    impl<'de> Visitor<'de> for Capped {
        type Value = Findings;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a list of baseline findings")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Findings, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut entries = Vec::new();
            while entries.len() < MAX_BASELINE_ENTRIES {
                let Some(entry) = seq.next_element::<BaselineEntry>()? else {
                    return Ok(Findings::Within(entries));
                };
                entries.push(entry);
            }
            let mut extra = 0usize;
            while seq.next_element::<IgnoredAny>()?.is_some() {
                extra += 1;
            }
            Ok(if extra == 0 {
                Findings::Within(entries)
            } else {
                Findings::Over(MAX_BASELINE_ENTRIES + extra)
            })
        }
    }

    deserializer.deserialize_seq(Capped)
}

fn capped_skipped<'de, D>(deserializer: D) -> Result<SkippedList, D::Error>
where
    D: Deserializer<'de>,
{
    struct CappedSkipped;

    impl<'de> Visitor<'de> for CappedSkipped {
        type Value = SkippedList;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a list of skipped paths")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<SkippedList, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut entries = Vec::new();
            while entries.len() < MAX_BASELINE_ENTRIES {
                let Some(entry) = seq.next_element::<SkippedEntry>()? else {
                    return Ok(SkippedList::Within(entries));
                };
                entries.push(entry);
            }
            let mut extra = 0usize;
            while seq.next_element::<IgnoredAny>()?.is_some() {
                extra += 1;
            }
            Ok(if extra == 0 {
                SkippedList::Within(entries)
            } else {
                SkippedList::Over(MAX_BASELINE_ENTRIES + extra)
            })
        }
    }

    deserializer.deserialize_seq(CappedSkipped)
}

impl Baseline {
    pub fn from_json(s: &str) -> Result<Baseline, BaselineError> {
        let doc: BaselineDocument =
            sonic_rs::from_str(s).map_err(|error| BaselineError::Json(error.to_string()))?;
        if doc.version != 1 {
            return Err(BaselineError::Version(doc.version));
        }
        let entries = match doc.findings {
            Findings::Within(entries) => entries,
            Findings::Over(count) => {
                return Err(BaselineError::TooManyEntries {
                    count,
                    limit: MAX_BASELINE_ENTRIES,
                });
            }
        };
        let skipped = match doc.skipped {
            SkippedList::Within(entries) => entries,
            SkippedList::Over(count) => {
                return Err(BaselineError::TooManyEntries {
                    count,
                    limit: MAX_BASELINE_ENTRIES,
                });
            }
        };
        if !Baseline::entries_within_limit(entries.len(), skipped.len()) {
            return Err(BaselineError::TooManyEntries {
                count: entries.len().saturating_add(skipped.len()),
                limit: MAX_BASELINE_ENTRIES,
            });
        }
        Ok(Baseline {
            entries: entries.into_iter().collect(),
            skipped: skipped.into_iter().collect(),
        })
    }

    pub fn entries_within_limit(findings: usize, skipped: usize) -> bool {
        findings <= MAX_BASELINE_ENTRIES
            && skipped <= MAX_BASELINE_ENTRIES
            && findings.saturating_add(skipped) <= MAX_BASELINE_ENTRIES
    }

    pub fn newly_skipped(&self, current: &[SkippedEntry]) -> Vec<SkippedEntry> {
        let known: BTreeSet<&str> = self.skipped.iter().map(|entry| &*entry.path).collect();
        let mut out: Vec<SkippedEntry> = current
            .iter()
            .filter(|entry| !known.contains(&*entry.path))
            .cloned()
            .collect();
        out.sort();
        out
    }

    pub fn empty() -> Baseline {
        Baseline {
            entries: BTreeSet::new(),
            skipped: BTreeSet::new(),
        }
    }

    pub fn to_json(&self) -> String {
        let doc = BaselineFile {
            version: 1,
            findings: &self.entries,
            skipped: &self.skipped,
        };
        let mut json =
            sonic_rs::to_string_pretty(&doc).expect("baseline serialization cannot fail");
        json.push('\n');
        json
    }

    pub fn from_findings(findings: &[Finding]) -> Baseline {
        Baseline::from_findings_and_skipped(findings, &[])
    }

    pub fn from_findings_and_skipped(findings: &[Finding], skipped: &[SkippedEntry]) -> Baseline {
        Baseline {
            entries: findings
                .iter()
                .map(|finding| BaselineEntry {
                    path: finding.path.clone(),
                    rule: finding.rule.clone(),
                    fingerprint: finding.fingerprint.clone(),
                })
                .collect(),
            skipped: skipped.iter().cloned().collect(),
        }
    }

    /// Input must be sorted by path then line, so equal findings get ascending occurrence numbers.
    pub fn assign_fingerprints(findings: &mut [Finding]) {
        let mut assigned: Vec<Box<str>> = Vec::with_capacity(findings.len());
        let mut seen: HashMap<(&str, &str, Cow<'_, str>), u32> =
            HashMap::with_capacity(findings.len());
        for finding in findings.iter() {
            let key = (
                &*finding.rule,
                &*finding.path,
                normalized_context(&finding.context),
            );
            assigned.push(match seen.entry(key) {
                Entry::Occupied(mut slot) => {
                    let occurrence = *slot.get();
                    *slot.get_mut() += 1;
                    fingerprint(&finding.rule, &finding.path, &slot.key().2, occurrence)
                }
                Entry::Vacant(slot) => {
                    let hex = fingerprint(&finding.rule, &finding.path, &slot.key().2, 0);
                    slot.insert(1);
                    hex
                }
            });
        }
        drop(seen);
        for (finding, hex) in findings.iter_mut().zip(assigned) {
            finding.fingerprint = hex;
        }
    }

    pub fn filter(
        &self,
        findings: Vec<Finding>,
        in_scope: &dyn Fn(&str) -> bool,
    ) -> (Vec<Finding>, u32, u32) {
        let ordered: Vec<&BaselineEntry> = self.entries.iter().collect();
        let mut matched = vec![false; ordered.len()];
        let mut kept = Vec::new();
        let mut baselined = 0u32;
        for finding in findings {
            let identity = (&*finding.path, &*finding.rule, &*finding.fingerprint);
            let position = ordered.binary_search_by(|entry| {
                (&*entry.path, &*entry.rule, &*entry.fingerprint).cmp(&identity)
            });
            match position {
                Ok(index) => {
                    baselined += 1;
                    matched[index] = true;
                }
                Err(_) => kept.push(finding),
            }
        }
        let stale = u32::try_from(
            ordered
                .iter()
                .zip(&matched)
                .filter(|(entry, hit)| !**hit && in_scope(&entry.path))
                .count(),
        )
        .unwrap_or(u32::MAX);
        (kept, baselined, stale)
    }
}

fn normalized_context(context: &str) -> Cow<'_, str> {
    if is_whitespace_normalized(context) {
        return Cow::Borrowed(context);
    }
    let mut joined = String::with_capacity(context.len());
    for (index, word) in context.split_whitespace().enumerate() {
        if index > 0 {
            joined.push(' ');
        }
        joined.push_str(word);
    }
    Cow::Owned(joined)
}

fn is_whitespace_normalized(context: &str) -> bool {
    let mut after_space = true;
    for ch in context.chars() {
        if ch.is_whitespace() {
            if ch != ' ' || after_space {
                return false;
            }
            after_space = true;
        } else {
            after_space = false;
        }
    }
    context.is_empty() || !after_space
}

struct HashSink<'a>(&'a mut blake3::Hasher);

impl fmt::Write for HashSink<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0.update(text.as_bytes());
        Ok(())
    }
}

fn fingerprint(rule: &str, path: &str, normalized: &str, occurrence: u32) -> Box<str> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(rule.as_bytes());
    hasher.update(&[0]);
    hasher.update(path.as_bytes());
    hasher.update(&[0]);
    hasher.update(normalized.as_bytes());
    hasher.update(&[0]);
    let _ = write!(HashSink(&mut hasher), "{occurrence}");
    let hex = hasher.finalize().to_hex();
    Box::from(&hex.as_str()[..32])
}

#[cfg(test)]
mod tests {
    use crate::finding::{Confidence, Finding, Severity};
    use proptest::prelude::*;
    use proptest::string::string_regex;

    use super::{Baseline, MAX_BASELINE_ENTRIES, fingerprint};

    fn finding_strategy() -> impl Strategy<Value = Finding> {
        (
            string_regex("[a-z][a-z0-9-]{0,15}").unwrap(),
            string_regex("[a-z][a-z0-9/]{0,20}").unwrap(),
            string_regex("[ -~]{0,60}").unwrap(),
            any::<u32>(),
        )
            .prop_map(|(rule, path, context, line)| Finding {
                rule: rule.into_boxed_str(),
                severity: Severity::Minor,
                confidence: Confidence::Medium,
                path: path.into_boxed_str(),
                line,
                column: 1,
                message: String::new(),
                fix: String::new(),
                context: context.into_boxed_str(),
                fingerprint: Box::from(""),
                context_pending: false,
                file_wide: false,
            })
    }

    proptest! {
        #[test]
        fn fingerprint_is_deterministic(
            rule in "[a-z][a-z0-9/-]{0,20}",
            path in "[a-z][a-z0-9./]{0,20}",
            context in "[ -~]{0,80}",
            occurrence in 0u32..1000,
        ) {
            let first = fingerprint(&rule, &path, &context, occurrence);
            let second = fingerprint(&rule, &path, &context, occurrence);
            prop_assert_eq!(first, second);
        }

        #[test]
        fn fingerprint_changes_with_every_input_part(
            rule in "[a-z]{1,8}",
            path in "[a-z]{1,8}",
            context in "[ -~]{1,40}",
            occurrence in 0u32..100,
        ) {
            let base = fingerprint(&rule, &path, &context, occurrence);
            prop_assert_ne!(&base, &fingerprint("zz-other-rule", &path, &context, occurrence));
            prop_assert_ne!(&base, &fingerprint(&rule, "zz/other/path.rs", &context, occurrence));
            prop_assert_ne!(&base, &fingerprint(&rule, &path, "zz other context", occurrence));
            prop_assert_ne!(&base, &fingerprint(&rule, &path, &context, occurrence + 1));
        }

        #[test]
        fn baseline_json_roundtrip(findings in proptest::collection::vec(finding_strategy(), 1..10)) {
            let mut findings = findings;
            findings.sort_by(|a, b| (&*a.path, a.line).cmp(&(&*b.path, b.line)));
            Baseline::assign_fingerprints(&mut findings);
            let baseline = Baseline::from_findings(&findings);
            let json = baseline.to_json();
            let reparsed = Baseline::from_json(&json).unwrap();
            prop_assert_eq!(&reparsed.entries, &baseline.entries);
            prop_assert_eq!(reparsed.to_json(), json);
        }

        #[test]
        fn baseline_filter_consumes_own_findings(
            findings in proptest::collection::vec(finding_strategy(), 1..10),
        ) {
            let count = u32::try_from(findings.len()).unwrap_or(u32::MAX);
            let mut findings = findings;
            findings.sort_by(|a, b| (&*a.path, a.line).cmp(&(&*b.path, b.line)));
            Baseline::assign_fingerprints(&mut findings);
            let baseline = Baseline::from_findings(&findings);
            let (kept, baselined, stale) = baseline.filter(findings, &|_| true);
            prop_assert!(kept.is_empty());
            prop_assert_eq!(baselined, count);
            prop_assert_eq!(stale, 0);
        }
    }

    #[test]
    fn entry_budget_counts_findings_and_skipped_together() {
        assert!(
            Baseline::entries_within_limit(MAX_BASELINE_ENTRIES, 0),
            "findings alone may fill the budget"
        );
        assert!(
            Baseline::entries_within_limit(0, MAX_BASELINE_ENTRIES),
            "skipped paths alone may fill the budget"
        );
        assert!(
            !Baseline::entries_within_limit(MAX_BASELINE_ENTRIES, 1),
            "one entry past a full findings budget is refused"
        );
        assert!(
            !Baseline::entries_within_limit(1, MAX_BASELINE_ENTRIES),
            "one entry past a full skipped budget is refused"
        );
        assert!(
            !Baseline::entries_within_limit(usize::MAX, 1),
            "the total never wraps past the ceiling"
        );
    }
}
