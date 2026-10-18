use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, CallKind, FieldCopy, Handler, ImplRef, Import,
    ImportKind, Param, Pos, Stmt, TypeDef, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, TypeHead, binary_concat, child_by_field, first_of_kind,
    has_child_kind, last_segment, named_children, narrow, pos_of, preceding_comments, slice,
    string_arg_shape, text, trimmed,
};
use crate::spec::Role;

/// Swift adapter hooks: declarations, calls, error handlers, and ambient state.
pub(crate) struct SwiftHooks;

#[rustfmt::skip]
pub(crate) static TABLE: crate::lang::Table = &[
    ("function_declaration", Role::Function), ("init_declaration", Role::Function),
    ("lambda_literal", Role::Closure), ("if_statement", Role::If),
    ("guard_statement", Role::If), ("switch_statement", Role::Match),
    ("for_statement", Role::Loop), ("while_statement", Role::Loop),
    ("repeat_while_statement", Role::Loop), ("catch_block", Role::Catch),
    ("ternary_expression", Role::Ternary), ("call_expression", Role::Call),
    ("property_declaration", Role::Decl), ("assignment", Role::Write),
    ("import_declaration", Role::Import), ("class_declaration", Role::TypeDef),
    ("protocol_declaration", Role::TypeDef), ("typealias_declaration", Role::TypeDef),
    ("comment", Role::Comment), ("multiline_comment", Role::Comment),
    ("switch_entry", Role::MatchArm), ("navigation_expression", Role::Member),
];

pub(crate) static HOOKS: SwiftHooks = SwiftHooks;

const PUBLIC_WORDS: &[&str] = &["public", "open"];
const BOUNDARY: &[&str] = &["function_declaration", "init_declaration", "lambda_literal"];
const COMMENT_KINDS: &[&str] = &["comment", "multiline_comment"];
const STRING_KINDS: &[&str] = &[
    "line_string_literal",
    "multi_line_string_literal",
    "raw_string_literal",
];

