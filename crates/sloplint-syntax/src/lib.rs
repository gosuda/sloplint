mod binds;
mod custom;
mod cx;
mod emit;
mod generated;
mod hooks;
mod lang;
mod shadow;
mod shapes;
mod spec;
mod walk;

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;

use sloplint_core::facts::FileFacts;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_core::{CommentText, CustomHit, Pos};
use tree_sitter::{Node, Parser, Tree};

use custom::CustomHits;
pub use custom::CustomRules;
pub use generated::is_generated;
pub use lang::shadowed_sources;
use spec::spec;

pub struct Parsers<'o> {
    parsers: HashMap<Lang, Parser>,
    ocaml_interfaces: Option<&'o HashSet<String>>,
}

impl Default for Parsers<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'o> Parsers<'o> {
    pub fn new() -> Self {
        Parsers {
            parsers: HashMap::new(),
            ocaml_interfaces: None,
        }
    }

    pub fn with_ocaml_interfaces(interfaces: &'o HashSet<String>) -> Self {
        Parsers {
            parsers: HashMap::new(),
            ocaml_interfaces: Some(interfaces),
        }
    }

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

pub struct FileInput<'s> {
    pub lang: Lang,
    /// The path relative to the project root, `/`-separated.
    pub path: &'s str,
    pub source: &'s str,
    pub role: CodeRole,
}

pub struct Analysis<'s> {
    pub facts: FileFacts<'s>,
    pub comments: Vec<CommentText<'s>>,
    pub parse_error: Option<Pos>,
    pub custom_hits: Vec<CustomHit<'s>>,
    pub custom_capped: Vec<&'s str>,
}

pub fn analyze<'s>(
    parsers: &mut Parsers<'_>,
    input: &FileInput<'s>,
    custom: &'s CustomRules,
) -> Analysis<'s> {
    let tree = parsers.parse(input.lang, input.source);
    let root = tree.root_node();
    if !root.has_error() {
        return clean_analysis(parsers, input, input.lang, &tree, custom);
    }
    for &fallback in input.lang.parse_fallbacks(input.path) {
        let retried = parsers.parse(fallback, input.source);
        if !retried.root_node().has_error() {
            return clean_analysis(parsers, input, fallback, &retried, custom);
        }
    }
    let hooks = lang::hooks_for(input.lang, input.path, parsers.ocaml_interfaces);
    let comments = spec(input.lang).walk_comments(hooks, input, root).comments;
    Analysis {
        facts: walk::empty_facts(input, input.lang),
        comments,
        parse_error: Some(first_error_pos(root).unwrap_or_else(|| root_end_pos(root))),
        custom_hits: Vec::new(),
        custom_capped: Vec::new(),
    }
}

fn clean_analysis<'s>(
    parsers: &Parsers<'_>,
    input: &FileInput<'s>,
    parsed_as: Lang,
    tree: &Tree,
    custom: &'s CustomRules,
) -> Analysis<'s> {
    let hooks = lang::hooks_for(parsed_as, input.path, parsers.ocaml_interfaces);
    let walked = spec(parsed_as).walk(hooks, input, tree.root_node());
    let CustomHits { hits, capped } = custom.run(tree, input.source, parsed_as);
    Analysis {
        facts: walked.facts,
        comments: walked.comments,
        parse_error: None,
        custom_hits: hits,
        custom_capped: capped,
    }
}

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

fn root_end_pos(root: Node<'_>) -> Pos {
    Pos {
        line: hooks::narrow(root.end_position().row) + 1,
        byte: hooks::narrow(root.end_byte()),
    }
}
