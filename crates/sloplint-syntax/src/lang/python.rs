use sloplint_core::facts::{Assertion, CallKind, FieldCopy, Handler, Import, Pos, WildcardArm};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, TypeHead, child_by_field, code_children,
    first_named_child, last_segment, named_children, pos_of, preceding_comments, text,
};
use crate::spec::Role;

mod dynamic;
mod dynamic_mutation;
mod dynamic_tables;
mod imports;
mod items;
mod params;
mod shapes;

use dynamic::{py_call_dynamic, py_subscript_dynamic};
use imports::push_imports;
use items::{
    push_ambient, push_assertion, push_barrel, push_handler, push_match_arm, push_struct_lit,
    push_typedef,
};
use params::{function_flags, push_params};
use shapes::py_call_args;

pub(crate) static TABLE: crate::lang::Table = &[
    ("function_definition", Role::Function),
    ("lambda", Role::Closure),
    ("if_statement", Role::If),
    ("elif_clause", Role::ElseIf),
    ("match_statement", Role::Match),
    ("for_statement", Role::Loop),
    ("while_statement", Role::Loop),
    ("except_clause", Role::Catch),
    ("with_statement", Role::With),
    ("conditional_expression", Role::Ternary),
    ("call", Role::Call),
    ("assignment", Role::Bind),
    ("augmented_assignment", Role::Write),
    ("import_statement", Role::Import),
    ("import_from_statement", Role::Import),
    ("class_definition", Role::TypeDef),
    ("comment", Role::Comment),
    ("case_clause", Role::MatchArm),
    ("subscript", Role::Subscript),
    ("attribute", Role::Member),
    ("assert_statement", Role::Assert),
    ("global_statement", Role::Global),
    ("return_statement", Role::Return),
];

pub(crate) struct PythonHooks;

pub(crate) static HOOKS: PythonHooks = PythonHooks;

pub(crate) struct Placement {
    pub class_member: bool,
    pub local: bool,
}

pub(crate) fn placement(cx: &Cx<'_, '_>) -> Placement {
    let mut class_member = None;
    let mut local = false;
    for ancestor in cx.ancestors() {
        match ancestor.kind() {
            "function_definition" => {
                local = true;
                class_member.get_or_insert(false);
            }
            "class_definition" => {
                class_member.get_or_insert(true);
            }
            _ => {}
        }
    }
    Placement {
        class_member: class_member.unwrap_or(false),
        local,
    }
}

pub(crate) fn inside_function(cx: &Cx<'_, '_>) -> bool {
    for ancestor in cx.ancestors() {
        match ancestor.kind() {
            "function_definition" | "lambda" => return true,
            "class_definition" => return false,
            _ => {}
        }
    }
    false
}

pub(crate) fn enclosing_function<'t>(cx: &Cx<'t, '_>) -> Option<Node<'t>> {
    cx.ancestors()
        .find(|ancestor| ancestor.kind() == "function_definition")
}

pub(crate) fn enclosing_class<'t>(cx: &Cx<'t, '_>) -> Option<Node<'t>> {
    for ancestor in cx.ancestors() {
        match ancestor.kind() {
            "class_definition" => return Some(ancestor),
            "function_definition" => return None,
            _ => {}
        }
    }
    None
}

pub(crate) fn is_test_path(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    if file == "conftest.py" || file.starts_with("test_") || file.ends_with("_test.py") {
        return true;
    }
    path.starts_with("tests/")
        || path.starts_with("test/")
        || path.starts_with("__tests__/")
        || path.contains("/tests/")
        || path.contains("/test/")
        || path.contains("/__tests__/")
}

pub(crate) fn class_is_abstract(cx: &Cx<'_, '_>, class: Node<'_>) -> bool {
    let Some(sup) = child_by_field(class, "superclasses") else {
        return false;
    };
    for base in named_children(sup) {
        if base.kind() == "keyword_argument" {
            let meta = child_by_field(base, "name").map_or("", |v| text(cx, v));
            let val = child_by_field(base, "value").map_or("", |v| text(cx, v));
            if meta == "metaclass" && last_segment(val, &["."]) == "ABCMeta" {
                return true;
            }
            continue;
        }
        let raw = if base.kind() == "subscript" {
            child_by_field(base, "value").map_or("", |v| text(cx, v))
        } else {
            text(cx, base)
        };
        let seg = last_segment(raw, &["."]);
        if seg == "Protocol" || seg == "ABC" {
            return true;
        }
    }
    false
}

