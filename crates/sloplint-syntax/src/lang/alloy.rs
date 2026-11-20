//! Alloy 6 adapter: predicates, functions, facts and asserts as functions,
//! implication as the conditional, quantification as the loop.
//!
//! Alloy is relational and expression-oriented: there is no `if` statement,
//! no `match`, and no statement loop. Conditionality is `implies_expression`
//! (with an optional `else` it is if-then-else); iteration is
//! `quantified_expression` (`all`/`some`/`no`/`lone`/`one` over bindings).
//! Application of a predicate or function is `box_join_expression`
//! (`name[args]`); the same syntax is also relational join, so the call
//! facts over-approximate where a head is a relation rather than a
//! paragraph. Plain boolean connectives (`and`/`or`/`iff`), temporal
//! operators (`always`/`eventually`/`until`/`since`) and `let` bindings
//! have no counterpart in the linter concepts and stay unmapped.

use std::borrow::Cow;

use sloplint_core::facts::{
    Arg, ArgShape, Import, ImportKind, ImportScope, Param, Pos, PrimitiveKind, Visibility,
};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, TypeHead, child_by_field, first_named_child_matching,
    named_children, narrow, pos_of, preceding_comments, strip_quotes, text, trimmed,
};
use crate::spec::Role;

pub(crate) static TABLE: crate::lang::Table = &[
    ("pred_decl", Role::Function),
    ("fun_decl", Role::Function),
    ("fact_decl", Role::Function),
    ("assert_decl", Role::Function),
    ("implies_expression", Role::If),
    ("quantified_expression", Role::Loop),
    ("box_join_expression", Role::Call),
    ("module_decl", Role::Module),
    ("open_decl", Role::Import),
    ("sig_decl", Role::TypeDef),
    ("enum_decl", Role::TypeDef),
    ("line_comment", Role::Comment),
    ("block_comment", Role::Comment),
];

pub(crate) struct AlloyHooks;

pub(crate) static HOOKS: AlloyHooks = AlloyHooks;

const COMMENT_KINDS: &[&str] = &["line_comment", "block_comment"];

fn has_visibility_marker(n: Node<'_>) -> bool {
    named_children(n).iter().any(|c| c.kind() == "visibility")
}
fn is_private(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    if has_visibility_marker(n) {
        return true;
    }
    // `sig` paragraphs carry `private` as a sig_qualifier instead.
    named_children(n)
        .iter()
        .any(|c| c.kind() == "sig_qualifier" && trimmed(cx, *c) == "private")
}

fn alloy_primitive(ty: Option<&str>) -> PrimitiveKind {
    match ty {
        Some("Int") => PrimitiveKind::Int,
        Some("String") => PrimitiveKind::Str,
        _ => PrimitiveKind::Other,
    }
}

/// A declarator spelling including trailing primes: `_decl_name` emits a
/// `name` node followed by anonymous `'` tokens, so extend the slice over
/// `'` runs (and whitespace between them) — `item'` must read `item'` for
/// passthrough and arity checks to match calls.
fn decl_name_text<'s>(cx: &Cx<'_, 's>, name: Node<'_>) -> &'s str {
    let bytes = cx.src.as_bytes();
    let mut end = name.end_byte();
    loop {
        let mut probe = end;
        while probe < bytes.len() && bytes[probe].is_ascii_whitespace() {
            probe += 1;
        }
        if probe < bytes.len() && bytes[probe] == b'\'' {
            end = probe + 1;
        } else {
            break;
        }
    }
    &cx.src[name.start_byte()..end]
}

fn decl_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let mut params = Vec::new();
    let Some(list) = first_named_child_matching(n, &["decl_list"]) else {
        return params;
    };
    for decl in named_children(list) {
        if decl.kind() != "decl" {
            continue;
        }
        let mut names = Vec::new();
        let mut ty: Option<&'s str> = None;
        for child in named_children(decl) {
            if child.kind() == "name_list" {
                names.extend(
                    named_children(child)
                        .into_iter()
                        .filter(|name| name.kind() == "name")
                        .map(|name| decl_name_text(cx, name)),
                );
            } else {
                ty = Some(trimmed(cx, child));
            }
        }
        let primitive_kind = alloy_primitive(ty);
        for name in names {
            params.push(Param {
                name,
                ty,
                primitive_kind,
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: false,
            });
        }
    }
    params
}

