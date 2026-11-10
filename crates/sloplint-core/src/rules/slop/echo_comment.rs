use std::collections::BTreeSet;

use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{
    Build, EmptyOptions, Family, FileRule, Preset, RuleInfo, Settings, parse_options,
};
use crate::text::identifier_words;

const STOPWORDS: [&str; 32] = [
    "a", "an", "the", "of", "to", "for", "in", "on", "at", "by", "with", "and", "or", "is", "are",
    "be", "this", "that", "it", "its", "returns", "return", "given", "from", "into", "function",
    "method", "fn", "func", "def", "self", "cls",
];

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/echo-comment",
    family: Family::Slop,
    summary: "doc comments that restate the function name",
    why: "A comment that repeats the name adds tokens and no information.",
    confidence: Confidence::Medium,
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

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal | Strictness::Basic => Level::Off,
        Strictness::Standard | Strictness::Strict => Level::At(Severity::Minor),
    };
    Preset {
        level,
        options: toml::Table::new(),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    parse_options::<EmptyOptions>(options)?;
    Ok(Box::new(EchoComment))
}

struct EchoComment;

impl FileRule for EchoComment {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            if function.doc.is_empty() || function.in_test {
                continue;
            }
            if first_ident(function.doc[0]).is_some_and(|lead| lead == function.name) {
                continue;
            }
            let doc_words = word_set(function.doc.iter().copied());
            let name_words = word_set([function.name]);
            if doc_words != name_words || doc_words.is_empty() {
                continue;
            }
            out.push(Finding::at(
                facts,
                function.doc_pos.unwrap_or(function.pos),
                INFO.id,
                Severity::Minor,
                format!(
                    "doc comment restates the name `{}`",
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
    for suffix in ["ing", "ed", "s", "es"] {
        if let Some(base) = word.strip_suffix(suffix)
            && base.chars().count() >= 4
            && (suffix != "s" || (!word.ends_with("ss") && !word.ends_with("sses")))
        {
            return base.to_string();
        }
    }
    word.to_string()
}

fn first_ident(doc: &str) -> Option<&str> {
    doc.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .find(|part| !part.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{FileFacts, Function, Visibility};
    use crate::finding::line_starts;
    use crate::lang::Lang;

    fn function<'s>(name: &'s str, doc: Vec<&'s str>) -> Function<'s> {
        build_function(name, doc, false)
    }

    fn test_function<'s>(name: &'s str, doc: Vec<&'s str>) -> Function<'s> {
        build_function(name, doc, true)
    }

    fn build_function<'s>(name: &'s str, doc: Vec<&'s str>, in_test: bool) -> Function<'s> {
        Function {
            name,
            pos: crate::facts::Pos { line: 1, byte: 0 },
            doc,
            doc_pos: None,
            visibility: Visibility::Private,
            signature_forced: false,
            entry_point: false,
            in_test,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            is_method: false,
            receiver: None,
            end_line: 1,
            has_value_return: false,
            params: Vec::new(),
            return_type: None,
            untyped_throws: false,
            missing_annotations: Vec::new(),
            cognitive: 0,
            max_depth: 0,
            deepest_line: 0,
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
            package: String::new(),
            is_generated: false,
            line_count: 1,
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

    fn flagged(rule: &EchoComment, source: &str, functions: Vec<Function<'_>>) -> usize {
        let owned = facts(source, functions);
        let mut out = Vec::new();
        rule.check(&owned, &mut out);
        out.len()
    }

    #[test]
    fn exact_restatement_flags() {
        let rule = EchoComment;
        let count = flagged(
            &rule,
            "fn process_order() {}\n",
            vec![function("process_order", vec!["Process the order."])],
        );
        assert_eq!(count, 1, "an exact restatement must flag");
    }

    #[test]
    fn one_extra_word_stays_silent() {
        let rule = EchoComment;
        let count = flagged(
            &rule,
            "fn process_order() {}\n",
            vec![function("process_order", vec!["Process the order simply."])],
        );
        assert_eq!(
            count, 0,
            "one filler word breaks set equality and must stay silent"
        );
    }

    #[test]
    fn name_prefix_sentence_stays_silent() {
        let rule = EchoComment;
        let count = flagged(
            &rule,
            "fn FormatFrame() {}\n",
            vec![function(
                "FormatFrame",
                vec!["FormatFrame formats the given frame."],
            )],
        );
        assert_eq!(
            count, 0,
            "a comment led by the function name must stay silent"
        );
    }

    #[test]
    fn test_function_stays_silent() {
        let rule = EchoComment;
        let count = flagged(
            &rule,
            "fn process_order() {}\n",
            vec![test_function("process_order", vec!["Process the order."])],
        );
        assert_eq!(count, 0, "in_test functions must stay silent");
    }

    #[test]
    fn short_stem_source_stays_silent() {
        let rule = EchoComment;
        let count = flagged(
            &rule,
            "fn _basic_auth_str() {}\n",
            vec![function(
                "_basic_auth_str",
                vec!["Returns a Basic Auth string."],
            )],
        );
        assert_eq!(count, 0, "string must not stem to str");
    }

    #[test]
    fn stem_suffix_cases() {
        assert_eq!(stem("parses"), "parse");
        assert_eq!(stem("string"), "string");
        assert_eq!(stem("processes"), "process");
        assert_eq!(stem("classes"), "class");
        assert_eq!(stem("class"), "class");
    }
}