fn docstring(n: Node<'_>) -> Option<Node<'_>> {
    let body = child_by_field(n, "body")?;
    let first = first_named_child(body)?;
    if first.kind() != "expression_statement" {
        return None;
    }
    if first.named_child_count() != 1
        || first_named_child(first).is_some_and(|c| c.kind() != "string")
    {
        return None;
    }
    first_named_child(first)
}

pub(crate) fn is_doc_expr(n: Node<'_>) -> bool {
    if n.kind() != "expression_statement" {
        return false;
    }
    n.named_child_count() == 1 && first_named_child(n).is_some_and(|c| c.kind() == "string")
}

fn py_single_call(n: Node<'_>) -> Option<usize> {
    match n.kind() {
        "expression_statement" | "return_statement" => {
            let inner = first_named_child(n)?;
            (inner.kind() == "call").then_some(inner.id())
        }
        _ => None,
    }
}

fn py_sole(body: Node<'_>) -> Option<usize> {
    let code = code_children(body);
    let first = *code.first()?;
    let (target, rest) = if is_doc_expr(first) {
        (*code.get(1)?, 2)
    } else {
        (first, 1)
    };
    if code.get(rest).is_some() {
        return None;
    }
    py_single_call(target)
}

fn py_is_simple_path(n: Node<'_>) -> bool {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" => return true,
            "attribute" => {
                let Some(attr) = child_by_field(cur, "attribute") else {
                    return false;
                };
                if attr.kind() != "identifier" {
                    return false;
                }
                let Some(obj) = child_by_field(cur, "object") else {
                    return false;
                };
                cur = obj;
            }
            _ => return false,
        }
    }
}

pub(crate) fn py_callee_text<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    match func.kind() {
        "identifier" => text(cx, func),
        "attribute" => {
            if py_is_simple_path(func) {
                return text(cx, func);
            }
            if let Some(attr) = child_by_field(func, "attribute") {
                text(cx, attr)
            } else {
                "attribute"
            }
        }
        _ => "call",
    }
}

fn py_pat_names<'s>(cx: &Cx<'_, 's>, left: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut stack = vec![left];
    while let Some(cur) = stack.pop() {
        match cur.kind() {
            "identifier" => out.push((text(cx, cur), pos_of(cur))),
            "pattern_list" | "tuple_pattern" | "list_pattern" => {
                stack.extend(named_children(cur));
            }
            _ => {}
        }
    }
}

fn py_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" => return Some((text(cx, cur), pos_of(cur))),
            "attribute" => cur = child_by_field(cur, "object")?,
            "subscript" => cur = child_by_field(cur, "value")?,
            "parenthesized_expression" => cur = first_named_child(cur)?,
            _ => return None,
        }
    }
}

const CALLEE_CHAIN: u8 = 1;

fn is_chain_root(t: &str) -> bool {
    !matches!(t, "True" | "False" | "None")
        && !t.as_bytes().first().is_some_and(|b| b.is_ascii_digit())
        && t.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || !b.is_ascii())
}

fn py_callee_root<'s>(cx: &Cx<'_, 's>, callee: Node<'_>) -> Option<&'s str> {
    match callee.kind() {
        "identifier" | "attribute" | "subscript" | "call" | "parenthesized_expression" => {}
        _ => return None,
    }
    let root = cx.chain_root(CALLEE_CHAIN, callee, |current| match current.kind() {
        "attribute" => match child_by_field(current, "object") {
            Some(next) => crate::hooks::Link::Next(next),
            None => crate::hooks::Link::Root(current),
        },
        "subscript" | "call" | "parenthesized_expression" => crate::hooks::Link::Root(current),
        _ => crate::hooks::Link::Root(current),
    });
    is_chain_root(root).then_some(root)
}

fn is_binding_init(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    if parent.kind() != "assignment" {
        return false;
    }
    let right_ok = child_by_field(parent, "right").is_some_and(|r| r.id() == n.id());
    let left_ok = child_by_field(parent, "left").is_some_and(|l| l.kind() == "identifier");
    right_ok && left_ok
}

