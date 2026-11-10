use sloplint_core::facts::{Barrel, FieldCopy, Handler, ImplRef, Stmt, TypeDef, Visibility};
use sloplint_core::role::CodeRole;
use tree_sitter::Node;

use super::attrs::{
    has_any_attr, has_bench_attr, has_deprecated_attr, has_sibling_attr, has_test_attr,
};
use super::calls::sole_call_id;
use super::errors::{body_stmts, err_bound, has_err_selective_predecessor};
use super::params::{closure_params, rust_params};
use crate::hooks::{
    Container, Cx, FunctionHead, TypeHead, attributed_region, child_by_field, earliest_preceding,
    first_named_child, first_named_child_matching, has_child_kind, last_segment, named_children,
    pos_of, preceding_comments, text,
};

fn visibility_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    named_children(n)
        .into_iter()
        .find(|c| c.kind() == "visibility_modifier")
        .map(|m| text(cx, m))
}

fn enclosing_trait_visibility<'s>(cx: &Cx<'_, 's>, _n: Node<'_>) -> Option<Visibility> {
    for node in cx.ancestors() {
        if node.kind() == "trait_item" {
            match visibility_text(cx, node) {
                Some("pub") => return Some(Visibility::Exported),
                Some(_) => return Some(Visibility::Restricted),
                None => return Some(Visibility::Private),
            }
        }
        if node.kind() == "source_file" {
            return None;
        }
    }
    None
}

pub(super) fn visibility_of<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Visibility {
    if cx
        .innermost_container()
        .is_some_and(|c| matches!(c, Container::Trait { .. }))
    {
        if let Some(inherited) = enclosing_trait_visibility(cx, n) {
            return inherited;
        }
        if cx
            .innermost_container()
            .is_some_and(|c| matches!(c, Container::Trait { public: true }))
        {
            return Visibility::Exported;
        }
        return Visibility::Private;
    }
    match visibility_text(cx, n) {
        Some("pub") => Visibility::Exported,
        Some(_) => Visibility::Restricted,
        None => Visibility::Private,
    }
}

fn is_tokio_main(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    has_sibling_attr(cx, n, |cx, sib| {
        let Some(attr) = first_named_child_matching(sib, &["attribute"]) else {
            return false;
        };
        let path = first_named_child(attr).map_or("", |p| text(cx, p));
        path == "tokio::main" || path.ends_with("::tokio::main")
    })
}

pub(super) fn is_entry_point(cx: &Cx<'_, '_>, n: Node<'_>, name: &str) -> bool {
    name == "main" || is_tokio_main(cx, n)
}

pub(super) fn fn_in_test(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    if has_test_attr(cx, n) || has_bench_attr(cx, n) || cx.inside_test_module() {
        return true;
    }
    for node in cx.ancestors() {
        if node.kind() == "mod_item"
            && child_by_field(node, "name").is_some_and(|m| text(cx, m) == "tests")
        {
            return true;
        }
        if node.kind() == "source_file" {
            return false;
        }
    }
    false
}

pub(super) fn is_trait_item(cx: &Cx<'_, '_>) -> bool {
    cx.innermost_container()
        .is_some_and(|c| matches!(c, Container::Trait { .. }))
}

pub(super) fn function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let body = child_by_field(n, "body")?;
    let name_node = child_by_field(n, "name")?;
    let name = text(cx, name_node);
    let mut params = Vec::new();
    rust_params(cx, n, &mut params);
    let (doc, doc_pos) = preceding_comments(
        cx,
        n,
        &["line_comment", "block_comment"],
        &["attribute_item"],
    );
    let bound = matches!(
        cx.innermost_container(),
        Some(Container::Impl {
            interface_bound: true
        })
    );
    let tested = has_test_attr(cx, n);
    let benched = !tested && has_bench_attr(cx, n);
    let leading = earliest_preceding(n, "attribute_item", &["line_comment", "block_comment"]);
    Some(FunctionHead {
        name,
        name_pos: pos_of(name_node),
        visibility: visibility_of(cx, n),
        signature_forced: bound,
        entry_point: is_entry_point(cx, n, name),
        in_test: fn_in_test(cx, n),
        is_overload: false,
        is_deprecated: has_deprecated_attr(cx, n),
        is_decorated: has_any_attr(n),
        is_trait_item: is_trait_item(cx),
        region: tested
            .then(|| attributed_region(n, leading, CodeRole::Test))
            .or_else(|| benched.then(|| attributed_region(n, leading, CodeRole::Bench))),
        params,
        return_type: child_by_field(n, "return_type").map(|t| text(cx, t)),
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call_id(body),
    })
}

