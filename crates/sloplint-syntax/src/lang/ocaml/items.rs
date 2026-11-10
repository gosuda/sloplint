use std::borrow::Cow;

use sloplint_core::facts::{
    Barrel, FieldCopy, Handler, Import, ImportKind, ImportScope, Pos, ScrutineeKind, TypeDef,
    Visibility, WildcardArm,
};
use sloplint_core::lang::Lang;
use tree_sitter::Node;

use super::OcamlHooks;
use super::calls::case_stmts;
use super::funcs::pattern_ident;
use super::params::is_parameter;
use crate::hooks::{
    Container, Cx, TypeHead, child_by_field, first_of_kind, last_segment, named_children, narrow,
    pos_of, text,
};

fn is_match_case(c: &Node<'_>) -> bool {
    c.kind() == "match_case"
}

const OCAML_STDLIB: &[&str] = &[
    "Arg",
    "Array",
    "ArrayLabels",
    "Atomic",
    "Bigarray",
    "Bool",
    "Buffer",
    "Bytes",
    "BytesLabels",
    "Callback",
    "Char",
    "Complex",
    "Condition",
    "Digest",
    "Domain",
    "Effect",
    "Either",
    "Ephemeron",
    "Filename",
    "Float",
    "Format",
    "Fun",
    "Gc",
    "Hashtbl",
    "In_channel",
    "Int",
    "Int32",
    "Int64",
    "Lazy",
    "Lexing",
    "List",
    "ListLabels",
    "Map",
    "Marshal",
    "MoreLabels",
    "Mutex",
    "Nativeint",
    "Oo",
    "Option",
    "Out_channel",
    "Parsing",
    "Printexc",
    "Printf",
    "Queue",
    "Random",
    "Result",
    "Scanf",
    "Semaphore",
    "Seq",
    "Set",
    "Stack",
    "StdLabels",
    "Stdlib",
    "Stream",
    "String",
    "StringLabels",
    "Sys",
    "Uchar",
    "Unit",
    "Weak",
];

fn external_root_for(module: &str) -> Option<&str> {
    let first = module.split('.').next().unwrap_or("");
    if first.is_empty() || OCAML_STDLIB.contains(&first) {
        return None;
    }
    Some(first)
}

pub(crate) fn imports_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
    let pos = pos_of(n);
    if n.kind() == "open_module" || n.kind() == "include_module" {
        let Some(module) = child_by_field(n, "module") else {
            return;
        };
        let raw = text(cx, module);
        let module = Cow::Borrowed(raw);
        let opened = Import {
            pos,
            module,
            name: "",
            alias: None,
            kind: ImportKind::Wildcard,
            scope: ImportScope::Module,
            in_test: false,
            is_reexport: n.kind() == "include_module",
            external_root: external_root_for(raw),
        };
        out.push(opened);
        return;
    }
    if n.kind() != "module_binding" {
        return;
    }
    let Some(body) = child_by_field(n, "body") else {
        return;
    };
    if body.kind() != "extended_module_path" && body.kind() != "module_path" {
        return;
    }
    let path = text(cx, body);
    let name = last_segment(path, &["."]);
    let found = first_of_kind(n, &["module_name"]).map(|m| text(cx, m));
    let alias = match found {
        Some(candidate) if candidate != name => Some(candidate),
        _ => None,
    };
    let module = Cow::Borrowed(path);
    let aliased = Import {
        pos,
        module,
        name,
        alias,
        kind: ImportKind::Namespace,
        scope: ImportScope::Module,
        in_test: false,
        is_reexport: false,
        external_root: external_root_for(path),
    };
    out.push(aliased);
}

fn typedef_at(name: &str, pos: Pos, visibility: Visibility, abstraction: bool) -> TypeDef<'_> {
    TypeDef {
        name,
        pos,
        visibility,
        abstraction,
    }
}

