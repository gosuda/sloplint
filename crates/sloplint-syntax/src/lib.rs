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
use std::ops::ControlFlow;
use std::ops::Range;
use std::time::{Duration, Instant};

use sloplint_core::facts::FileFacts;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_core::{CommentText, CustomHit, Pos};
use tree_sitter::{Node, ParseOptions, ParseState, Parser, Point, Tree};

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
/// Wall-clock budget shared by every `parse()` call behind one file's
/// analysis: the first parse, each fallback retry, and each rescue re-parse
/// draw from the same deadline, so one pathological file can hold a worker
/// (and its growing parse stack) for at most this long. Past the deadline
/// the parse is abandoned and the file is reported as a single
/// `meta/parse-error`, exactly like a file no grammar accepts.
///
/// Why a timeout: the vendored Lean grammar (18,895 states) does not just
/// parse some tactic-heavy files slowly — it diverges. One 71 KB mathlib
/// file (`Mathlib/Data/Set/Card.lean`) grows a single worker past 13 GB RSS
/// without finishing in 5 minutes, and a 67 KB sibling needs 11 GB RSS /
/// 30 GB of address space in 4 minutes under the reference tree-sitter CLI.
/// The stack nobody retains is the problem: `Parsers` drops every tree
/// after the file's findings are produced, but a runaway parse accumulates
/// gigabytes *during* the parse, and the default 80-way parallelism
/// multiplies that into the hundreds of GB the baseline measured before
/// the OOM-killer (or the address-space cap) aborted the run with no JSON.
/// A convergent parse returns the same tree whether or not a progress
/// callback is installed — returning `Continue` never changes parsing — so
/// files that finish inside the budget produce byte-identical findings.
/// The slowest convergent Lean file observed takes under half a second of
/// parser CPU, over twenty times less than this budget (see the commit
/// message), so the budget only ever fires on files the unmodified code
/// cannot finish in practice.
const PARSE_BUDGET: Duration = Duration::from_secs(10);

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

    /// Parses `source`, abandoning the parse once `deadline` passes. A
    /// `None` return means the budget ran out — never a missing language,
    /// which is a panic as before — so callers treat it as an unparseable
    /// file, not as an error.
    fn parse(&mut self, lang: Lang, source: &str, deadline: Instant) -> Option<Tree> {
        // A spent budget starts no parse: the runtime first asks the
        // callback after a hundred operations, so a short retry past the
        // deadline would otherwise still finish and outlive the budget.
        if Instant::now() >= deadline {
            return None;
        }
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
        let bytes = source.as_bytes();
        let mut read = |offset: usize, _: Point| -> &[u8] {
            if offset < bytes.len() {
                &bytes[offset..]
            } else {
                &[]
            }
        };
        // Checked by the runtime every hundred parse operations; answering
        // `Continue` leaves parsing untouched, so convergent parses return
        // the same tree as without a callback.
        let mut cancel = |_: &ParseState| {
            if Instant::now() >= deadline {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        let options = ParseOptions::new().progress_callback(&mut cancel);
        let result = parser.parse_with_options(&mut read, None, Some(options));
        if result.is_none() {
            // A cancelled parse leaves the parser mid-flight: the runtime
            // would resume the abandoned stack on the next parse call, and
            // this parser is reused for every file on the worker thread.
            // Reset so the budget actually frees the stack and the next
            // file starts from a clean state instead of inheriting a
            // multi-GB resume plus another file's bytes.
            parser.reset();
        }
        result
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
    /// True when the parse was abandoned past the budget instead of failing
    /// on a syntax error: there is no tree, so `facts` are empty, there are
    /// no comments, and `parse_error` needs no error position. The pipeline
    /// reports this as a timeout `meta/parse-error`, not a syntax one.
    pub timed_out: bool,
    pub custom_hits: Vec<CustomHit<'s>>,
    pub custom_capped: Vec<&'s str>,
}

/// Analysis for a file whose parse ran past the shared budget: no tree, so
/// no facts, no comments, no rescue — just the position the timeout finding
/// is anchored at. The file keeps exactly one `meta/parse-error` finding,
/// the same shape as a file no grammar accepts.
fn timeout_analysis<'s>(input: &FileInput<'s>) -> Analysis<'s> {
    Analysis {
        facts: walk::empty_facts(input, input.lang),
        comments: Vec::new(),
        parse_error: Some(Pos { line: 1, byte: 0 }),
        partial: false,
        tainted: TaintedLines::default(),
        timed_out: true,
        custom_hits: Vec::new(),
        custom_capped: Vec::new(),
    }
}

pub fn analyze<'s>(
    parsers: &mut Parsers<'_>,
    input: &FileInput<'s>,
    custom: &'s CustomRules,
) -> Analysis<'s> {
    // One deadline for the whole file: the first parse, every fallback
    // retry, and every rescue re-parse share it, so a divergent file costs
    // at most one budget no matter how many rounds it would need.
    let deadline = Instant::now() + PARSE_BUDGET;
    let Some(tree) = parsers.parse(input.lang, input.source, deadline) else {
        return timeout_analysis(input);
    };
    if !tree.root_node().has_error() {
        return clean_analysis(parsers, input, input.lang, &tree, custom);
    }
    retry_analysis(parsers, input, &tree, custom, deadline)
}