pub(super) fn closure_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    if n.kind() != "closure_expression" {
        return None;
    }
    let body = child_by_field(n, "body")?;
    let mut params = Vec::new();
    closure_params(cx, n, &mut params);
    Some(FunctionHead {
        name: "",
        name_pos: pos_of(n),
        visibility: Visibility::Private,
        signature_forced: false,
        entry_point: false,
        in_test: cx.inside_test_module(),
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        params,
        return_type: child_by_field(n, "return_type").map(|t| text(cx, t)),
        missing_annotations: Vec::new(),
        doc: Vec::new(),
        doc_pos: None,
        sole_call: sole_call_id(body),
    })
}

fn file_trait_names<'s>(cx: &Cx<'_, 's>, _n: Node<'_>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    named_children(root)
        .into_iter()
        .filter(|c| c.kind() == "trait_item")
        .filter_map(|t| child_by_field(t, "name"))
        .map(|name| text(cx, name))
        .collect()
}

fn child_is_abstract(cx: &Cx<'_, '_>, n: Node<'_>, impl_node: Node<'_>) -> bool {
    let Some(ty) = child_by_field(impl_node, "type") else {
        return false;
    };
    let last = text(cx, ty).rsplit("::").next().unwrap_or("");
    let base = last.split('<').next().unwrap_or("");
    let base = base.trim();
    !base.is_empty() && file_trait_names(cx, n).contains(&base)
}

pub(super) fn typedef<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<TypeHead<'s>> {
    if n.kind() == "impl_item" {
        let bound = child_by_field(n, "trait").is_some();
        let mut impls = Vec::new();
        if let Some(tr) = child_by_field(n, "trait") {
            let base = match tr.kind() {
                "scoped_type_identifier" => child_by_field(tr, "name").map_or("", |x| text(cx, x)),
                "generic_type" => child_by_field(tr, "type").map_or("", |x| text(cx, x)),
                _ => text(cx, tr),
            };
            impls.push(ImplRef {
                name: last_segment(base, &["::"]),
                pos: pos_of(tr),
                is_generic: child_by_field(n, "type_parameters").is_some(),
                child_is_abstract: child_is_abstract(cx, n, n),
            });
        }
        return Some(TypeHead {
            def: None,
            impls,
            container: Some(Container::Impl {
                interface_bound: bound,
            }),
        });
    }
    let name_node = child_by_field(n, "name")?;
    let abstraction = n.kind() == "trait_item";
    let def = TypeDef {
        name: text(cx, name_node),
        pos: pos_of(name_node),
        visibility: visibility_of(cx, n),
        abstraction,
    };
    let container = abstraction.then(|| {
        let public = visibility_text(cx, n).is_some();
        Container::Trait { public }
    });
    Some(TypeHead {
        def: Some(def),
        impls: Vec::new(),
        container,
    })
}

pub(super) fn container(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Container> {
    if n.kind() == "impl_item" {
        let bound = child_by_field(n, "trait").is_some();
        return Some(Container::Impl {
            interface_bound: bound,
        });
    }
    if n.kind() == "mod_item" && super::attrs::mod_is_test(cx, n) {
        return Some(Container::TestModule);
    }
    None
}

pub(super) fn module_region(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<CodeRole> {
    if n.kind() == "mod_item" && super::attrs::mod_is_test(cx, n) {
        return Some(CodeRole::Test);
    }
    None
}

pub(super) fn call_marker(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<CodeRole> {
    if n.kind() != "macro_invocation" {
        return None;
    }
    let name = child_by_field(n, "macro").map_or("", |mac| text(cx, mac));
    match last_segment(name, &["::"]) {
        "fuzz_target" => Some(CodeRole::Bench),
        "proptest" | "quickcheck" => Some(CodeRole::Test),
        _ => None,
    }
}

pub(super) fn module_leading<'t>(n: Node<'t>, _cx: &Cx<'_, '_>) -> Option<Node<'t>> {
    if n.kind() != "mod_item" {
        return None;
    }
    earliest_preceding(n, "attribute_item", &["line_comment", "block_comment"])
}

pub(super) fn barrel(root: Node<'_>) -> Option<Barrel> {
    if root.kind() != "source_file" {
        return None;
    }
    let mut uses: usize = 0;
    for child in named_children(root) {
        match child.kind() {
            "line_comment" | "block_comment" | "attribute_item" => {}
            "use_declaration" => {
                if !has_child_kind(child, "visibility_modifier") {
                    return None;
                }
                uses += 1;
            }
            _ => return None,
        }
    }
    if uses == 0 {
        return None;
    }
    Some(Barrel {
        targets: vec![Vec::new(); uses],
    })
}

fn field_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>, field: &str) -> &'s str {
    child_by_field(n, field).map_or("", |v| text(cx, v))
}