fn paragraph_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (&'s str, Pos, bool) {
    match child_by_field(n, "name") {
        Some(name_node) => {
            let name = if name_node.kind() == "string" {
                strip_quotes(trimmed(cx, name_node))
            } else {
                trimmed(cx, name_node)
            };
            (name, pos_of(name_node), true)
        }
        None => ("", pos_of(n), false),
    }
}

fn alloy_function<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    if !matches!(
        n.kind(),
        "pred_decl" | "fun_decl" | "fact_decl" | "assert_decl"
    ) {
        return None;
    }
    let (name, name_pos, named) = paragraph_name(cx, n);
    // An unnamed paragraph cannot be referenced or searched by name, so it
    // is private to the name-based rules even without a `private` marker.
    let visibility = if named && !is_private(cx, n) {
        Visibility::Exported
    } else {
        Visibility::Private
    };
    let receiver = (n.kind() == "pred_decl" || n.kind() == "fun_decl")
        .then(|| first_named_child_matching(n, &["sig_ref"]))
        .flatten()
        .map(|sig| trimmed(cx, sig));
    let return_type = (n.kind() == "fun_decl")
        .then(|| child_by_field(n, "return_type"))
        .flatten()
        .map(|ty| trimmed(cx, ty));
    let (doc, doc_pos) = preceding_comments(cx, n, COMMENT_KINDS, &[]);
    // `S.foo[x]` reads `this` implicitly, so the receiver counts as the
    // first parameter for arity checks (passthrough, boolean-param rules).
    let mut params = Vec::new();
    if let Some(recv) = receiver {
        params.push(Param {
            name: "this",
            ty: Some(recv),
            primitive_kind: alloy_primitive(Some(recv)),
            has_default: false,
            bool_default: false,
            keyword_only: false,
            is_variadic: false,
        });
    }
    params.extend(decl_params(cx, n));
    Some(FunctionHead {
        shadow_params: Vec::new(),
        name,
        name_pos,
        visibility,
        signature_forced: false,
        entry_point: false,
        in_test: is_alloy_test_path(cx.path),
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        params,
        receiver,
        return_type,
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_box_join(cx, n),
    })
}

/// A paragraph body holding exactly one call, for the passthrough rule.
/// Suppressed when a `run`/`check` command targets the paragraph's own
/// name — `run Foo` is how Alloy invokes the entry point, and flagging
/// `pred Foo { sole[call] }` as a passthrough would flag every runner.
fn sole_box_join<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<usize> {
    let body = first_named_child_matching(n, &["block_expression"])?;
    let mut inner = named_children(body)
        .into_iter()
        .filter(|c| !COMMENT_KINDS.contains(&c.kind()));
    let only = inner.next()?;
    if inner.next().is_some() || only.kind() != "box_join_expression" {
        return None;
    }
    if let Some(name_node) = child_by_field(n, "name") {
        let decl_name = trimmed(cx, name_node);
        let run_targeted = cx.root().is_some_and(|root| {
            let mut cursor = root.walk();
            root.children(&mut cursor).any(|c| {
                c.kind() == "command"
                    && child_by_field(c, "name")
                        .is_some_and(|target| last_segment(trimmed(cx, target)) == decl_name)
            })
        });
        if run_targeted {
            return None;
        }
    }
    Some(only.id())
}

fn make_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    let shape = match n.kind() {
        "string" => {
            let inner = strip_quotes(trimmed(cx, n));
            ArgShape::Str {
                longest_static: longest_static_chunk(&[inner]),
            }
        }
        "name" | "qual_name" => ArgShape::Ident(trimmed(cx, n)),
        "this_expr" => ArgShape::Ident("this"),
        "prime_expression" => ArgShape::Ident(trimmed(cx, n)),
        "set_comprehension" => ArgShape::Collection,
        _ => ArgShape::Other,
    };
    Arg {
        shape,
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn box_join_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    let Some(list) = first_named_child_matching(n, &["expr_list"]) else {
        return Vec::new();
    };
    named_children(list)
        .into_iter()
        .filter(|c| !COMMENT_KINDS.contains(&c.kind()))
        .map(|child| make_arg(cx, child))
        .collect()
}

