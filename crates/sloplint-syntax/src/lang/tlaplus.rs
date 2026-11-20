//! TLA+ (with PlusCal) adapter.
//!
//! The vendored `tree-sitter-tlaplus` grammar parses whole modules. TLA+
//! operator and function definitions are the function-like units; PlusCal
//! macros, procedures and processes are the statement-level units living
//! inside block comments. Quantifiers are the loop construct (TLA+ has no
//! statements to loop); proofs and conjunction/disjunction lists are
//! deliberately unmapped so idiomatic specification structure never counts
//! as code nesting or branches.

use std::borrow::Cow;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, child_by_field, first_named_child_matching,
    named_children, pos_of, preceding_comments, trimmed,
};
use crate::spec::Role;
use sloplint_core::facts::{
    CallKind, Import, ImportKind, ImportScope, Param, Pos, PrimitiveKind, Visibility,
};
use tree_sitter::Node;

pub(crate) static TABLE: crate::lang::Table = &[
    ("operator_definition", Role::Function),
    ("function_definition", Role::Function),
    ("theorem", Role::Function),
    ("pcal_macro_decl", Role::Function),
    ("pcal_procedure", Role::Function),
    ("pcal_process", Role::Function),
    ("if_then_else", Role::If),
    ("pcal_if", Role::If),
    ("pcal_either", Role::If),
    ("case", Role::Match),
    ("case_arm", Role::MatchArm),
    ("other_arm", Role::MatchArm),
    ("bounded_quantification", Role::Loop),
    ("unbounded_quantification", Role::Loop),
    ("pcal_while", Role::Loop),
    ("pcal_with", Role::With),
    ("bound_infix_op", Role::Call),
    ("bound_prefix_op", Role::Call),
    ("bound_postfix_op", Role::Call),
    ("bound_nonfix_op", Role::Call),
    ("bound_op", Role::Call),
    ("prefixed_op", Role::Call),
    ("function_evaluation", Role::Call),
    ("pcal_macro_call", Role::Call),
    ("pcal_proc_call", Role::Call),
    ("extends", Role::Import),
    ("instance", Role::Import),
    ("variable_declaration", Role::Decl),
    ("constant_declaration", Role::Decl),
    ("recursive_declaration", Role::Decl),
    ("pcal_var_decl", Role::Decl),
    ("pcal_assign", Role::Write),
    ("comment", Role::Comment),
    ("block_comment", Role::Comment),
];

pub(crate) static HOOKS: TlaplusHooks = TlaplusHooks;

pub(crate) struct TlaplusHooks;

const COMMENT_KINDS: &[&str] = &["comment", "block_comment"];

fn is_test_path(path: &str) -> bool {
    path.split('/')
        .any(|segment| segment == "test" || segment == "tests")
}

fn tlaplus_visibility(cx: &Cx<'_, '_>) -> Visibility {
    if cx
        .parent()
        .is_some_and(|parent| parent.kind() == "local_definition")
    {
        Visibility::Private
    } else {
        Visibility::Exported
    }
}

fn name_of<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let name = child_by_field(n, "name")?;
    Some((trimmed(cx, name), pos_of(name)))
}

fn operator_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let name_id = child_by_field(n, "name").map(|name| name.id());
    let mut params = Vec::new();
    for child in named_children(n) {
        if Some(child.id()) == name_id {
            continue;
        }
        match child.kind() {
            "identifier" => params.push(plain_param(cx, child)),
            "operator_declaration" => {
                if let Some(name) = child_by_field(child, "name") {
                    params.push(plain_param(cx, name));
                }
            }
            "operator_args" => {
                for arg in named_children(child) {
                    if arg.kind() == "identifier" {
                        params.push(plain_param(cx, arg));
                    }
                }
            }
            "quantifier_bound" => {
                for bound in named_children(child) {
                    if bound.kind() == "identifier" {
                        params.push(plain_param(cx, bound));
                    }
                }
            }
            _ => {}
        }
    }
    params
}

fn plain_param<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Param<'s> {
    Param {
        name: trimmed(cx, n),
        ty: None,
        primitive_kind: PrimitiveKind::Other,
        has_default: false,
        bool_default: false,
        keyword_only: false,
        is_variadic: false,
    }
}

fn function_head<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    name: (&'s str, Pos),
    params: Vec<Param<'s>>,
) -> FunctionHead<'s> {
    let (doc, doc_pos) = preceding_comments(cx, n, COMMENT_KINDS, &[]);
    FunctionHead {
        shadow_params: Vec::new(),
        name: name.0,
        name_pos: name.1,
        visibility: tlaplus_visibility(cx),
        signature_forced: false,
        entry_point: false,
        in_test: is_test_path(cx.path),
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        params,
        receiver: None,
        return_type: None,
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: None,
    }
}

fn proc_params<'s>(cx: &Cx<'_, 's>, decl: Node<'_>) -> Vec<Param<'s>> {
    let mut params = Vec::new();
    for child in named_children(decl) {
        if child.kind() == "pcal_proc_var_decl"
            && let Some(name) = first_named_child_matching(child, &["identifier"])
        {
            params.push(plain_param(cx, name));
        }
    }
    params
}

