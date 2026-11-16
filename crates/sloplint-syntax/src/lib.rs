mod binds;
mod custom;
mod cx;
mod emit;
mod generated;
mod header_comments;
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
    /// True when `facts` and `comments` were walked on a rescued parse: the
    /// source's ERROR regions were blanked to recover a clean tree, so rule
    /// coverage is partial. `parse_error` stays `Some` in that case.
    pub partial: bool,
    /// 1-based inclusive line ranges of the top-level constructs that
    /// contained an excised ERROR region. Only set when `partial`: findings
    /// inside them are judgments on truncated code and must be dropped.
    /// Sorted and merged for `is_tainted`'s binary search.
    pub tainted: Vec<(u32, u32)>,
    pub custom_hits: Vec<CustomHit<'s>>,
    pub custom_capped: Vec<&'s str>,
}

impl Analysis<'_> {
    /// Whether `line` lies inside a construct that rescue excised. The
    /// ranges are sorted and merged at construction, so the check is a
    /// binary search rather than a scan per finding.
    pub fn is_tainted(&self, line: u32) -> bool {
        let after = self.tainted.partition_point(|&(_, last)| last < line);
        self.tainted
            .get(after)
            .is_some_and(|&(first, _)| first <= line)
    }
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
    if let Some(rescued) = rescue_analysis(parsers, input, input.lang, &tree, custom) {
        return rescued;
    }
    let hooks = lang::hooks_for(input.lang, input.path, parsers.ocaml_interfaces);
    let comments = spec(input.lang).walk_comments(hooks, input, root).comments;
    Analysis {
        facts: walk::empty_facts(input, input.lang),
        comments,
        parse_error: Some(first_error_pos(root).unwrap_or_else(|| root_end_pos(root))),
        partial: false,
        tainted: Vec::new(),
        custom_hits: Vec::new(),
        custom_capped: Vec::new(),
    }
}

/// Rounds of excise-and-reparse the rescue gets before giving up. Each round
/// can unmask a construct the previous ERROR region was hiding.
const MAX_RESCUE_ROUNDS: usize = 3;

/// Blank every top-level ERROR node's bytes (keeping `\n` so positions stay
/// stable) and reparse. A single unparseable construct — an unknown macro
/// qualifier like `CAMLprim`/`g_autoptr` in declaration position, a stray
/// brace inside `#if 0` — must not sink the whole file's rule coverage. When
/// a round produces a clean tree, the file is analyzed normally and the
/// original error position is still reported via `parse_error` with
/// `partial` set. Excised spans hold no nodes, so every slice the walk hands
/// out still names real source bytes.
fn rescue_analysis<'s>(
    parsers: &mut Parsers<'_>,
    input: &FileInput<'s>,
    lang: Lang,
    first_tree: &Tree,
    custom: &'s CustomRules,
) -> Option<Analysis<'s>> {
    let first_pos = first_error_pos(first_tree.root_node())?;
    let mut text = input.source.as_bytes().to_vec();
    let mut tree = first_tree.clone();
    let mut tainted = Vec::new();
    for _ in 0..MAX_RESCUE_ROUNDS {
        let mut errors = Vec::new();
        error_nodes(tree.root_node(), &mut errors);
        if errors.is_empty() {
            return None;
        }
        for &node in &errors {
            tainted.push(tainted_lines(tree.root_node(), node));
            for byte in &mut text[node.start_byte()..node.end_byte()] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
        }
        let source = std::str::from_utf8(&text).ok()?;
        tree = parsers.parse(lang, source);
        if !tree.root_node().has_error() {
            // A program excised to nothing keeps the old contract: the file
            // is judged by its syntax error alone.
            if tree.root_node().named_child_count() == 0 {
                return None;
            }
            let mut analysis = clean_analysis(parsers, input, lang, &tree, custom);
            merge_comments(parsers, input, first_tree, &mut analysis);
            analysis.parse_error = Some(first_pos);
            analysis.partial = true;
            analysis.tainted = merge_ranges(tainted);
            return Some(analysis);
        }
    }
    None
}

/// Collect every ERROR node without recursion: a deeply nested syntax error
/// must not grow one stack frame per level like the tree walk it replaces.
fn error_nodes<'t>(root: Node<'t>, errors: &mut Vec<Node<'t>>) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.is_error() {
            errors.push(node);
            continue;
        }
        if !node.has_error() {
            continue;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// Sort and merge the tainted ranges so `Analysis::is_tainted` can binary
/// search them: one entry per ERROR node would otherwise cost a linear scan
/// per finding. Adjacent ranges merge too — excision drops everything in
/// between anyway.
fn merge_ranges(mut ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    ranges.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for (first, last) in ranges {
        match merged.last_mut() {
            Some((_, prev_last)) if first <= prev_last.saturating_add(1) => {
                *prev_last = (*prev_last).max(last);
            }
            _ => merged.push((first, last)),
        }
    }
    merged
}

/// The line range of `node`'s top-level enclosing construct: everything the
/// excision truncated, where findings would judge edited-out code.
fn tainted_lines(root: Node<'_>, node: Node<'_>) -> (u32, u32) {
    let mut top = node;
    while let Some(parent) = top.parent() {
        if parent == root {
            break;
        }
        top = parent;
    }
    (
        hooks::narrow(top.start_position().row) + 1,
        hooks::narrow(top.end_position().row) + 1,
    )
}

/// A comment inside an excised region — say a `sloplint-allow-file` directive
/// in an unsupported construct — vanishes from the rescued tree. Re-walk the
/// original parse's comments and union them in so suppressions keep working.
fn merge_comments<'s>(
    parsers: &Parsers<'_>,
    input: &FileInput<'s>,
    first_tree: &Tree,
    analysis: &mut Analysis<'s>,
) {
    let hooks = lang::hooks_for(input.lang, input.path, parsers.ocaml_interfaces);
    let original = spec(input.lang)
        .walk_comments(hooks, input, first_tree.root_node())
        .comments;
    let mut seen: HashSet<(u32, &str)> = analysis
        .comments
        .iter()
        .map(|comment| (comment.start_line, comment.text))
        .collect();
    for comment in original {
        if seen.insert((comment.start_line, comment.text)) {
            analysis.comments.push(comment);
        }
    }
    analysis.comments.sort_by_key(|comment| comment.start_line);
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
        partial: false,
        tainted: Vec::new(),
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
