//! Turns tree-sitter parse trees into sloplint-core file facts.

mod custom;
mod emit;
mod hooks;
mod lang;
mod spec;
mod walk;

use std::collections::HashMap;

use sloplint_core::facts::FileFacts;
use sloplint_core::lang::Lang;
use sloplint_core::{CommentText, CustomHit, Pos};
use tree_sitter::{Node, Parser};

pub use custom::CustomRules;
pub use spec::spec;

/// Lazily configured parsers, one per language actually analyzed.
pub struct Parsers {
    parsers: HashMap<Lang, Parser>,
}

impl Default for Parsers {
    fn default() -> Self {
        Self::new()
    }
}

impl Parsers {
    /// Creates an empty pool; a language's parser is configured on first use.
    pub fn new() -> Parsers {
        Parsers {
            parsers: HashMap::new(),
        }
    }

    /// Parses one file, or `None` when the grammar cannot be loaded.
    fn parse(&mut self, lang: Lang, source: &str) -> Option<tree_sitter::Tree> {
        let language = spec(lang).language;
        let parser = self.parsers.entry(lang).or_insert_with(Parser::new);
        parser.set_language(&language).ok()?;
        parser.parse(source, None)
    }
}

/// One file to analyze, borrowed from the run's file table.
pub struct FileInput<'s> {
    /// The file's language.
    pub lang: Lang,
    /// The path relative to the project root, `/`-separated.
    pub path: &'s str,
    /// The BOM-stripped source text.
    pub source: &'s str,
    /// Whether the path matches a configured test glob.
    pub is_test_file: bool,
    /// Whether an OCaml `.ml` file has a sibling `.mli` in the analyzed set.
    pub has_interface: bool,
}

/// One analyzed file: facts, suppressible comments, parse-error position, custom hits.
pub struct Analysis<'s> {
    /// The facts every file rule reads; empty when the parse failed.
    pub facts: FileFacts<'s>,
    /// Comments carrying `sloplint-allow` text, for suppression parsing.
    pub comments: Vec<CommentText<'s>>,
    /// The first error or missing node when the parse failed.
    pub parse_error: Option<Pos>,
    /// Custom-rule query hits; empty when the parse failed.
    pub custom_hits: Vec<CustomHit<'s>>,
}

/// Parses and walks one file into facts, comments, and custom-rule hits.
///
/// A failed parse yields empty facts, the collected comments, and the first
/// error position, so one file's findings never depend on other files.
pub fn analyze<'s>(
    parsers: &mut Parsers,
    input: &FileInput<'s>,
    custom: &CustomRules,
) -> Analysis<'s> {
    let Some(tree) = parsers.parse(input.lang, input.source) else {
        return Analysis {
            facts: walk::empty_facts(input),
            comments: Vec::new(),
            parse_error: None,
            custom_hits: Vec::new(),
        };
    };
    let language_spec = spec(input.lang);
    let root = tree.root_node();
    if root.has_error() {
        let comments = language_spec.walk_comments(input, root).comments;
        return Analysis {
            facts: walk::empty_facts(input),
            comments,
            parse_error: first_error_pos(root),
            custom_hits: Vec::new(),
        };
    }
    let walked = language_spec.walk(input, root);
    let custom_hits = custom.run(&tree, input.source, input.lang);
    Analysis {
        facts: walked.facts,
        comments: walked.comments,
        parse_error: None,
        custom_hits,
    }
}

/// The position of the first error or missing node, cursor DFS order.
fn first_error_pos(root: Node<'_>) -> Option<Pos> {
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            return Some(hooks::pos_of(node));
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return None;
            }
        }
    }
}