impl LangHooks for TlaplusHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        match n.kind() {
            "operator_definition" => {
                let name = name_of(cx, n)?;
                let params = operator_params(cx, n);
                Some(function_head(cx, n, name, params))
            }
            "function_definition" => {
                let name = name_of(cx, n)?;
                let params = operator_params(cx, n);
                Some(function_head(cx, n, name, params))
            }
            "theorem" => {
                let name = name_of(cx, n)?;
                Some(function_head(cx, n, name, Vec::new()))
            }
            "pcal_macro_decl" => {
                let name = name_of(cx, n)?;
                let name_id = child_by_field(n, "name").map(|named| named.id());
                let mut params = Vec::new();
                for child in named_children(n) {
                    if child.kind() == "identifier" && Some(child.id()) != name_id {
                        params.push(plain_param(cx, child));
                    }
                }
                Some(function_head(cx, n, name, params))
            }
            "pcal_procedure" => {
                let decl = first_named_child_matching(n, &["pcal_proc_decl"])?;
                let name = name_of(cx, decl)?;
                let params = proc_params(cx, decl);
                Some(function_head(cx, n, name, params))
            }
            "pcal_process" => {
                let name = name_of(cx, n)?;
                Some(function_head(cx, n, name, Vec::new()))
            }
            _ => None,
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        match n.kind() {
            "bound_infix_op" | "bound_prefix_op" | "bound_postfix_op" | "bound_nonfix_op" => {
                let symbol = child_by_field(n, "symbol")?;
                Some(CallHead {
                    callee: trimmed(cx, symbol),
                    kind: CallKind::Plain,
                    callee_root: None,
                    is_simple_path: false,
                    initializes_binding: false,
                    args: Vec::new(),
                })
            }
            "bound_op" | "prefixed_op" => {
                let name = child_by_field(n, "name").or_else(|| child_by_field(n, "op"))?;
                let leaf = first_named_child_matching(name, &["identifier_ref", "identifier"])
                    .unwrap_or(name);
                Some(CallHead {
                    callee: trimmed(cx, leaf),
                    kind: CallKind::Plain,
                    callee_root: None,
                    is_simple_path: false,
                    initializes_binding: false,
                    args: Vec::new(),
                })
            }
            "function_evaluation" => {
                let head = named_children(n).into_iter().next()?;
                Some(CallHead {
                    callee: trimmed(cx, head),
                    kind: CallKind::Plain,
                    callee_root: None,
                    is_simple_path: false,
                    initializes_binding: false,
                    args: Vec::new(),
                })
            }
            "pcal_macro_call" | "pcal_proc_call" => {
                let name = child_by_field(n, "name")?;
                Some(CallHead {
                    callee: trimmed(cx, name),
                    kind: CallKind::Plain,
                    callee_root: None,
                    is_simple_path: true,
                    initializes_binding: false,
                    args: Vec::new(),
                })
            }
            _ => None,
        }
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        match n.kind() {
            "variable_declaration" => {
                for child in named_children(n) {
                    if child.kind() == "identifier" {
                        out.push((trimmed(cx, child), pos_of(child)));
                    }
                }
            }
            "constant_declaration" | "recursive_declaration" => {
                for child in named_children(n) {
                    match child.kind() {
                        "identifier" => out.push((trimmed(cx, child), pos_of(child))),
                        "operator_declaration" => {
                            if let Some(name) = child_by_field(child, "name") {
                                out.push((trimmed(cx, name), pos_of(name)));
                            }
                        }
                        _ => {}
                    }
                }
            }
            "pcal_var_decl" => {
                if let Some(name) = first_named_child_matching(n, &["identifier"]) {
                    out.push((trimmed(cx, name), pos_of(name)));
                }
            }
            _ => {}
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() == "pcal_assign"
            && let Some(lhs) = first_named_child_matching(n, &["pcal_lhs"])
            && let Some(target) = first_named_child_matching(lhs, &["identifier_ref", "identifier"])
        {
            out.push((trimmed(cx, target), pos_of(target)));
        }
    }

    fn binds_flow_targets(&self, kind: &str) -> bool {
        kind == "pcal_with"
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        let pos = pos_of(n);
        match n.kind() {
            "extends" => {
                for child in named_children(n) {
                    if child.kind() == "identifier_ref" {
                        let module = trimmed(cx, child);
                        out.push(Import {
                            pos,
                            module: Cow::Borrowed(module),
                            name: module,
                            alias: None,
                            kind: ImportKind::Namespace,
                            scope: ImportScope::Module,
                            in_test: is_test_path(cx.path),
                            is_reexport: false,
                            external_root: None,
                        });
                    }
                }
            }
            "instance" => {
                if let Some(module) = first_named_child_matching(n, &["identifier_ref"]) {
                    let name = trimmed(cx, module);
                    out.push(Import {
                        pos,
                        module: Cow::Borrowed(name),
                        name,
                        alias: None,
                        kind: ImportKind::Namespace,
                        scope: ImportScope::Module,
                        in_test: is_test_path(cx.path),
                        is_reexport: false,
                        external_root: None,
                    });
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::spec::{Role, spec};

    const SOURCE: &str = r#"---- MODULE Demo ----
EXTENDS Naturals

Op(a, b) == a + b

Fib[n \in Nat] == IF n < 2 THEN n ELSE Fib[n - 1] + Fib[n - 2]

=============================================================================
"#;

    #[test]
    fn tlaplus_only_kinds_keep_their_roles() {
        let spec = spec(sloplint_core::lang::Lang::Tlaplus);
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&spec.language)
            .expect("tlaplus language loads");
        let tree = parser.parse(SOURCE, None).expect("tree");
        assert!(!tree.root_node().has_error(), "sample parses clean");
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
                        ("operator_definition", Role::Function),
                        ("if_then_else", Role::If),
                        ("bound_infix_op", Role::Call),
                        ("extends", Role::Import),
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
