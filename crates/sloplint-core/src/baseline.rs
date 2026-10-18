//! Baseline files for ratchet mode and the finding fingerprints they key on.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::finding::Finding;

/// A set of baselined findings, keyed by path, rule, and fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    /// Every recorded finding identity.
    pub entries: BTreeSet<BaselineEntry>,
}

/// One baselined finding identity; orders by path, then rule, then fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BaselineEntry {
    /// Repo-relative path with `/` separators.
    pub path: Box<str>,
    /// Rule id that produced the finding.
    pub rule: Box<str>,
    /// 32 lowercase hex chars identifying the finding's stable content.
    pub fingerprint: Box<str>,
}

#[derive(Serialize, Deserialize)]
struct BaselineFile {
    version: u32,
    findings: Vec<BaselineEntry>,
}

impl Baseline {
    /// Parses a baseline document; only version 1 is accepted.
    pub fn from_json(s: &str) -> Result<Baseline, String> {
        let doc: BaselineFile =
            sonic_rs::from_str(s).map_err(|error| format!("invalid baseline JSON: {error}"))?;
        if doc.version != 1 {
            return Err(format!("unsupported baseline version {}", doc.version));
        }
        Ok(Baseline {
            entries: doc.findings.into_iter().collect(),
        })
    }

    /// Pretty-prints the baseline with sorted entries and a trailing newline.
    pub fn to_json(&self) -> String {
        let doc = BaselineFile {
            version: 1,
            findings: self.entries.iter().cloned().collect(),
        };
        let mut json =
            sonic_rs::to_string_pretty(&doc).expect("baseline serialization cannot fail");
        json.push('\n');
        json
    }

    /// Collects findings that already carry fingerprints into a baseline.
    pub fn from_findings(findings: &[Finding]) -> Baseline {
        Baseline {
            entries: findings
                .iter()
                .map(|finding| BaselineEntry {
                    path: finding.path.clone(),
                    rule: finding.rule.clone(),
                    fingerprint: finding.fingerprint.clone(),
                })
                .collect(),
        }
    }

    /// Assigns `fingerprint` to every finding; input must be sorted by path then line.
    pub fn assign_fingerprints(findings: &mut [Finding]) {
        let mut seen: BTreeMap<(String, String, String), u32> = BTreeMap::new();
        for finding in findings.iter_mut() {
            let normalized = normalized_context(&finding.context);
            let key = (
                finding.rule.to_string(),
                finding.path.to_string(),
                normalized.clone(),
            );
            let slot = seen.entry(key).or_insert(0);
            let occurrence = *slot;
            *slot += 1;
            let hex = fingerprint(&finding.rule, &finding.path, &normalized, occurrence);
            finding.fingerprint = hex.into_boxed_str();
        }
    }

    /// Splits findings into kept and baselined, counting baselined matches and stale in-scope entries.
    pub fn filter(
        &self,
        findings: Vec<Finding>,
        in_scope: &dyn Fn(&str) -> bool,
    ) -> (Vec<Finding>, u32, u32) {
        let mut matched: BTreeSet<&BaselineEntry> = BTreeSet::new();
        let mut kept = Vec::new();
        let mut baselined = 0u32;
        for finding in findings {
            let probe = BaselineEntry {
                path: finding.path.clone(),
                rule: finding.rule.clone(),
                fingerprint: finding.fingerprint.clone(),
            };
            if let Some(entry) = self.entries.get(&probe) {
                baselined += 1;
                matched.insert(entry);
            } else {
                kept.push(finding);
            }
        }
        let stale = u32::try_from(
            self.entries
                .iter()
                .filter(|entry| in_scope(&entry.path) && !matched.contains(entry))
                .count(),
        )
        .unwrap_or(u32::MAX);
        (kept, baselined, stale)
    }
}

fn normalized_context(context: &str) -> String {
    context.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fingerprint(rule: &str, path: &str, normalized: &str, occurrence: u32) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(rule.as_bytes());
    hasher.update(&[0]);
    hasher.update(path.as_bytes());
    hasher.update(&[0]);
    hasher.update(normalized.as_bytes());
    hasher.update(&[0]);
    hasher.update(occurrence.to_string().as_bytes());
    let hex = hasher.finalize().to_hex();
    hex.as_str()[..32].to_string()
}

#[cfg(test)]
mod tests {
    use crate::finding::{Finding, Severity};
    use proptest::prelude::*;
    use proptest::string::string_regex;

    use super::{Baseline, fingerprint};

    fn finding_strategy() -> impl Strategy<Value = Finding> {
        (
            string_regex("[a-z][a-z0-9-]{0,15}").unwrap(),
            string_regex("[a-z][a-z0-9/]{0,20}").unwrap(),
            string_regex("[ -~]{0,60}").unwrap(),
            any::<u32>(),
        )
            .prop_map(|(rule, path, context, line)| Finding {
                rule: rule.into_boxed_str(),
                severity: Severity::Warn,
                path: path.into_boxed_str(),
                line,
                column: 1,
                message: String::new(),
                fix: String::new(),
                context: context.into_boxed_str(),
                fingerprint: Box::from(""),
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
}
