//! Recover Lean commands when hidden missing nodes or an error root defeat span recovery.

mod boundaries;

use std::time::Instant;

use sloplint_core::Pos;
use tree_sitter::{Node, Tree};

use crate::hooks::{narrow, pos_of};
use crate::lang::lean::COMMENT_KINDS;
use crate::{
    Analysis, CustomRules, FileInput, Parsers, TaintedLines, clean_analysis, merge_comments,
    timeout_analysis,
};

/// Every successful round removes source bytes. The limit bounds cascades;
/// all reparses also share the caller's deadline.
const MAX_ROUNDS: usize = 8;

pub(super) fn rescue<'s>(
    parsers: &mut Parsers<'_>,
    input: &FileInput<'s>,
    first_tree: &Tree,
    custom: &'s CustomRules,
    deadline: Instant,
) -> Option<Analysis<'s>> {
    let first_pos = error_pos(first_tree.root_node());
    let mut source = RecoveryText::new(input.source);
    let mut tree = first_tree.clone();
    for _ in 0..MAX_ROUNDS {
        if !source.blank_failed_commands(tree.root_node()) {
            return None;
        }
        let Some(reparsed) = parsers.parse(input.lang, source.as_str(), deadline) else {
            return Some(timeout_analysis(input));
        };
        tree = reparsed;
        if tree.root_node().has_error() || source.has_opaque_command(tree.root_node()) {
            continue;
        }
        if !has_substantive_command(tree.root_node()) {
            return None;
        }
        let mut analysis = clean_analysis(parsers, input, input.lang, &tree, custom);
        merge_comments(parsers, input, first_tree, &mut analysis);
        analysis.parse_error = Some(first_pos.unwrap_or_else(|| source.first_removed_pos()));
        analysis.partial = true;
        analysis.tainted = TaintedLines::new(source.tainted);
        return Some(analysis);
    }
    None
}

/// Blanking whole rows preserves UTF-8, byte offsets, and newline positions.
/// The bytes and taint belong to one recovery attempt, including fragment
/// recovery after earlier rounds have removed complete commands.
struct RecoveryText {
    bytes: Vec<u8>,
    line_starts: Vec<usize>,
    boundaries: boundaries::Boundaries,
    tainted: Vec<(u32, u32)>,
}