impl LangHooks for SwiftHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let body = child_by_field(n, "body")?;
        let name_node = child_by_field(n, "name")?;
        let class = ancestors(n).find(|a| a.kind() == "class_declaration");
        let xctest = class.is_some_and(|c| {
            named_children(c).into_iter().any(|s| {
                s.kind() == "inheritance_specifier" && trimmed(cx, s).contains("XCTestCase")
            })
        });
        let (doc, doc_pos) = preceding_comments(cx, n, COMMENT_KINDS, &[]);
        Some(FunctionHead {
            name: trimmed(cx, name_node),
            name_pos: pos_of(name_node),
            public: is_public(cx, n),
            interface_bound: class.is_some_and(|c| has_child_kind(c, "inheritance_specifier")),
            in_test: cx.is_test_file || xctest || swift_test_attribute(cx, n),
            params: params(cx, n),
            return_type: return_type(cx, n),
            missing_annotations: Vec::new(),
            doc,
            doc_pos,
            sole_call: sole_call(body),
        })
    }

    fn is_else_if(&self, n: Node<'_>) -> bool {
        n.parent().is_some_and(|p| p.kind() == "if_statement")
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if is_var(cx, n) {
            out.extend(bound_names(cx, n));
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() == "assignment" {
            out.extend(child_by_field(n, "target").and_then(|t| root_of(cx, t)));
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if is_subscript(n) {
            return None;
        }
        let callee = n.named_child(0)?;
        Some(CallHead {
            callee: trimmed(cx, callee),
            kind: CallKind::Plain,
            callee_root: callee_root(cx, callee),
            args: call_args(cx, n),
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        let target = named_children(n)
            .into_iter()
            .find(|c| c.kind() == "identifier");
        let (Some(target), "import_declaration") = (target, n.kind()) else {
            return;
        };
        let module = trimmed(cx, target);
        let named = first_of_kind(n, &["import"]).is_some_and(|keyword| {
            !slice(cx.src, keyword.end_byte(), target.start_byte())
                .trim()
                .is_empty()
        });
        out.push(Import {
            pos: pos_of(n),
            module: Cow::Borrowed(module),
            name: last_segment(module, &["."]),
            alias: None,
            kind: if named {
                ImportKind::Named
            } else {
                ImportKind::Namespace
            },
            in_test: cx.is_test_file,
        });
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        let name_node = child_by_field(n, "name")?;
        let base = TypeDef {
            name: trimmed(cx, name_node),
            pos: pos_of(name_node),
            public: is_public(cx, n),
            abstraction: false,
        };
        let extension = child_by_field(n, "declaration_kind")
            .is_some_and(|kind| trimmed(cx, kind) == "extension");
        let (def, impls) = match n.kind() {
            "class_declaration" if !extension => (base, inheritance_refs(cx, n)),
            "protocol_declaration" => (
                TypeDef {
                    abstraction: true,
                    ..base
                },
                Vec::new(),
            ),
            "typealias_declaration" => (base, Vec::new()),
            _ => return None,
        };
        Some(TypeHead {
            def: Some(def),
            impls,
            container: None,
        })
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        (n.kind() == "switch_entry" && is_wildcard_entry(cx, n)).then(|| WildcardArm {
            pos: pos_of(n),
            enum_like: sibling_enum_like(cx, n),
        })
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        if n.kind() != "catch_block" {
            return None;
        }
        let binding = match child_by_field(n, "error") {
            Some(pattern) => child_by_field(pattern, "bound_identifier")
                .map_or_else(|| trimmed(cx, pattern), |id| trimmed(cx, id)),
            None => "error",
        };
        let stmts = named_children(n)
            .into_iter()
            .find(|c| c.kind() == "statements")
            .map_or_else(Vec::new, |body| {
                named_children(body)
                    .into_iter()
                    .filter(|c| !COMMENT_KINDS.contains(&c.kind()))
                    .map(|s| handler_stmt(cx, s, binding))
                    .collect()
            });
        Some(Handler {
            pos: pos_of(n),
            stmts,
        })
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        if n.kind() != "call_expression" || is_subscript(n) {
            return None;
        }
        let callee = trimmed(cx, n.named_child(0)?);
        let arguments = value_arguments(n);
        let labeled = arguments
            .iter()
            .all(|a| child_by_field(*a, "name").is_some());
        let copies = !arguments.is_empty() && labeled;
        if !copies || !callee.starts_with(|c: char| c.is_ascii_uppercase()) {
            return None;
        }
        Some(FieldCopy {
            pos: pos_of(n),
            fields: narrow(arguments.len()),
            source: copy_source(cx, &arguments),
        })
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        match n.kind() {
            "property_declaration" if !in_function && is_var(cx, n) && global_scope(cx, n) => {
                let names = bound_names(cx, n);
                out.extend(
                    names
                        .into_iter()
                        .map(|(name, pos)| (AmbientKind::GlobalMutable, name, pos)),
                );
            }
            "navigation_expression"
                if eq_ignoring_whitespace(cx, n, "ProcessInfo.processInfo.environment") =>
            {
                out.push((AmbientKind::EnvRead, trimmed(cx, n), pos_of(n)));
            }
            "call_expression" => out.extend(
                n.named_child(0)
                    .filter(|c| eq_ignoring_whitespace(cx, *c, "getenv"))
                    .map(|c| (AmbientKind::EnvRead, trimmed(cx, c), pos_of(n))),
            ),
            _ => {}
        }
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() != "call_expression" {
            return None;
        }
        let first = value_arguments(n)
            .first()
            .and_then(|a| child_by_field(*a, "value"))?;
        let message = match trimmed(cx, n.named_child(0)?) {
            "NSSelectorFromString" => "NSSelectorFromString() with computed name",
            "NSClassFromString" => "NSClassFromString() with computed name",
            _ => return None,
        };
        (first.kind() != "line_string_literal").then_some(message)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "call_expression" {
            return None;
        }
        let callee = trimmed(cx, n.named_child(0)?);
        let arguments = value_arguments(n);
        let (has_message, comparison) = match callee {
            "assert" | "precondition" => {
                let first = arguments.first().and_then(|a| child_by_field(*a, "value"));
                let comparison = first.is_some_and(|v| {
                    matches!(v.kind(), "comparison_expression" | "equality_expression")
                });
                (arguments.len() >= 2, comparison)
            }
            _ => {
                let (operands, comparison) = xctest_shape(callee)?;
                let positional = arguments
                    .iter()
                    .filter(|a| child_by_field(**a, "name").is_none())
                    .count();
                let labeled = arguments.iter().any(|a| {
                    child_by_field(*a, "name")
                        .is_some_and(|label| trimmed(cx, label).trim_end_matches(':') == "message")
                });
                (positional > operands || labeled, comparison)
            }
        };
        Some(Assertion {
            pos: pos_of(n),
            has_message,
            comparison,
        })
    }
}

fn ancestors(n: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    std::iter::successors(n.parent(), |node| node.parent())
}

fn value_arguments(call: Node<'_>) -> Vec<Node<'_>> {
    suffix_children(call, "value_arguments")
        .into_iter()
        .flat_map(named_children)
        .filter(|c| c.kind() == "value_argument")
        .collect()
}

fn suffix_children<'a>(call: Node<'a>, kind: &str) -> Vec<Node<'a>> {
    named_children(call)
        .into_iter()
        .filter(|c| c.kind() == "call_suffix")
        .flat_map(named_children)
        .filter(|c| c.kind() == kind)
        .collect()
}

fn is_subscript(call: Node<'_>) -> bool {
    suffix_children(call, "value_arguments")
        .first()
        .and_then(|list| list.child(0))
        .is_some_and(|open| open.kind() == "[")
}

fn modifier_children(n: Node<'_>) -> Vec<Node<'_>> {
    named_children(n)
        .into_iter()
        .filter(|c| c.kind() == "modifiers")
        .flat_map(named_children)
        .collect()
}

fn has_public_modifier(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    modifier_children(n)
        .into_iter()
        .any(|m| m.kind() == "visibility_modifier" && PUBLIC_WORDS.contains(&trimmed(cx, m)))
}

fn is_public(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    has_public_modifier(cx, n) && enclosing_public(cx, n)
}

fn enclosing_public(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    !ancestors(n)
        .take_while(|a| !BOUNDARY.contains(&a.kind()))
        .any(|a| a.kind() == "class_declaration" && !has_public_modifier(cx, a))
}

fn swift_test_attribute(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    modifier_children(n)
        .into_iter()
        .filter(|c| c.kind() == "attribute")
        .flat_map(named_children)
        .any(|c| c.kind() == "user_type" && trimmed(cx, c) == "Test")
}

fn sole_call(body: Node<'_>) -> Option<usize> {
    let statements = named_children(body)
        .into_iter()
        .find(|c| c.kind() == "statements")?;
    let significant: Vec<Node<'_>> = named_children(statements)
        .into_iter()
        .filter(|c| !COMMENT_KINDS.contains(&c.kind()))
        .collect();
    let [only] = significant.as_slice() else {
        return None;
    };
    let call = unwrap_statement(*only)?;
    (call.kind() == "call_expression" && !is_subscript(call)).then(|| call.id())
}

fn unwrap_statement(node: Node<'_>) -> Option<Node<'_>> {
    let inner = match node.kind() {
        "control_transfer_statement" => child_by_field(node, "result"),
        "try_expression" | "await_expression" => child_by_field(node, "expr"),
        _ => return Some(node),
    };
    unwrap_statement(inner?)
}

fn params<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Vec<Param<'s>> {
    let parameters: Vec<Node<'_>> = named_children(func)
        .into_iter()
        .filter(|c| c.kind() == "parameter")
        .collect();
    parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            let next = parameters
                .get(index + 1)
                .map_or(usize::MAX, |p| p.start_byte());
            Param {
                name: child_by_field(*parameter, "name").map_or("", |name| trimmed(cx, name)),
                ty: param_type(cx, *parameter),
                bool_default: default_is_bool(cx, func, parameter.end_byte(), next),
            }
        })
        .collect()
}

fn return_type<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    let mut children = (0..func.child_count()).filter_map(|i| func.child(i));
    let arrow = children.find(|c| c.kind() == "->");
    let throws = (0..func.child_count())
        .filter_map(|i| func.child(i))
        .find(|c| matches!(c.kind(), "throws" | "throws_clause"))
        .filter(|throws| arrow.is_none_or(|arrow| throws.start_byte() < arrow.start_byte()));
    match (arrow, children.next()) {
        (Some(_), Some(ty)) if ty.is_named() => Some(slice(
            cx.src,
            throws.map_or(ty.start_byte(), |throws| throws.start_byte()),
            ty.end_byte(),
        )),
        (None, _) => throws.map(|throws| trimmed(cx, throws)),
        _ => None,
    }
}

