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
mod tainted;
mod walk;

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::ops::Range;

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
pub use tainted::TaintedLines;

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
    pub tainted: TaintedLines,
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
    if let Some(rescued) = rescue_analysis(parsers, input, input.lang, &tree, custom) {
        return rescued;
    }
    if input.lang == Lang::Lean
        && let Some(rescued) = lean_command_rescue(parsers, input, &tree, custom)
    {
        return rescued;
    }
    let hooks = lang::hooks_for(input.lang, input.path, parsers.ocaml_interfaces);
    let comments = spec(input.lang).walk_comments(hooks, input, root).comments;
    Analysis {
        facts: walk::empty_facts(input, input.lang),
        comments,
        parse_error: Some(first_error_pos(root).unwrap_or_else(|| root_end_pos(root))),
        partial: false,
        tainted: TaintedLines::default(),
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
        let errors = error_regions(tree.root_node());
        if errors.is_empty() {
            return None;
        }
        for region in errors {
            tainted.push(region.lines);
            for byte in &mut text[region.bytes] {
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
            analysis.tainted = TaintedLines::new(tainted);
            return Some(analysis);
        }
    }
    None
}

struct ErrorRegion {
    bytes: Range<usize>,
    lines: (u32, u32),
}

fn error_regions(root: Node<'_>) -> Vec<ErrorRegion> {
    let mut errors = Vec::new();
    let mut stack = vec![(root, line_range(root))];
    while let Some((node, lines)) = stack.pop() {
        if node.is_error() {
            errors.push(ErrorRegion {
                bytes: node.byte_range(),
                lines,
            });
            continue;
        }
        if !node.has_error() {
            continue;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.has_error() {
                let lines = if node == root {
                    line_range(child)
                } else {
                    lines
                };
                stack.push((child, lines));
            }
        }
    }
    errors
}

fn line_range(node: Node<'_>) -> (u32, u32) {
    (
        hooks::narrow(node.start_position().row) + 1,
        hooks::narrow(node.end_position().row) + 1,
    )
}

/// Rounds of blank-and-reparse the Lean command rescue gets before giving
/// up. Each round blanks at least one new command, so the loop always makes
/// progress; the cap is a backstop for pathological cascades. One bad
/// command can poison the layout state that follows it — blanking the first
/// `example` in `BatteriesTest/array_scan.lean` newly breaks the next — so
/// one round per poisoned command is sometimes needed, more than
/// [`MAX_RESCUE_ROUNDS`].
const LEAN_RESCUE_ROUNDS: usize = 8;

/// Per-command recovery for Lean, after [`rescue_analysis`] gives up.
///
/// The generic rescue blanks reachable `ERROR` spans and needs at least one
/// reachable `ERROR` node. Two Lean shapes defeat it: a bad command that
/// recovery wraps in a file-spanning `ERROR` node — even an `ERROR` root, so
/// blanking the span redacts the whole file — and a hidden `MISSING` node
/// with no reachable `ERROR` at all (`MyNat/Inequality.lean`: `(.≤.)`
/// leaves two zero-width recovery sites while the file is otherwise clean).
///
/// Lean lays commands out flat with every command head at column 0, so the
/// file is partitioned into command groups — maximal runs of top-level
/// nodes starting at a column-0 node — and every group overlapping a
/// top-level error span is blanked. Groups are recomputed from each reparse
/// because blanking one command can unmask, move, or newly break the next;
/// only a clean reparse is ever analyzed, exactly like the generic rescue.
/// The reported position is the first error site in document order,
/// including zero-width recovery sites, so a hidden `MISSING` points at its
/// command instead of the end of the file.
fn lean_command_rescue<'s>(
    parsers: &mut Parsers<'_>,
    input: &FileInput<'s>,
    first_tree: &Tree,
    custom: &'s CustomRules,
) -> Option<Analysis<'s>> {
    let first_pos = lean_error_pos(first_tree.root_node());
    let mut text = input.source.as_bytes().to_vec();
    let mut line_starts = vec![0usize];
    for (at, &byte) in text.iter().enumerate() {
        if byte == b'\n' {
            line_starts.push(at + 1);
        }
    }
    let mut blanked: HashSet<usize> = HashSet::new();
    let mut tainted: Vec<(u32, u32)> = Vec::new();
    let mut tree = first_tree.clone();
    for _ in 0..LEAN_RESCUE_ROUNDS {
        let (groups, bad) = {
            let root = tree.root_node();
            let mut bad = lean_top_errors(root);
            bad.extend(lean_gap_spans(root, &text, &line_starts));
            (lean_command_groups(root), bad)
        };
        if groups.is_empty() || bad.is_empty() {
            return None;
        }
        let mut fresh = false;
        for &(start, end) in &groups {
            if !bad.iter().any(|&(a, b)| a <= end && b >= start) {
                continue;
            }
            tainted.push((hooks::narrow(start) + 1, hooks::narrow(end) + 1));
            for row in start..=end {
                if blanked.insert(row) {
                    fresh = true;
                    let (lo, hi) = (
                        *line_starts.get(row).unwrap_or(&text.len()),
                        line_starts.get(row + 1).copied().unwrap_or(text.len()),
                    );
                    for byte in &mut text[lo..hi] {
                        if *byte != b'\n' {
                            *byte = b' ';
                        }
                    }
                }
            }
        }
        if !fresh {
            return None;
        }
        let source = std::str::from_utf8(&text).ok()?;
        tree = parsers.parse(input.lang, source);
        if !tree.root_node().has_error() {
            // A program excised to nothing keeps the old contract: the file
            // is judged by its syntax error alone.
            if tree.root_node().named_child_count() == 0 {
                return None;
            }
            let mut analysis = clean_analysis(parsers, input, input.lang, &tree, custom);
            merge_comments(parsers, input, first_tree, &mut analysis);
            analysis.parse_error =
                Some(first_pos.unwrap_or_else(|| lean_group_pos(&text, &line_starts, &tainted)));
            analysis.partial = true;
            analysis.tainted = TaintedLines::new(tainted);
            return Some(analysis);
        }
    }
    None
}