impl RecoveryText {
    fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            source
                .bytes()
                .enumerate()
                .filter_map(|(at, byte)| (byte == b'\n').then_some(at + 1)),
        );
        Self {
            bytes: source.as_bytes().to_vec(),
            line_starts,
            boundaries: boundaries::scan(source),
            tainted: Vec::new(),
        }
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes).expect("blanking complete rows preserves UTF-8")
    }

    fn blank_failed_commands(&mut self, root: Node<'_>) -> bool {
        if root.is_error() {
            return self.blank_error_root(root);
        }
        let mut cursor = root.walk();
        let mut bad: Vec<_> = root
            .children(&mut cursor)
            .filter(|child| {
                child.has_error()
                    || (is_command(*child) && !self.boundaries.is_top_level(child.start_byte()))
            })
            .map(row_span)
            .collect();
        bad.extend(gap_spans(root, &self.bytes, &self.line_starts));
        if bad.is_empty() {
            return self.blank_fragments(root);
        }
        let groups = command_groups(root, &self.boundaries);
        let mut changed = false;
        for (start, end) in bad {
            for &(group_start, group_end) in &groups {
                if start <= group_end && end >= group_start {
                    changed |=
                        self.blank_rows(group_start, self.failed_end(group_start, group_end));
                }
            }
            // A dropped-token gap may lie between all exposed nodes. Its
            // own rows still need removing even when no group overlaps it.
            changed |= self.blank_rows(start, self.failed_end(start, end));
        }
        changed
    }

    /// An error root can fold a later declaration into an expression fragment.
    /// Remove the first failed command, then let a fresh parse recover the tail.
    /// Original lexical boundaries keep literals and quotations out of that tail.
    fn blank_error_root(&mut self, root: Node<'_>) -> bool {
        let mut cursor = root.walk();
        let first_fragment = root
            .children(&mut cursor)
            .filter(|node| {
                !is_comment(*node)
                    && (!is_command(*node)
                        || node.has_error()
                        || !self.boundaries.is_top_level(node.start_byte()))
            })
            .map(|node| node.start_position().row)
            .min();
        let first_gap = gap_spans(root, &self.bytes, &self.line_starts)
            .into_iter()
            .map(|(start, _)| start)
            .min();
        let Some(start) = first_fragment.into_iter().chain(first_gap).min() else {
            return false;
        };
        let next_declaration = self.next_declaration_row(start);
        let next_command = root
            .children(&mut cursor)
            .filter(|node| {
                is_command(*node)
                    && !node.has_error()
                    && self.boundaries.is_top_level(node.start_byte())
            })
            .map(|node| node.start_position().row)
            .filter(|&row| row > start)
            .min();
        let end = next_declaration
            .into_iter()
            .chain(next_command)
            .min()
            .map_or_else(|| row_span(root).1, |row| row - 1);
        self.blank_rows(start, end)
    }

    fn blank_fragments(&mut self, root: Node<'_>) -> bool {
        let mut changed = false;
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if is_comment(child)
                || (is_command(child) && self.boundaries.is_top_level(child.start_byte()))
            {
                continue;
            }
            let (start, end) = row_span(child);
            changed |= self.blank_rows(start, end);
        }
        changed
    }

    fn has_opaque_command(&self, root: Node<'_>) -> bool {
        let mut cursor = root.walk();
        root.named_children(&mut cursor)
            .any(|node| is_command(node) && !self.boundaries.is_top_level(node.start_byte()))
    }

    fn next_declaration_row(&self, row: usize) -> Option<usize> {
        let next = self
            .boundaries
            .declarations
            .partition_point(|&at| at <= self.line_starts[row]);
        self.boundaries
            .declarations
            .get(next)
            .map(|&at| row_of(&self.line_starts, at))
    }

    fn failed_end(&self, start: usize, end: usize) -> usize {
        self.next_declaration_row(start)
            .filter(|&next| next <= end)
            .map_or(end, |next| next - 1)
    }

    fn blank_rows(&mut self, start: usize, end: usize) -> bool {
        let lo = self.line_starts[start];
        let hi = self
            .line_starts
            .get(end + 1)
            .copied()
            .unwrap_or(self.bytes.len());
        let mut changed = false;
        for byte in &mut self.bytes[lo..hi] {
            if !matches!(*byte, b' ' | b'\n') {
                *byte = b' ';
                changed = true;
            }
        }
        if changed {
            self.tainted.push((narrow(start) + 1, narrow(end) + 1));
        }
        changed
    }

    fn first_removed_pos(&self) -> Pos {
        let line = self
            .tainted
            .iter()
            .map(|&(start, _)| start)
            .min()
            .expect("a successful recovery removed at least one row");
        Pos {
            line,
            byte: narrow(self.line_starts[(line - 1) as usize]),
        }
    }
}

/// The grammar's `_command` alternatives, imports, and module preamble.
const COMMAND_KINDS: &[&str] = &[
    "moduleTk",
    "import",
    "prelude",
    "check",
    "eval",
    "print",
    "reduce",
    "namespace",
    "section",
    "end",
    "variable",
    "universe",
    "open",
    "export",
    "attribute_cmd",
    "initialize",
    "set_option",
    "deriving_cmd",
    "assert_cmd",
    "decl_doc_cmd",
    "grind_pattern_cmd",
    "alias_cmd",
    "notation_decl_cmd",
    "syntax_cmd",
    "macro_cmd",
    "macro_rules_cmd",
    "elab_rules_cmd",
    "elab_cmd",
    "omit",
    "register_cmd",
    "unif_hint_cmd",
    "declare_syntax_cat_cmd",
    "mutual",
    "hash_cmd",
    "custom_command",
    "run_meta_cmd",
    "include_cmd",
    "lake_cmd",
    "declaration",
];

fn is_command(node: Node<'_>) -> bool {
    COMMAND_KINDS.contains(&node.kind())
}

fn is_comment(node: Node<'_>) -> bool {
    COMMENT_KINDS.contains(&node.kind())
}

fn has_substantive_command(root: Node<'_>) -> bool {
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .any(|node| is_command(node) && !matches!(node.kind(), "moduleTk" | "prelude"))
}