fn param_type<'s>(cx: &Cx<'_, 's>, parameter: Node<'_>) -> Option<&'s str> {
    (0..parameter.child_count())
        .filter(|&i| parameter.field_name_for_child(i) == Some("name"))
        .filter_map(|i| parameter.child(i))
        .find(|c| c.kind() != "simple_identifier")
        .map(|ty| trimmed(cx, ty))
}

fn default_is_bool(cx: &Cx<'_, '_>, func: Node<'_>, after: usize, before: usize) -> bool {
    (0..func.child_count())
        .filter(|&index| func.field_name_for_child(index) == Some("default_value"))
        .filter_map(|index| func.child(index))
        .find(|child| child.start_byte() >= after && child.start_byte() < before)
        .is_some_and(|value| matches!(trimmed(cx, value), "true" | "false"))
}

fn is_var(cx: &Cx<'_, '_>, decl: Node<'_>) -> bool {
    named_children(decl)
        .into_iter()
        .find(|c| c.kind() == "value_binding_pattern")
        .and_then(|pattern| child_by_field(pattern, "mutability"))
        .is_some_and(|mutability| trimmed(cx, mutability) == "var")
}

fn bound_names<'s>(cx: &Cx<'_, 's>, decl: Node<'_>) -> Vec<(&'s str, Pos)> {
    (0..decl.child_count())
        .filter(|&index| decl.field_name_for_child(index) == Some("name"))
        .filter_map(|index| decl.child(index))
        .flat_map(named_children)
        .filter(|c| c.kind() == "simple_identifier")
        .map(|c| (text(cx, c), pos_of(c)))
        .collect()
}

