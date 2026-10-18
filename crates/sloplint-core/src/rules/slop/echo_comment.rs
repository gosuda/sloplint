//! `slop/echo-comment`: doc comments that restate the function name.

use std::collections::BTreeSet;

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Finding, Level, Severity};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};
use crate::text::identifier_words;

const STOPWORDS: [&str; 32] = [
    "a", "an", "the", "of", "to", "for", "in", "on", "at", "by", "with", "and", "or", "is", "are",
    "be", "this", "that", "it", "its", "returns", "return", "given", "from", "into", "function",
    "method", "fn", "func", "def", "self", "cls",
];

/// `slop/echo-comment`: a doc comment whose words restate the name.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/echo-comment",
    family: Family::Slop,
    summary: "doc comments that restate the function name",
    why: "A comment that repeats the name adds tokens and no information.",
    exempt_tests: false,
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    let (level, min_similarity) = match strictness {
        Strictness::Minimal | Strictness::Basic => (Level::Off, 0.8),
        Strictness::Standard => (Level::Warn, 0.8),
        Strictness::Strict => (Level::Warn, 0.65),
    };
    Preset {
        level,
        options: options(min_similarity),
    }
}

fn options(min_similarity: f64) -> toml::Table {
    let mut table = toml::Table::new();
    table.insert(
        "min_similarity".to_string(),
        toml::Value::Float(min_similarity),
    );
    table
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let options: Options = toml::Value::Table(options)
        .try_into()
        .map_err(|error| error.to_string())?;
    if options.min_similarity.is_nan()
        || options.min_similarity <= 0.0
        || options.min_similarity > 1.0
    {
        return Err(format!(
            "min_similarity {} must be greater than 0 and at most 1",
            options.min_similarity
        ));
    }
    Ok(Box::new(EchoComment {
        min_similarity: options.min_similarity,
    }))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    min_similarity: f64,
}

struct EchoComment {
    min_similarity: f64,
}

impl FileRule for EchoComment {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            if function.doc.is_empty() {
                continue;
            }
            let doc_words = word_set(function.doc.iter().copied());
            let name_words = word_set([function.name]);
            let union = doc_words.union(&name_words).count();
            if union == 0 {
                continue;
            }
            let intersection = doc_words.intersection(&name_words).count();
            let Some(similarity) = ratio(intersection, union) else {
                continue;
            };
            if similarity < self.min_similarity {
                continue;
            }
            out.push(Finding::at(
                facts,
                function.doc_pos.unwrap_or(function.pos),
                INFO.id,
                Severity::Warn,
                format!(
                    "doc comment restates the name `{}` (similarity {similarity:.2})",
                    function.name
                ),
                "delete the comment, or replace it with what the name cannot say: preconditions, errors, invariants, or why it exists".to_string(),
            ));
        }
    }
}

fn word_set<'a>(texts: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    texts
        .into_iter()
        .flat_map(identifier_words)
        .filter(|word| !STOPWORDS.contains(&word.as_str()))
        .map(|word| stem(&word))
        .collect()
}

fn stem(word: &str) -> String {
    for suffix in ["ing", "ed", "es", "s"] {
        if let Some(base) = word.strip_suffix(suffix)
            && base.chars().count() >= 3
            && (suffix != "s" || !word.ends_with("ss"))
        {
            return base.to_string();
        }
    }
    word.to_string()
}

fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
    let numerator = f64::from(u32::try_from(numerator).ok()?);
    let denominator = f64::from(u32::try_from(denominator).ok()?);
    Some(numerator / denominator)
}