pub(crate) fn parent_dir(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

pub(crate) fn join_path(base: &str, rest: String) -> String {
    if base.is_empty() {
        rest
    } else {
        base.to_owned() + "/" + rest.as_str()
    }
}

pub(crate) fn py_targets(cx: &Cx<'_, '_>, dir: &str, n: Node<'_>) -> Vec<String> {
    let module = child_by_field(n, "module_name").map_or("", |m| text(cx, m));
    let dots = module.bytes().take_while(|b| *b == b'.').count();
    if dots == 0 {
        return Vec::new();
    }
    let rest = module[dots..].replace('.', "/");
    let mut base = dir.to_owned();
    for _ in 1..dots {
        match base.rfind('/') {
            Some(i) => base.truncate(i),
            None => base.clear(),
        }
    }
    if rest.is_empty() {
        let mod_id = child_by_field(n, "module_name").map(|m| m.id());
        let mut out = Vec::new();
        for c in named_children(n) {
            if Some(c.id()) == mod_id || c.kind() == "wildcard_import" {
                continue;
            }
            let nm = if c.kind() == "aliased_import" {
                child_by_field(c, "name").map_or("", |v| text(cx, v))
            } else {
                text(cx, c)
            };
            let seg = last_segment(nm, &["."]);
            out.push(join_path(&base, seg.to_owned() + ".py"));
            out.push(join_path(&base, seg.to_owned() + "/__init__.py"));
        }
        return out;
    }
    vec![
        join_path(&base, rest.clone() + ".py"),
        join_path(&base, rest + "/__init__.py"),
    ]
}

impl LangHooks for PythonHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let name_node = child_by_field(n, "name")?;
        let name = text(cx, name_node);
        let place = placement(cx);
        let flags = function_flags(cx, n, name, &place);
        let mut plist = Vec::new();
        if let Some(list) = child_by_field(n, "parameters") {
            push_params(cx, list, &mut plist);
        }
        if place.class_member {
            let drops_receiver = plist
                .first()
                .is_some_and(|p| p.name == "self" || p.name == "cls");
            if drops_receiver {
                plist.remove(0);
            }
        }
        let ret = child_by_field(n, "return_type").map(|t| text(cx, t));
        let mut missing: Vec<&'s str> = plist
            .iter()
            .filter(|p| p.ty.is_none())
            .map(|p| p.name)
            .collect();
        if ret.is_none() && name != "__init__" && name != "__new__" {
            missing.push("return");
        }
        let anchor = cx
            .parent()
            .filter(|p| p.kind() == "decorated_definition")
            .unwrap_or(n);
        let (mut doc, mut doc_pos) = preceding_comments(cx, anchor, &["comment"], &[]);
        if let Some(ds) = docstring(n) {
            if doc_pos.is_none() {
                doc_pos = Some(pos_of(ds));
            }
            doc.push(text(cx, ds));
        }
        let body = child_by_field(n, "body")?;
        Some(FunctionHead {
            shadow_params: Vec::new(),
            name,
            name_pos: pos_of(name_node),
            visibility: flags.visibility,
            signature_forced: flags.signature_forced,
            entry_point: flags.entry_point,
            in_test: flags.in_test,
            is_overload: flags.is_overload,
            is_deprecated: flags.is_deprecated,
            is_decorated: flags.is_decorated,
            is_trait_item: flags.is_trait_item,
            region: None,
            receiver: None,
            params: plist,
            return_type: ret,
            missing_annotations: missing,
            doc,
            doc_pos,
            sole_call: py_sole(body),
        })
    }

    fn is_else_if(&self, _: Node<'_>, _parent: Option<Node<'_>>) -> bool {
        false
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "assignment" {
            return;
        }
        if let Some(left) = child_by_field(n, "left") {
            py_pat_names(cx, left, out);
        }
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        if n.kind() != "assignment" && n.kind() != "augmented_assignment" {
            return;
        }
        let Some(left) = child_by_field(n, "left") else {
            return;
        };
        if n.kind() == "augmented_assignment" || left.kind() != "identifier" {
            let plain = left.kind() == "identifier" || left.kind() == "attribute";
            if plain || left.kind() == "subscript" {
                if let Some(root) = py_root(cx, left) {
                    out.push(root);
                }
            } else if n.kind() == "augmented_assignment"
                && let Some(root) = py_root(cx, left)
            {
                out.push(root);
            }
        }
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() != "call" {
            return None;
        }
        let func = child_by_field(n, "function")?;
        let mut args = Vec::new();
        if let Some(list) = child_by_field(n, "arguments") {
            py_call_args(cx, list, &mut args);
        }
        Some(CallHead {
            callee: py_callee_text(cx, func),
            kind: CallKind::Plain,
            callee_root: py_callee_root(cx, func),
            is_simple_path: py_is_simple_path(func),
            initializes_binding: is_binding_init(cx, n),
            args,
        })
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        push_imports(n, cx, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        push_typedef(n, cx)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        push_match_arm(n, cx)
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        push_handler(n, cx)
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        push_struct_lit(n, cx)
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(sloplint_core::facts::AmbientKind, &'s str, Pos)>,
    ) {
        push_ambient(n, cx, in_function, out);
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() == "call" {
            return py_call_dynamic(n, cx);
        }
        if n.kind() != "subscript" {
            return None;
        }
        py_subscript_dynamic(n, cx)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        push_assertion(n, cx)
    }

    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<sloplint_core::facts::Barrel> {
        push_barrel(root, cx)
    }
}