fn global_scope(cx: &Cx<'_, '_>, decl: Node<'_>) -> bool {
    let nested =
        ancestors(decl).any(|a| matches!(a.kind(), "class_declaration" | "protocol_declaration"));
    !nested
        || modifier_children(decl).into_iter().any(|m| {
            m.kind() == "property_modifier" && matches!(trimmed(cx, m), "static" | "class")
        })
}

fn root_of<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    match n.kind() {
        "simple_identifier" => Some((text(cx, n), pos_of(n))),
        "self_expression" => Some(("self", pos_of(n))),
        "navigation_expression" | "postfix_expression" => root_of(cx, child_by_field(n, "target")?),
        "directly_assignable_expression" => root_of(cx, n.named_child(0)?),
        "call_expression" if is_subscript(n) => root_of(cx, n.named_child(0)?),
        _ => None,
    }
}

fn callee_root<'s>(cx: &Cx<'_, 's>, callee: Node<'_>) -> Option<&'s str> {
    match callee.kind() {
        "simple_identifier" => Some(text(cx, callee)),
        "self_expression" => Some("self"),
        "navigation_expression" => callee_root(cx, child_by_field(callee, "target")?),
        _ => None,
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, call: Node<'_>) -> Vec<Arg<'s>> {
    let positional = value_arguments(call)
        .into_iter()
        .filter_map(|argument| child_by_field(argument, "value"))
        .map(|value| arg(shape_of(cx, value), value));
    let trailing = suffix_children(call, "lambda_literal")
        .into_iter()
        .map(|lambda| arg(ArgShape::Other, lambda));
    positional.chain(trailing).collect()
}

fn arg<'s>(shape: ArgShape<'s>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape,
        start_line: pos_of(n).line,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn shape_of<'s>(cx: &Cx<'_, 's>, value: Node<'_>) -> ArgShape<'s> {
    match value.kind() {
        "line_string_literal" | "multi_line_string_literal" => {
            string_arg_shape(cx, value, &["line_str_text", "multi_line_str_text"])
        }
        "raw_string_literal" => string_arg_shape(cx, value, &["raw_str_part"]),
        "additive_expression" if binary_concat(cx, value, "+", STRING_KINDS) => ArgShape::Concat,
        "simple_identifier" => ArgShape::Ident(trimmed(cx, value)),
        _ => ArgShape::Other,
    }
}