pub(crate) fn typedef_head<'s>(
    hooks: &OcamlHooks,
    n: Node<'_>,
    cx: &Cx<'_, 's>,
) -> Option<TypeHead<'s>> {
    if n.kind() == "module_type_definition" {
        let name_node = child_by_field(n, "module_type_name")?;
        let visibility = if cx.lang == Lang::OcamlInterface {
            Visibility::Exported
        } else {
            hooks.visibility_at(cx, n)
        };
        let def = typedef_at(text(cx, name_node), pos_of(name_node), visibility, true);
        return Some(TypeHead {
            def: Some(def),
            impls: Vec::new(),
            container: None,
        });
    }
    if n.kind() != "type_binding" {
        return None;
    }
    let name_node = child_by_field(n, "name")?;
    let no_body = child_by_field(n, "body").is_none();
    let no_synonym = child_by_field(n, "synonym").is_none();
    let visibility = if cx.lang == Lang::OcamlInterface {
        Visibility::Exported
    } else {
        hooks.visibility_at(cx, n)
    };
    let def = typedef_at(
        text(cx, name_node),
        pos_of(name_node),
        visibility,
        no_body && no_synonym,
    );
    Some(TypeHead {
        def: Some(def),
        impls: Vec::new(),
        container: None,
    })
}

pub(crate) fn container_head(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Container> {
    if n.kind() != "structure" {
        return None;
    }
    let parent = cx.parent()?;
    if parent.kind() != "module_binding" {
        return None;
    }
    let testy = first_of_kind(parent, &["module_name"])
        .is_some_and(|name| matches!(text(cx, name), "Test" | "Tests"));
    testy.then_some(Container::TestModule)
}

fn is_nullable_ctor(name: &str) -> bool {
    matches!(last_segment(name, &["."]), "Some" | "None" | "Ok" | "Error")
}

fn record_pattern_name<'s>(names: &mut Vec<&'s str>, nullable: &mut bool, name: &'s str) {
    if !names.contains(&name) {
        names.push(name);
    }
    if is_nullable_ctor(name) {
        *nullable = true;
    }
}

fn collect_pattern_names<'s>(
    cx: &Cx<'_, 's>,
    pat: Node<'_>,
    names: &mut Vec<&'s str>,
    nullable: &mut bool,
) {
    let mut stack = vec![pat];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "constructor_pattern" => {
                if let Some(path) = first_of_kind(node, &["constructor_path"]) {
                    record_pattern_name(names, nullable, text(cx, path));
                }
            }
            "constructor_path" | "tag" | "polymorphic_variant_pattern" => {
                record_pattern_name(names, nullable, text(cx, node));
            }
            _ => {}
        }
        stack.extend(named_children(node));
    }
}

fn sibling_facts(cx: &Cx<'_, '_>, parent: Node<'_>, skip: Node<'_>) -> (bool, u32, bool) {
    let mut names: Vec<&str> = Vec::new();
    let mut nullable = false;
    for sib in named_children(parent) {
        if sib.kind() != "match_case" || sib.id() == skip.id() {
            continue;
        }
        if let Some(pat) = child_by_field(sib, "pattern") {
            collect_pattern_names(cx, pat, &mut names, &mut nullable);
        }
    }
    (names.len() >= 2, narrow(names.len()), nullable)
}

pub(crate) fn match_arm_head(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
    if n.kind() != "match_case" {
        return None;
    }
    let pattern = child_by_field(n, "pattern");
    if !pattern.is_some_and(|p| p.kind() == "any_pattern") {
        return None;
    }
    let parent = cx.parent()?;
    let (enum_like, variant_count, has_nullable_ctor) = sibling_facts(cx, parent, n);
    let scrutinee = if parent.kind() != "match_expression" {
        ScrutineeKind::Other
    } else {
        match child_by_field(parent, "expression").map(|e| e.kind()) {
            Some("identifier") | Some("value_name") | Some("value_path") => ScrutineeKind::Ident,
            Some("field_get_expression") => ScrutineeKind::Field,
            _ => ScrutineeKind::Other,
        }
    };
    let arm = WildcardArm {
        pos: pos_of(n),
        enum_like,
        scrutinee,
        variant_count,
        has_nullable_ctor,
    };
    Some(arm)
}