/// The first parse failed: try each fallback grammar, then the rescue, and
/// only then report the first tree's syntax error. A retry the deadline
/// cancels ends the file as a timeout: an unfinished fallback leaves open
/// whether the source has a syntax error at all, so the first grammar's
/// error position must not be reported as one.
fn retry_analysis<'s>(
    parsers: &mut Parsers<'_>,
    input: &FileInput<'s>,
    tree: &Tree,
    custom: &'s CustomRules,
    deadline: Instant,
) -> Analysis<'s> {
    for &fallback in input.lang.parse_fallbacks(input.path) {
        let Some(retried) = parsers.parse(fallback, input.source, deadline) else {
            return timeout_analysis(input);
        };
        if !retried.root_node().has_error() {
            return clean_analysis(parsers, input, fallback, &retried, custom);
        }
    }
    if let Some(rescued) = rescue_analysis(parsers, input, input.lang, tree, custom, deadline) {
        return rescued;
    }
    let root = tree.root_node();
    let hooks = lang::hooks_for(input.lang, input.path, parsers.ocaml_interfaces);
    let comments = spec(input.lang).walk_comments(hooks, input, root).comments;
    Analysis {
        facts: walk::empty_facts(input, input.lang),
        comments,
        parse_error: Some(first_error_pos(root).unwrap_or_else(|| root_end_pos(root))),
        partial: false,
        tainted: TaintedLines::default(),
        timed_out: false,
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
/// out still names real source bytes. `None` leaves the file to its syntax
/// error; a re-parse the deadline cancels returns the timeout analysis.
fn rescue_analysis<'s>(
    parsers: &mut Parsers<'_>,
    input: &FileInput<'s>,
    lang: Lang,
    first_tree: &Tree,
    custom: &'s CustomRules,
    deadline: Instant,
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
        // A re-parse past the shared deadline ends the file as a timeout:
        // without a fresh tree there is nothing to recover, retrying would
        // just burn another budget on a file already known to diverge, and
        // the budget, not the source, decided that the rescue failed.
        let Some(reparsed) = parsers.parse(lang, source, deadline) else {
            return Some(timeout_analysis(input));
        };
        tree = reparsed;
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
        timed_out: false,
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

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;
    use std::time::Instant;

    use sloplint_core::Pos;
    use sloplint_core::lang::Lang;
    use sloplint_core::role::CodeRole;

    use super::{Analysis, CustomRules, FileInput, Parsers, retry_analysis, spec};

    static NO_CUSTOM: LazyLock<CustomRules> =
        LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

    /// Runs the retries of `source` against a deadline that has already
    /// passed, so every fallback and rescue parse meets a spent budget
    /// without the test depending on how long anything takes. The first
    /// parse has no deadline: it must finish, with errors, to reach them.
    fn retry_past_deadline<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&spec(Lang::C).language)
            .expect("C language loads");
        let tree = parser.parse(source, None).expect("tree");
        assert!(
            tree.root_node().has_error(),
            "{path}: the first parse must fail to reach the retries"
        );
        let input = FileInput {
            lang: Lang::C,
            path,
            source,
            role: CodeRole::Library,
        };
        retry_analysis(
            &mut Parsers::new(),
            &input,
            &tree,
            &NO_CUSTOM,
            Instant::now(),
        )
    }

    fn assert_timed_out(analysis: &Analysis<'_>, path: &str) {
        assert!(
            analysis.timed_out,
            "{path}: a parse the deadline cut must report a timeout, not a syntax error at {:?}",
            analysis.parse_error
        );
        assert_eq!(
            analysis.parse_error,
            Some(Pos { line: 1, byte: 0 }),
            "{path}: the timeout anchors at the file start"
        );
        assert!(!analysis.partial, "{path}: nothing was salvaged");
        assert!(
            analysis.facts.functions.is_empty(),
            "{path}: a timeout yields no facts"
        );
    }

    #[test]
    fn a_fallback_parse_past_the_deadline_reports_a_timeout() {
        let source: String = (0..40)
            .map(|n| {
                format!(
                    "class Widget{n} {{\npublic:\n    int value() const {{ return value_; }}\nprivate:\n    int value_ = {n};\n}};\n"
                )
            })
            .collect();
        assert_timed_out(&retry_past_deadline("widget.h", &source), "widget.h");
    }

    #[test]
    fn a_rescue_parse_past_the_deadline_reports_a_timeout() {
        let mut source: String = (0..40)
            .map(|n| format!("int value{n}(void) {{ return {n}; }}\n"))
            .collect();
        source.push_str("int broken = @@;\n");
        assert_timed_out(&retry_past_deadline("broken.c", &source), "broken.c");
    }
}
