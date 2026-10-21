use sloplint_core::facts::{AmbientKind, Arg, ArgShape, CallKind, Pos, WildcardArm};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, child_by_field, first_named_child,
    first_named_child_matching, named_children, narrow, pos_of, preceding_comments,
    string_arg_shape, text, trimmed,
};
use crate::spec::Role;

/// Role table mapping Bash node kinds to walker roles.
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_definition", Role::Function),
    ("if_statement", Role::If),
    ("else_clause", Role::ElseClause),
    ("elif_clause", Role::ElseIf),
    ("case_statement", Role::Match),
    ("for_statement", Role::Loop),
    ("c_style_for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("command", Role::Call),
    ("declaration_command", Role::Decl),
    ("variable_assignment", Role::Bind),
    ("postfix_expression", Role::Write),
    ("unary_expression", Role::Write),
    ("ternary_expression", Role::Ternary),
    ("comment", Role::Comment),
    ("case_item", Role::MatchArm),
    ("expansion", Role::Global),
];

/// Bash language hooks.
pub(crate) static HOOKS: BashHooks = BashHooks;

/// Bash adapter.
pub(crate) struct BashHooks;

fn sole_call_id(body: Node<'_>) -> Option<usize> {
    if body.kind() != "compound_statement" {
        return None;
    }
    let mut found = None;
    for child in named_children(body) {
        if child.kind() == "comment" {
            continue;
        }
        if found.is_some() || child.kind() != "command" {
            return None;
        }
        found = Some(child.id());
    }
    found
}

fn push_assign_name<'s>(cx: &Cx<'_, 's>, name: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if name.kind() == "variable_name" {
        out.push((text(cx, name), pos_of(name)));
    } else if name.kind() == "subscript"
        && let Some(inner) = child_by_field(name, "name")
    {
        out.push((text(cx, inner), pos_of(inner)));
    }
}

fn bash_arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "word" => ArgShape::Str {
            longest_static: longest_static_chunk(&[text(cx, n)]),
        },
        "string" => string_arg_shape(cx, n, &["string_content"]),
        "raw_string" => ArgShape::Str {
            longest_static: narrow(text(cx, n).chars().count().saturating_sub(2)),
        },
        "ansi_c_string" => ArgShape::Str {
            longest_static: narrow(text(cx, n).chars().count().saturating_sub(3)),
        },
        "translated_string" => match first_named_child_matching(n, &["string"]) {
            Some(inner) => string_arg_shape(cx, inner, &["string_content"]),
            None => ArgShape::Other,
        },
        "concatenation" => ArgShape::Concat,
        "simple_expansion" | "variable_name" => ArgShape::Ident(text(cx, n)),
        _ => ArgShape::Other,
    }
}

fn bash_call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    let mut out = Vec::new();
    let mut cursor = n.walk();
    for arg in n.children_by_field_name("argument", &mut cursor) {
        out.push(Arg {
            shape: bash_arg_shape(cx, arg),
            start_line: narrow(arg.start_position().row) + 1,
            end_line: narrow(arg.end_position().row) + 1,
        });
    }
    out
}

impl LangHooks for BashHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        if n.kind() != "function_definition" {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        let body = child_by_field(n, "body")?;
        let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
        Some(FunctionHead {
            name: text(cx, name_node),
            name_pos: pos_of(name_node),
            public: true,
            interface_bound: false,
            in_test: cx.is_test_file,
            params: Vec::new(),
            return_type: None,
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: sole_call_id(body),
        })
    }

    fn is_else_if(&self, _n: Node<'_>) -> bool {
        false
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() == "variable_assignment" {
            if let Some(parent) = n.parent() {
                let kind = parent.kind();
                if kind == "declaration_command" || kind == "command" {
                    return;
                }
            }
            if let Some(name) = child_by_field(n, "name") {
                push_assign_name(cx, name, out);
            }
            return;
        }
        if n.kind() != "declaration_command" {
            return;
        }
        for child in named_children(n) {
            if child.kind() == "variable_assignment" {
                if let Some(name) = child_by_field(child, "name") {
                    push_assign_name(cx, name, out);
                }
            } else if child.kind() == "variable_name" {
                out.push((text(cx, child), pos_of(child)));
            }
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let kind = n.kind();
        if kind != "postfix_expression" && kind != "unary_expression" {
            return;
        }
        let own = trimmed(cx, n);
        if !own.contains("++") && !own.contains("--") {
            return;
        }
        let mut current = match first_named_child(n) {
            Some(first) => first,
            None => return,
        };
        loop {
            if current.kind() == "variable_name" {
                out.push((text(cx, current), pos_of(current)));
                return;
            }
            if current.kind() == "subscript" {
                match child_by_field(current, "name") {
                    Some(inner) => current = inner,
                    None => return,
                }
            } else {
                return;
            }
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() != "command" {
            return None;
        }
        let name_node = child_by_field(n, "name")?;
        Some(CallHead {
            callee: text(cx, name_node),
            kind: CallKind::Plain,
            callee_root: None,
            args: bash_call_args(cx, n),
        })
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        if n.kind() != "case_item" {
            return None;
        }
        let mut cursor = n.walk();
        let wild = n
            .children_by_field_name("value", &mut cursor)
            .any(|v| trimmed(cx, v) == "*");
        wild.then_some(WildcardArm {
            pos: pos_of(n),
            enum_like: false,
        })
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        if in_function || n.kind() != "declaration_command" {
            return;
        }
        let mut names = Vec::new();
        self.decl_names(n, cx, &mut names);
        for (name, pos) in names {
            out.push((AmbientKind::GlobalMutable, name, pos));
        }
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() == "expansion" {
            if text(cx, n).starts_with("${!") {
                return Some("indirect expansion");
            }
            return None;
        }
        if n.kind() != "command" {
            return None;
        }
        let name = child_by_field(n, "name").map(|m| text(cx, m)).unwrap_or("");
        if name == "eval" {
            return Some("eval");
        }
        if name.contains('$') {
            return Some("computed command name");
        }
        None
    }
}
