//! Turns tree-sitter parse trees into sloplint-core file facts.

mod custom;
mod emit;
mod hooks;
mod lang;
mod spec;
mod walk;

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use sloplint_core::facts::FileFacts;
use sloplint_core::lang::Lang;
use sloplint_core::{CommentText, CustomHit, Pos};
use tree_sitter::{Node, Parser, Tree};

pub use custom::CustomRules;
use spec::spec;

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

    /// Parses one file.
    ///
    /// # Panics
    ///
    /// Panics when the pinned grammar cannot be loaded into the parser or the parser
    /// returns no tree. Both are build faults, like the grammar-drift panic in
    /// [`spec`], and never depend on the file: a quiet empty analysis would look
    /// exactly like a clean file. Syntax errors in the source are not faults; they
    /// yield a tree with error nodes.
    fn parse(&mut self, lang: Lang, source: &str) -> tree_sitter::Tree {
        let parser = match self.parsers.entry(lang) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let language = spec(lang).language.clone();
                let mut parser = Parser::new();
                parser
                    .set_language(&language)
                    .unwrap_or_else(|error| panic!("grammar for {lang:?} failed to load: {error}"));
                entry.insert(parser)
            }
        };
        parser
            .parse(source, None)
            .unwrap_or_else(|| panic!("tree-sitter returned no tree for {lang:?}"))
    }
}

/// One file to analyze, borrowed from the run's file table.
pub struct FileInput<'s> {
    /// The file's routed language, chosen from its name.
    ///
    /// It decides which grammar parses first and the path-only entry-file flag. The grammar
    /// that produces the facts can differ; see [`Lang::parse_fallbacks`] and
    /// [`FileFacts::lang`].
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
/// A source with syntax errors yields empty facts, the collected comments, and
/// the first error position, so one file's findings never depend on other files.
///
/// A file whose language has [`Lang::parse_fallbacks`] (a `.h` file routed to C
/// has C++ and then Objective-C) is parsed again with each grammar in that order
/// while every earlier parse has errors, and the first error-free tree wins. That
/// tree is walked with its grammar's roles and hooks and queried with its custom
/// rules, and the facts carry its language, so every language-keyed rule reads the
/// grammar that produced them. When every parse fails, the result is the first
/// parse's error and comments, and the empty facts keep `input.lang`.
///
/// Custom hits borrow the rules table for capture names and the file source
/// for capture text, so `custom` shares the `'s` lifetime of the input.
///
/// # Panics
///
/// Panics when the pinned grammar for `input.lang` or one of its fallbacks cannot
/// be loaded or the parser returns no tree. Both are build faults (grammar or ABI
/// drift) that never depend on the source, so they stop the run instead of
/// passing as clean.
pub fn analyze<'s>(
    parsers: &mut Parsers,
    input: &FileInput<'s>,
    custom: &'s CustomRules,
) -> Analysis<'s> {
    let tree = parsers.parse(input.lang, input.source);
    let root = tree.root_node();
    if !root.has_error() {
        return clean_analysis(input, input.lang, &tree, custom);
    }
    for &fallback in input.lang.parse_fallbacks(input.path) {
        let retried = parsers.parse(fallback, input.source);
        if !retried.root_node().has_error() {
            return clean_analysis(input, fallback, &retried, custom);
        }
    }
    let comments = spec(input.lang).walk_comments(input, root).comments;
    Analysis {
        facts: walk::empty_facts(input, input.lang),
        comments,
        parse_error: Some(first_error_pos(root).unwrap_or_else(|| root_end_pos(root))),
        custom_hits: Vec::new(),
    }
}

/// Walks and queries an error-free `tree` that the `parsed_as` grammar produced.
///
/// The tree decides the role table, hooks, custom rules, and the language of the
/// facts, so all four come from `parsed_as`.
fn clean_analysis<'s>(
    input: &FileInput<'s>,
    parsed_as: Lang,
    tree: &Tree,
    custom: &'s CustomRules,
) -> Analysis<'s> {
    let walked = spec(parsed_as).walk(input, tree.root_node());
    let custom_hits = custom.run(tree, input.source, parsed_as);
    Analysis {
        facts: walked.facts,
        comments: walked.comments,
        parse_error: None,
        custom_hits,
    }
}

/// The position of the first error or missing node, cursor DFS order, or
/// [`None`] when recovery hides every fault in the tree.
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

/// The 1-based line and byte offset where `root` ends. Recovery can hide every
/// error and missing node yet still flag `has_error`; the tree's end then
/// stands in for the fault so a failed parse always reports a position.
fn root_end_pos(root: Node<'_>) -> Pos {
    Pos {
        line: hooks::narrow(root.end_position().row) + 1,
        byte: hooks::narrow(root.end_byte()),
    }
}
