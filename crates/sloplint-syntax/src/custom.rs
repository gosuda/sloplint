//! Declarative custom query rules: compilation per grammar and execution per file.

use sloplint_core::lang::{ALL_LANGS, Lang};
use sloplint_core::rules::custom::message_placeholders;
use sloplint_core::{ConfigError, CustomHit, CustomRuleSpec};
use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

use crate::hooks::{narrow, pos_of, slice};
use crate::spec::spec;

/// Every compiled custom rule, paired with each grammar it runs against.
///
/// The query owns its capture names; `run` borrows them from the query, so
/// hits borrow the rules table for capture names and the file source for
/// capture text.
pub struct CustomRules {
    entries: Vec<(Lang, u32, Query)>,
}

impl CustomRules {
    /// Compiles every spec against its language's grammar or grammars.
    ///
    /// `typescript` specs compile against both the TypeScript and TSX
    /// grammars, and `ocaml` specs against both the `.ml` and `.mli`
    /// grammars, so one spec covers every file spelling of a language.
    pub fn compile(specs: &[CustomRuleSpec]) -> Result<CustomRules, ConfigError> {
        let mut entries = Vec::new();
        for (index, rule) in specs.iter().enumerate() {
            let langs = langs_for_key(&rule.language);
            if langs.is_empty() {
                return Err(ConfigError::UnknownLanguage {
                    value: rule.language.clone(),
                });
            }
            for lang in langs {
                let query = compile_query(lang, rule)?;
                entries.push((lang, narrow(index), query));
            }
        }
        Ok(CustomRules { entries })
    }

    /// Runs every query compiled for `lang` against the file's tree.
    ///
    /// Each match yields one hit at its `@match` capture carrying every
    /// `(capture name, capture text)` pair for message interpolation.
    ///
    /// The returned hits borrow the rules table (`&'s self`) for capture
    /// names and the file source for capture text; the caller must keep
    /// both alive while the hits are used.
    pub(crate) fn run<'s>(
        &'s self,
        tree: &Tree,
        source: &'s str,
        lang: Lang,
    ) -> Vec<CustomHit<'s>> {
        let mut queries = self
            .entries
            .iter()
            .filter(|(entry_lang, _, _)| *entry_lang == lang)
            .peekable();
        if queries.peek().is_none() {
            return Vec::new();
        }
        let mut hits = Vec::new();
        let mut cursor = QueryCursor::new();
        for (_, rule, query) in queries {
            let names = query.capture_names();
            let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
            while let Some(found) = matches.next() {
                if let Some(hit) = hit_from_match(names, found.captures(), *rule, source) {
                    hits.push(hit);
                }
            }
        }
        hits
    }
}

/// The languages a config key names; empty for an unknown key.
fn langs_for_key(key: &str) -> Vec<Lang> {
    ALL_LANGS
        .iter()
        .copied()
        .filter(|lang| lang.config_key() == key)
        .collect()
}

/// Compiles one spec against one grammar and validates its captures.
fn compile_query(lang: Lang, rule: &CustomRuleSpec) -> Result<Query, ConfigError> {
    let language = spec(lang).language.clone();
    let query = Query::new(&language, &rule.query).map_err(|error| {
        custom_error(
            rule,
            &format!(
                "query error {:?} at row {} column {}: {}",
                error.kind, error.row, error.column, error.message
            ),
        )
    })?;
    let names = query.capture_names();
    if !names.contains(&"match") {
        return Err(custom_error(rule, "query must define a @match capture"));
    }
    let placeholders = message_placeholders(&rule.message)
        .map_err(|error| custom_error(rule, &error.to_string()))?;
    for placeholder in placeholders {
        if !names.contains(&placeholder.as_str()) {
            return Err(custom_error(
                rule,
                &format!("message placeholder {{{placeholder}}} names no capture in the query"),
            ));
        }
    }
    Ok(query)
}

/// Wraps a compile failure in the typed custom-rule error.
fn custom_error(rule: &CustomRuleSpec, message: &str) -> ConfigError {
    ConfigError::CustomRule {
        id: rule.id.clone(),
        message: message.to_string(),
    }
}

/// Builds one hit from a match; `None` when the match lacks `@match`.
fn hit_from_match<'s>(
    names: &[&'s str],
    captures: &[tree_sitter::QueryCapture<'_>],
    rule: u32,
    source: &'s str,
) -> Option<CustomHit<'s>> {
    let mut pos = None;
    let mut pairs = Vec::new();
    for capture in captures {
        let name = names[capture.index as usize];
        let text = slice(source, capture.node.start_byte(), capture.node.end_byte());
        if name == "match" {
            pos = Some(pos_of(capture.node));
        }
        pairs.push((name, text));
    }
    Some(CustomHit {
        rule,
        pos: pos?,
        captures: pairs,
    })
}