fn inheritance_refs<'s>(cx: &Cx<'_, 's>, class: Node<'_>) -> Vec<ImplRef<'s>> {
    named_children(class)
        .into_iter()
        .filter(|c| c.kind() == "inheritance_specifier")
        .filter_map(|specifier| {
            let user = named_children(specifier)
                .into_iter()
                .find(|c| c.kind() == "user_type")?;
            let base = trimmed(cx, user);
            let cut = base.find('<').unwrap_or(base.len());
            Some(ImplRef {
                name: last_segment(&base[..cut], &["."]),
                pos: pos_of(specifier),
            })
        })
        .collect()
}

fn pattern_of(entry: Node<'_>) -> Option<Node<'_>> {
    named_children(entry)
        .into_iter()
        .find(|c| matches!(c.kind(), "switch_pattern" | "pattern"))
}

fn is_wildcard_entry(cx: &Cx<'_, '_>, entry: Node<'_>) -> bool {
    has_child_kind(entry, "default_keyword")
        || (!has_child_kind(entry, "where_keyword")
            && pattern_of(entry).is_some_and(|pattern| trimmed(cx, pattern) == "_"))
}

fn sibling_enum_like(cx: &Cx<'_, '_>, entry: Node<'_>) -> bool {
    entry.parent().is_some_and(|parent| {
        named_children(parent)
            .into_iter()
            .filter(|s| s.kind() == "switch_entry" && s.id() != entry.id())
            .filter_map(pattern_of)
            .any(|pattern| enum_pattern(trimmed(cx, pattern)))
    })
}

fn enum_pattern(pattern: &str) -> bool {
    pattern.starts_with('.')
        || pattern.split_once('.').is_some_and(|(head, tail)| {
            head.starts_with(|c: char| c.is_ascii_uppercase())
                && head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && tail.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        })
}

fn handler_stmt<'s>(cx: &Cx<'_, 's>, statement: Node<'_>, binding: &str) -> Stmt<'s> {
    match statement.kind() {
        "call_expression" if !is_subscript(statement) => {
            statement
                .named_child(0)
                .map_or(Stmt::Other, |callee| Stmt::Call {
                    callee: trimmed(cx, callee),
                    kind: CallKind::Plain,
                })
        }
        "control_transfer_statement"
            if has_child_kind(statement, "throw_keyword")
                && named_children(statement).last().is_some_and(|last| {
                    last.kind() == "simple_identifier" && trimmed(cx, *last) == binding
                }) =>
        {
            Stmt::Rethrow
        }
        _ => Stmt::Other,
    }
}

fn copy_source<'s>(cx: &Cx<'_, 's>, arguments: &[Node<'_>]) -> Option<&'s str> {
    let mut source: Option<&'s str> = None;
    for argument in arguments {
        let label = trimmed(cx, child_by_field(*argument, "name")?).trim_end_matches(':');
        let value =
            child_by_field(*argument, "value").filter(|v| v.kind() == "navigation_expression")?;
        let target = child_by_field(value, "target").filter(|t| t.kind() == "simple_identifier")?;
        let member = child_by_field(child_by_field(value, "suffix")?, "suffix")?;
        if trimmed(cx, member) != label || source.is_some_and(|s| s != text(cx, target)) {
            return None;
        }
        source = Some(text(cx, target));
    }
    source
}

fn eq_ignoring_whitespace(cx: &Cx<'_, '_>, n: Node<'_>, want: &str) -> bool {
    text(cx, n)
        .chars()
        .filter(|c| !c.is_whitespace())
        .eq(want.chars())
}

#[rustfmt::skip]
fn xctest_shape(callee: &str) -> Option<(usize, bool)> {
    match callee {
        "XCTAssert" | "XCTAssertTrue" | "XCTAssertFalse" | "XCTAssertNil" | "XCTAssertNotNil" => Some((1, false)),
        "XCTAssertEqual" | "XCTAssertNotEqual" | "XCTAssertGreaterThan" | "XCTAssertGreaterThanOrEqual"
        | "XCTAssertLessThan" | "XCTAssertLessThanOrEqual" => Some((2, true)),
        "XCTFail" => Some((0, false)),
        _ => None,
    }
}