fn is_cleanup_name(name: &str) -> bool {
    const MARKERS: &[&str] = &["close", "dispose", "cleanup", "release"];
    MARKERS.iter().any(|marker| {
        name == *marker
            || name
                .strip_prefix(*marker)
                .is_some_and(|rest| rest.starts_with('_'))
    })
}

fn enclosing_cleanup(cx: &Cx<'_, '_>) -> bool {
    for ancestor in cx.ancestors() {
        if ancestor.kind() != "let_binding" {
            continue;
        }
        let is_func = named_children(ancestor)
            .into_iter()
            .any(|c| is_parameter(&c))
            || child_by_field(ancestor, "body")
                .is_some_and(|b| matches!(b.kind(), "fun_expression" | "function_expression"));
        if !is_func {
            continue;
        }
        return child_by_field(ancestor, "pattern")
            .and_then(|p| pattern_ident(cx, p))
            .is_some_and(|(name, _)| is_cleanup_name(name));
    }
    false
}

pub(crate) fn handler_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
    if n.kind() != "try_expression" {
        return None;
    }
    let mut stmts = Vec::new();
    for case in named_children(n) {
        if !is_match_case(&case) {
            continue;
        }
        case_stmts(cx, case, &mut stmts);
    }
    let in_cleanup = enclosing_cleanup(cx);
    Some(Handler {
        pos: pos_of(n),
        stmts,
        has_else: false,
        try_imports_only: false,
        in_cleanup,
    })
}

fn field_source<'s>(cx: &Cx<'_, 's>, field: Node<'_>) -> Option<&'s str> {
    let path = first_of_kind(field, &["field_path"])?;
    let body = child_by_field(field, "body")?;
    if body.kind() != "field_get_expression" {
        return None;
    }
    let record = child_by_field(body, "record")?;
    if !matches!(record.kind(), "identifier" | "value_path" | "value_name") {
        return None;
    }
    let target = child_by_field(body, "field")
        .map(|f| text(cx, f))
        .unwrap_or("");
    if target != text(cx, path) {
        return None;
    }
    Some(text(cx, record))
}

pub(crate) fn struct_lit_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
    if n.kind() != "record_expression" {
        return None;
    }
    let fields: Vec<Node<'_>> = named_children(n)
        .into_iter()
        .filter(|c| c.kind() == "field_expression")
        .collect();
    if fields.is_empty() {
        return None;
    }
    let mut source: Option<&'s str> = None;
    let mut complete = true;
    for field in &fields {
        match field_source(cx, *field) {
            Some(current) if source.is_none_or(|prev| prev == current) => {
                source = Some(current);
            }
            _ => {
                complete = false;
                break;
            }
        }
    }
    let source = if complete { source } else { None };
    Some(FieldCopy {
        pos: pos_of(n),
        fields: narrow(fields.len()),
        source,
    })
}

pub(crate) fn barrel_head(root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
    if root.kind() != "compilation_unit" {
        return None;
    }
    let kids: Vec<Node<'_>> = named_children(root)
        .into_iter()
        .filter(|c| c.kind() != "comment")
        .collect();
    if kids.is_empty() || !kids.iter().all(|c| c.kind() == "include_module") {
        return None;
    }
    let dir = match cx.path.rfind('/') {
        Some(at) => &cx.path[..at],
        None => "",
    };
    let mut targets = Vec::new();
    for child in kids {
        let Some(module) = child_by_field(child, "module") else {
            continue;
        };
        let name = text(cx, module);
        if name.contains('.') {
            continue;
        }
        let lower = name.to_lowercase();
        if dir.is_empty() {
            targets.push(vec![format!("{lower}.ml"), format!("{lower}.mli")]);
        } else {
            targets.push(vec![
                format!("{dir}/{lower}.ml"),
                format!("{dir}/{lower}.mli"),
            ]);
        }
    }
    (!targets.is_empty()).then_some(Barrel { targets })
}