fn is_alloy_test_path(path: &str) -> bool {
    let mut in_test_tree = false;
    let mut in_support_tree = false;
    for segment in path.split('/') {
        if segment == "test" || segment == "tests" {
            in_test_tree = true;
        } else if in_test_tree && matches!(segment, "support" | "common" | "shared" | "helpers") {
            in_support_tree = true;
        }
    }
    let test_suffix = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .ends_with("_test.als");
    !in_support_tree && (in_test_tree || test_suffix)
}

fn last_segment(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl LangHooks for AlloyHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        alloy_function(cx, n)
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() != "box_join_expression" {
            return None;
        }
        let head = named_children(n).into_iter().next()?;
        let callee = trimmed(cx, head);
        let callee_root = (head.kind() == "qual_name")
            .then(|| first_named_child_matching(head, &["name"]))
            .flatten()
            .map(|name| text(cx, name));
        Some(CallHead {
            callee,
            kind: sloplint_core::facts::CallKind::Plain,
            callee_root,
            is_simple_path: head.kind() == "qual_name",
            initializes_binding: false,
            args: box_join_args(cx, n),
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() != "open_decl" {
            return;
        }
        let Some(module_node) = child_by_field(n, "name") else {
            return;
        };
        let module = trimmed(cx, module_node);
        let alias = child_by_field(n, "alias").map(|alias| trimmed(cx, alias));
        out.push(Import {
            pos: pos_of(n),
            module: Cow::Borrowed(module),
            name: last_segment(module),
            alias,
            kind: ImportKind::Namespace,
            scope: ImportScope::Module,
            in_test: false,
            is_reexport: false,
            external_root: None,
        });
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        let (name_node, abstraction) = match n.kind() {
            "sig_decl" => {
                let names = child_by_field(n, "names")?;
                let first = named_children(names)
                    .into_iter()
                    .flat_map(|list| {
                        if list.kind() == "name_list" {
                            named_children(list)
                        } else {
                            vec![list]
                        }
                    })
                    .find(|name| name.kind() == "name")?;
                let abstraction = named_children(n)
                    .iter()
                    .any(|c| c.kind() == "sig_qualifier" && trimmed(cx, *c) == "abstract");
                (first, abstraction)
            }
            "enum_decl" => (child_by_field(n, "name")?, false),
            _ => return None,
        };
        let visibility = if is_private(cx, n) {
            Visibility::Private
        } else {
            Visibility::Exported
        };
        Some(TypeHead {
            def: Some(sloplint_core::facts::TypeDef {
                name: text(cx, name_node),
                pos: pos_of(name_node),
                visibility,
                abstraction,
            }),
            impls: Vec::new(),
            container: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::spec::{Role, spec};

    #[test]
    fn alloy_kinds_keep_their_roles() {
        let spec = spec(sloplint_core::lang::Lang::Alloy);
        let source = "module tour\nopen util/ordering as ord\nsig Book {\naddr: Name -> Addr\n}\npred add[b: Book] {\nall x: Book | b in x => some x else no x\n}\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&spec.language)
            .expect("alloy language loads");
        let tree = parser.parse(source, None).expect("tree");
        let mut cursor = tree.walk();
        let mut roles = Vec::new();
        loop {
            let node = cursor.node();
            if node.is_named() {
                roles.push((node.kind().to_string(), spec.role_for(node.kind_id())));
            }
            if cursor.goto_first_child() {
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    for (want_kind, want_role) in [
                        ("pred_decl", Role::Function),
                        ("implies_expression", Role::If),
                        ("quantified_expression", Role::Loop),
                        ("open_decl", Role::Import),
                        ("sig_decl", Role::TypeDef),
                    ] {
                        let got = roles
                            .iter()
                            .find(|(kind, _)| kind == want_kind)
                            .map(|(_, role)| *role);
                        assert_eq!(
                            got,
                            Some(want_role),
                            "{want_kind} must map to {want_role:?}, roles: {roles:?}"
                        );
                    }
                    return;
                }
            }
        }
    }
}
