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
        let language = spec(lang).language.clone();
        let parser = self.parsers.entry(lang).or_default();
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

#[cfg(test)]
mod audit_propagation_tests {
    use super::{CustomRules, FileInput, Parsers, analyze};
    use sloplint_core::facts::Stmt;
    use sloplint_core::lang::Lang;

    fn audit_analyze<'s>(lang: Lang, path: &'s str, source: &'s str) -> super::Analysis<'s> {
        let mut parsers = Parsers::new();
        let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
        let input = FileInput {
            lang,
            path,
            source,
            is_test_file: false,
            has_interface: false,
        };
        analyze(&mut parsers, &input, &custom)
    }

    #[test]
    fn audit_propagation_error_bubble_emits_no_handler() {
        let cases = [
            (
                Lang::Zig,
                "propagate.zig",
                "fn load() !void { try work(); }\n",
            ),
            (
                Lang::Odin,
                "propagate.odin",
                "package sample\nload :: proc() -> (err: Error) { value := work() or_return; return; }\n",
            ),
        ];
        for (lang, path, source) in cases {
            let analysis = audit_analyze(lang, path, source);
            assert!(analysis.parse_error.is_none(), "{path} parses");
            assert!(
                analysis.facts.handlers.is_empty(),
                "{path} emits no handler"
            );
            let function = analysis
                .facts
                .functions
                .iter()
                .find(|f| f.name == "load")
                .expect("load function");
            assert_eq!(function.cognitive, 0, "{path} adds no cognitive");
            assert!(
                analysis
                    .facts
                    .calls
                    .iter()
                    .any(|c| c.callee.contains("work")),
                "{path} still traverses the propagated call"
            );
        }
    }

    #[test]
    fn audit_propagation_zig_catch_still_emits_handler() {
        let source = "fn read_value() !u32 {\n    return 1;\n}\n\npub fn load() void {\n    read_value() catch {};\n}\n";
        let analysis = audit_analyze(Lang::Zig, "handler.zig", source);
        assert!(analysis.parse_error.is_none(), "empty catch source parses");
        assert_eq!(analysis.facts.handlers.len(), 1, "empty catch is a handler");
        assert!(
            analysis.facts.handlers.iter().all(|h| h.stmts.is_empty()),
            "empty catch block stays a swallow handler"
        );
        assert!(
            analysis
                .facts
                .calls
                .iter()
                .any(|c| c.callee.contains("read_value")),
            "catch scrutinee still traverses calls"
        );
    }

    #[test]
    fn audit_propagation_zig_catch_retains_body_calls() {
        let body_call = "fn read_value() !u32 {\n    return 1;\n}\n\nfn log_fallback() void {}\n\npub fn load() void {\n    read_value() catch {\n        log_fallback();\n    };\n}\n";
        let analysis = audit_analyze(Lang::Zig, "body_call.zig", body_call);
        assert!(
            analysis.parse_error.is_none(),
            "catch with body call parses"
        );
        assert_eq!(
            analysis.facts.handlers.len(),
            1,
            "catch with body call is a handler"
        );
        assert!(
            analysis
                .facts
                .calls
                .iter()
                .any(|c| c.callee.contains("log_fallback")),
            "catch body still traverses calls"
        );
    }

    #[test]
    fn audit_zig_handler_rethrow_returns_bound_identifier() {
        let source = "fn read_value() !u32 {\n    return 1;\n}\n\npub fn load() !u32 {\n    return read_value() catch |e| {\n        return e;\n    };\n}\n";
        let analysis = audit_analyze(Lang::Zig, "rethrow.zig", source);
        assert!(
            analysis.parse_error.is_none(),
            "rethrow catch source parses without error"
        );
        assert_eq!(
            analysis.facts.handlers.len(),
            1,
            "rethrow catch emits exactly one handler"
        );
        let stmts = &analysis.facts.handlers[0].stmts;
        assert_eq!(
            stmts.len(),
            1,
            "rethrow catch body keeps its single return statement"
        );
        assert!(
            matches!(&stmts[0], Stmt::Rethrow),
            "return of the captured identifier classifies as Rethrow, got {:?}",
            stmts[0]
        );
    }

    #[test]
    fn audit_zig_handler_recovery_value_is_other() {
        let source = "fn read_value() !u32 {\n    return 1;\n}\n\nfn recover_value() u32 {\n    return 0;\n}\n\npub fn load() u32 {\n    return read_value() catch {\n        return recover_value();\n    };\n}\n";
        let analysis = audit_analyze(Lang::Zig, "handle.zig", source);
        assert!(
            analysis.parse_error.is_none(),
            "recovery catch source parses without error"
        );
        assert_eq!(
            analysis.facts.handlers.len(),
            1,
            "recovery catch emits exactly one handler"
        );
        let stmts = &analysis.facts.handlers[0].stmts;
        assert_eq!(
            stmts.len(),
            1,
            "recovery catch body keeps its single return statement"
        );
        assert!(
            matches!(&stmts[0], Stmt::Other),
            "return of a recovery value classifies as Other, got {:?}",
            stmts[0]
        );
    }

    #[test]
    fn audit_zig_handler_body_call_is_call() {
        let source = "fn read_value() !u32 {\n    return 1;\n}\n\nfn log_fallback() void {}\n\npub fn load() void {\n    read_value() catch {\n        log_fallback();\n    };\n}\n";
        let analysis = audit_analyze(Lang::Zig, "handler_call.zig", source);
        assert!(
            analysis.parse_error.is_none(),
            "catch with body call parses without error"
        );
        assert_eq!(
            analysis.facts.handlers.len(),
            1,
            "catch with body call emits exactly one handler"
        );
        let stmts = &analysis.facts.handlers[0].stmts;
        assert_eq!(
            stmts.len(),
            1,
            "catch body call keeps its single call statement"
        );
        assert!(
            matches!(&stmts[0], Stmt::Call { callee, .. } if callee.contains("log_fallback")),
            "catch body call retains its callee, got {:?}",
            stmts[0]
        );
    }

    #[test]
    fn audit_zig_handler_empty_and_comment_only_stay_empty() {
        let empty = "fn read_value() !u32 {\n    return 1;\n}\n\npub fn load() void {\n    read_value() catch {};\n}\n";
        let analysis = audit_analyze(Lang::Zig, "empty.zig", empty);
        assert!(
            analysis.parse_error.is_none(),
            "empty catch source parses without error"
        );
        assert_eq!(
            analysis.facts.handlers.len(),
            1,
            "empty catch emits exactly one handler"
        );
        assert!(
            analysis.facts.handlers[0].stmts.is_empty(),
            "empty catch block stays empty, got {:?}",
            analysis.facts.handlers[0].stmts
        );
        let commented = "fn read_value() !u32 {\n    return 1;\n}\n\npub fn load() void {\n    read_value() catch {\n        // nothing to do\n    };\n}\n";
        let analysis = audit_analyze(Lang::Zig, "comment_only.zig", commented);
        assert!(
            analysis.parse_error.is_none(),
            "comment-only catch source parses without error"
        );
        assert_eq!(
            analysis.facts.handlers.len(),
            1,
            "comment-only catch emits exactly one handler"
        );
        assert!(
            analysis.facts.handlers[0].stmts.is_empty(),
            "comment-only catch block stays empty, got {:?}",
            analysis.facts.handlers[0].stmts
        );
    }
}
