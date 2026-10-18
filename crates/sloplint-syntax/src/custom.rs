//! Declarative custom query rules: compilation per grammar and execution per file.

use sloplint_core::lang::Lang;
use sloplint_core::rules::custom::message_placeholders;
use sloplint_core::{ConfigError, CustomHit, CustomRuleSpec};
use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

use crate::hooks::{narrow, pos_of, slice};
use crate::spec::spec;

/// Every compiled custom rule, paired with each grammar it runs against.
///
/// Each entry carries the query's capture names as `'static` strings so a
/// hit can name its captures with the source lifetime the contract requires.
pub struct CustomRules {
    entries: Vec<(Lang, u32, Query, Vec<&'static str>)>,
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
                let names = static_names(&query);
                entries.push((lang, narrow(index), query, names));
            }
        }
        Ok(CustomRules { entries })
    }

    /// Runs every query compiled for `lang` against the file's tree.
    ///
    /// Each match yields one hit at its `@match` capture carrying every
    /// `(capture name, capture text)` pair for message interpolation.
    pub fn run<'s>(&self, tree: &Tree, source: &'s str, lang: Lang) -> Vec<CustomHit<'s>> {
        let mut hits = Vec::new();
        let mut cursor = QueryCursor::new();
        for (entry_lang, rule, query, names) in &self.entries {
            if *entry_lang != lang {
                continue;
            }
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

/// Every `Lang` variant, in declaration order.
const ALL_LANGS: [Lang; 25] = [
    Lang::Rust,
    Lang::TypeScript,
    Lang::Tsx,
    Lang::JavaScript,
    Lang::Python,
    Lang::Go,
    Lang::Java,
    Lang::Kotlin,
    Lang::CSharp,
    Lang::C,
    Lang::Cpp,
    Lang::ObjC,
    Lang::Php,
    Lang::Ruby,
    Lang::Bash,
    Lang::PowerShell,
    Lang::Swift,
    Lang::R,
    Lang::Scala,
    Lang::Dart,
    Lang::Zig,
    Lang::Ocaml,
    Lang::OcamlInterface,
    Lang::Odin,
    Lang::Starlark,
];

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
    let language = spec(lang).language;
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
    for placeholder in message_placeholders(&rule.message) {
        if !names.contains(&placeholder.as_str()) {
            return Err(custom_error(
                rule,
                &format!("message placeholder {{{placeholder}}} names no capture in the query"),
            ));
        }
    }
    Ok(query)
}

/// Copies the query's capture names into process-lifetime strings.
fn static_names(query: &Query) -> Vec<&'static str> {
    query
        .capture_names()
        .iter()
        .map(|name| -> &'static str { name.to_string().leak() })
        .collect()
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
    names: &[&str],
    captures: &[tree_sitter::QueryCapture<'_>],
    rule: u32,
    source: &'s str,
) -> Option<CustomHit<'s>> {
    let mut pos = None;
    let mut pairs = Vec::new();
    for capture in captures {
        let name = names.get(capture.index as usize).copied().unwrap_or("");
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