pub(super) fn struct_lit<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FieldCopy<'s>> {
    if n.kind() != "struct_expression" {
        return None;
    }
    let body = child_by_field(n, "body")?;
    let mut fields = 0u32;
    let mut source: Option<&'s str> = None;
    let mut poisoned = false;
    for child in named_children(body) {
        match child.kind() {
            "field_initializer" => {
                fields += 1;
                let want = child_by_field(child, "field").map(|k| text(cx, k))?;
                let val = child_by_field(child, "value")?;
                let cand = if val.kind() != "field_expression" {
                    None
                } else if let Some(obj) = child_by_field(val, "value") {
                    if obj.kind() != "identifier" || field_text(cx, val, "field") != want {
                        None
                    } else {
                        Some(text(cx, obj))
                    }
                } else {
                    None
                };
                match (source, cand) {
                    (None, Some(s)) => source = Some(s),
                    (Some(a), Some(b)) if a == b => {}
                    _ => poisoned = true,
                }
            }
            "shorthand_field_initializer" => {
                fields += 1;
                poisoned = true;
            }
            "base_field_initializer" => {}
            _ => poisoned = true,
        }
    }
    if fields == 0 {
        return None;
    }
    let source = if poisoned { None } else { source };
    Some(FieldCopy {
        pos: pos_of(n),
        fields,
        source,
    })
}

fn body_imports_only(body: Node<'_>) -> bool {
    if body.kind() != "block" {
        return false;
    }
    let mut found = false;
    for child in named_children(body) {
        match child.kind() {
            "line_comment" | "block_comment" => {}
            "use_declaration" => found = true,
            _ => return false,
        }
    }
    found
}

fn in_cleanup_fn(cx: &Cx<'_, '_>, _n: Node<'_>) -> bool {
    for node in cx.ancestors() {
        if node.kind() == "function_item" {
            return child_by_field(node, "name")
                .is_some_and(|name| matches!(text(cx, name), "drop" | "close"));
        }
        if node.kind() == "source_file" {
            return false;
        }
    }
    false
}

pub(super) fn handler<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<Handler<'s>> {
    if n.kind() == "match_arm" {
        let bound = err_bound(cx, child_by_field(n, "pattern")?)?;
        let value = child_by_field(n, "value")?;
        let stmts = body_stmts(cx, value, bound);
        if matches!(&stmts[..], [Stmt::Rethrow]) && has_err_selective_predecessor(cx, n) {
            return None;
        }
        return Some(Handler {
            pos: pos_of(n),
            stmts,
            has_else: false,
            try_imports_only: body_imports_only(value),
            in_cleanup: in_cleanup_fn(cx, n),
        });
    }
    if n.kind() == "if_expression" {
        let cond = child_by_field(n, "condition")?;
        if cond.kind() != "let_condition" {
            return None;
        }
        let bound = err_bound(cx, child_by_field(cond, "pattern")?)?;
        let body = child_by_field(n, "consequence")?;
        let stmts = body_stmts(cx, body, bound);
        return Some(Handler {
            pos: pos_of(n),
            stmts,
            has_else: child_by_field(n, "alternative").is_some(),
            try_imports_only: body_imports_only(body),
            in_cleanup: in_cleanup_fn(cx, n),
        });
    }
    None
}