/// A complete command starts a group even when indented. Other fragments
/// start groups at column zero, keeping flat `run_meta` bodies together.
fn command_groups(root: Node<'_>, boundaries: &boundaries::Boundaries) -> Vec<(usize, usize)> {
    let mut groups = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    let mut cursor = root.walk();
    for child in root
        .children(&mut cursor)
        .filter(|child| !is_comment(*child))
    {
        if (child.start_position().column == 0 || is_command(child))
            && boundaries.is_top_level(child.start_byte())
            && let Some(group) = current.take()
        {
            groups.push(group);
        }
        let (start, end) = row_span(child);
        current = Some(match current {
            Some((first, last)) => (first.min(start), last.max(end)),
            None => (start, end),
        });
    }
    groups.extend(current);
    groups
}

/// Node ends are exclusive. A node ending at column zero does not own any
/// bytes on that row; a zero-width error still belongs to its start row.
fn row_span(node: Node<'_>) -> (usize, usize) {
    let start = node.start_position().row;
    let end = node.end_position();
    (
        start,
        end.row - usize::from(end.column == 0 && end.row > start),
    )
}

/// Locate only the non-whitespace bytes dropped between direct children.
/// Neither adjacent healthy command belongs to the resulting row span.
fn gap_spans(root: Node<'_>, bytes: &[u8], line_starts: &[usize]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut edge = 0;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if let Some(span) = text_span(bytes, line_starts, edge, child.start_byte()) {
            spans.push(span);
        }
        edge = edge.max(child.end_byte());
    }
    if let Some(span) = text_span(bytes, line_starts, edge, bytes.len()) {
        spans.push(span);
    }
    spans
}

fn text_span(
    bytes: &[u8],
    line_starts: &[usize],
    from: usize,
    to: usize,
) -> Option<(usize, usize)> {
    let gap = &bytes[from..to];
    let first = gap.iter().position(|byte| !byte.is_ascii_whitespace())?;
    let last = gap
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .expect("a nonempty text span has a last byte");
    Some((
        row_of(line_starts, from + first),
        row_of(line_starts, from + last),
    ))
}

fn row_of(line_starts: &[usize], offset: usize) -> usize {
    line_starts.partition_point(|&start| start <= offset) - 1
}

/// Include zero-width parents of hidden MISSING nodes. The root itself
/// spans the file and is not an error site when it exposes child fragments.
fn error_pos(root: Node<'_>) -> Option<Pos> {
    let mut cursor = root.walk();
    if !cursor.goto_first_child() {
        return (root.is_error() || root.is_missing()).then(|| pos_of(root));
    }
    loop {
        let node = cursor.node();
        if node.is_error()
            || node.is_missing()
            || (node.byte_range().is_empty() && node.has_error())
        {
            return Some(pos_of(node));
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() || cursor.node() == root {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use sloplint_core::lang::Lang;
    use sloplint_core::role::CodeRole;
    use tree_sitter::{Parser, Tree};

    use super::{RecoveryText, gap_spans, rescue};
    use crate::{CustomRules, FileInput, Parsers, spec};

    fn lean_tree(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&spec(Lang::Lean).language)
            .expect("Lean language loads");
        parser.parse(source, None).expect("tree")
    }

    #[test]
    fn gaps_exclude_both_healthy_neighbors_and_trailing_whitespace() {
        let source = "import Lean\n  \nopen Lean Meta\n  \n";
        let tree = lean_tree(source);
        assert!(!tree.root_node().has_error());
        for row in [1, 3] {
            let mut text = RecoveryText::new(source);
            let at = text.line_starts[row];
            text.bytes[at..at + 2].copy_from_slice(b"xx");
            assert_eq!(
                gap_spans(tree.root_node(), &text.bytes, &text.line_starts),
                [(row, row)],
            );
            assert!(text.blank_failed_commands(tree.root_node()));
            assert_eq!(text.as_str(), source);
            assert_eq!(text.tainted, [(row as u32 + 1, row as u32 + 1)]);
        }
    }

    #[test]
    fn command_recovery_uses_the_original_deadline() {
        let source = "open Lean Meta\ntheorem mid : MyNat.le = (.≤.) := rfl\n";
        let tree = lean_tree(source);
        assert!(tree.root_node().has_error());
        let input = FileInput {
            lang: Lang::Lean,
            path: "broken.lean",
            source,
            role: CodeRole::Library,
        };
        let custom = CustomRules::compile(&[]).expect("empty rules compile");
        let analysis = rescue(&mut Parsers::new(), &input, &tree, &custom, Instant::now())
            .expect("the required reparse reports its spent budget");
        assert!(analysis.timed_out);
        assert!(!analysis.partial);
        assert!(analysis.facts.imports.is_empty());
    }
}