/// Partition a Lean parse into top-level command row ranges (0-based,
/// inclusive). A group starts at each column-0 child; indented lines belong
/// to the preceding group, which keeps a `run_meta` do-block and its flat
/// interior lines together. Works on a `module` root and on an `ERROR`
/// root, whose children are the recovered command fragments.
fn lean_command_groups(root: Node<'_>) -> Vec<(usize, usize)> {
    let mut groups = Vec::new();
    let mut cursor = root.walk();
    let mut start: Option<usize> = None;
    let mut end = 0;
    let mut first = true;
    for child in root.children(&mut cursor) {
        if !first
            && child.start_position().column == 0
            && let Some(open) = start.take()
        {
            groups.push((open, end));
        }
        let child_start = child.start_position().row;
        let child_end = child.end_position().row;
        (start, end) = match start {
            Some(open) => (Some(open.min(child_start)), end.max(child_end)),
            None => (Some(child_start), child_end),
        };
        first = false;
    }
    if let Some(open) = start {
        groups.push((open, end));
    }
    groups
}

/// Row spans (0-based, inclusive) of the root's direct children that carry
/// an error. A span test against command groups keeps the blanking
/// command-granular even when an `ERROR` node straddles group boundaries.
fn lean_top_errors(root: Node<'_>) -> Vec<(usize, usize)> {
    let mut bad = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.has_error() {
            bad.push((child.start_position().row, child.end_position().row));
        }
    }
    bad
}

/// Row spans (0-based, inclusive) holding non-whitespace the parse dropped
/// between the root's direct children (or before the first / after the
/// last): tokens error recovery skipped without leaving a reachable `ERROR`
/// or a `MISSING` in the s-expression. The group holding the gap is where
/// the unparseable command sits.
fn lean_gap_spans(root: Node<'_>, text: &[u8], line_starts: &[usize]) -> Vec<(usize, usize)> {
    let mut cursor = root.walk();
    let kids: Vec<Node<'_>> = root.children(&mut cursor).collect();
    let mut spans = Vec::new();
    let mut edge = 0;
    for child in &kids {
        let start = child.start_byte();
        if has_text(&text[edge..start.min(text.len())]) {
            spans.push((row_of(line_starts, edge), row_of(line_starts, start)));
        }
        edge = child.end_byte().max(edge);
    }
    if has_text(&text[edge.min(text.len())..]) {
        spans.push((row_of(line_starts, edge), row_of(line_starts, text.len())));
    }
    spans
}

/// Whether a gap holds anything but ASCII whitespace (comments are child
/// nodes, never gap content).
fn has_text(gap: &[u8]) -> bool {
    gap.iter().any(|&byte| !byte.is_ascii_whitespace())
}

/// 0-based row containing `offset`, via the file's line starts.
fn row_of(line_starts: &[usize], offset: usize) -> usize {
    line_starts
        .partition_point(|&start| start <= offset)
        .saturating_sub(1)
}
/// First error site in document order, like [`first_error_pos`] but also
/// matching zero-width nodes with an error beneath them: a hidden `MISSING`
/// node is invisible to the cursor, while its zero-width parent is
/// reachable and marks the recovery site (`MyNat/Inequality.lean`). The
/// root itself is never the site: on an `ERROR` root it spans the whole
/// file, so the search starts at its children.
fn lean_error_pos(root: Node<'_>) -> Option<Pos> {
    let mut cursor = root.walk();
    if !cursor.goto_first_child() {
        let empty = root.byte_range().is_empty() && root.has_error();
        return (root.is_error() || root.is_missing() || empty).then(|| hooks::pos_of(root));
    }
    let root_id = root.id();
    loop {
        let node = cursor.node();
        if node.id() != root_id {
            if node.is_error() || node.is_missing() {
                return Some(hooks::pos_of(node));
            }
            if node.byte_range().is_empty() && node.has_error() {
                return Some(hooks::pos_of(node));
            }
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

/// Fallback parse-error position when no error site is reachable at all:
/// the start of the first blanked command. Points at the bad command
/// instead of the end of the file.
fn lean_group_pos(text: &[u8], line_starts: &[usize], tainted: &[(u32, u32)]) -> Pos {
    let line = tainted
        .iter()
        .map(|&(start, _)| start)
        .min()
        .unwrap_or(1)
        .max(1);
    let byte = line_starts
        .get(
            usize::try_from(line)
                .unwrap_or(usize::MAX)
                .saturating_sub(1),
        )
        .copied()
        .unwrap_or(text.len());
    Pos {
        line,
        byte: hooks::narrow(byte),
    }
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
        tainted: TaintedLines::default(),
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
