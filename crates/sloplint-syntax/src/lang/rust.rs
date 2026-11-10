use std::borrow::Cow;

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, Barrel, CallKind, FieldCopy, Handler, ImplRef, Import,
    ImportKind, Param, Pos, Stmt, TypeDef, WildcardArm,
};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, TypeHead, any_preceding, attributed_region,
    binary_concat, child_by_field, earliest_preceding, first_named_child,
    first_named_child_matching, first_of_kind, has_child_kind, last_segment, named_children,
    narrow, only_named_child_matching, pos_of, preceding_comments, string_arg_shape, text,
};
use crate::spec::Role;
use sloplint_core::role::CodeRole;

mod errors;
mod use_tree;

use errors::{body_stmts, err_bound, has_err_selective_predecessor, sibling_enum};
use use_tree::push_use;

pub(crate) static TABLE: crate::lang::Table = &[
    ("function_item", Role::Function),
    ("closure_expression", Role::Closure),
    ("if_expression", Role::If),
    ("else_clause", Role::ElseClause),
    ("match_expression", Role::Match),
    ("for_expression", Role::Loop),
    ("while_expression", Role::Loop),
    ("loop_expression", Role::Loop),
    ("call_expression", Role::Call),
    ("macro_invocation", Role::MacroCall),
    ("let_declaration", Role::Decl),
    ("assignment_expression", Role::Write),
    ("compound_assignment_expr", Role::Write),
    ("reference_expression", Role::Write),
    ("use_declaration", Role::Import),
    ("struct_item", Role::TypeDef),
    ("enum_item", Role::TypeDef),
    ("union_item", Role::TypeDef),
    ("type_item", Role::TypeDef),
    ("trait_item", Role::TypeDef),
    ("impl_item", Role::Impl),
    ("mod_item", Role::Module),
    ("line_comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("match_arm", Role::MatchArm),
    ("struct_expression", Role::StructLit),
    ("static_item", Role::StaticItem),
];

pub(crate) struct RustHooks;

pub(crate) static HOOKS: RustHooks = RustHooks;
const STR_KINDS: [&str; 2] = ["string_literal", "raw_string_literal"];
fn field_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>, field: &str) -> &'s str {
    child_by_field(n, field).map_or("", |v| text(cx, v))
}
fn attr_last_segment<'s>(cx: &Cx<'_, 's>, attr: Node<'_>) -> &'s str {
    let path = first_named_child(attr).map_or("", |p| text(cx, p));
    path.rsplit("::").next().unwrap_or(path)
}
fn cfg_test_ids<'s>(cx: &Cx<'_, 's>, args: Node<'_>) -> Vec<&'s str> {
    named_children(args)
        .into_iter()
        .filter(|c| c.kind() == "identifier")
        .map(|c| text(cx, c))
        .collect()
}
fn cfg_all_implies_test(cx: &Cx<'_, '_>, args: Node<'_>) -> bool {
    let kids = named_children(args);
    if kids.len() != 2 || kids[0].kind() != "identifier" || kids[1].kind() != "token_tree" {
        return false;
    }
    if text(cx, kids[0]) != "all" {
        return false;
    }
    cfg_test_ids(cx, kids[1]).contains(&"test")
}
fn cfg_implies_test(cx: &Cx<'_, '_>, attr: Node<'_>) -> bool {
    let Some(args) = child_by_field(attr, "arguments") else {
        return false;
    };
    if cfg_test_ids(cx, args) == ["test"] {
        return true;
    }
    cfg_all_implies_test(cx, args)
}
fn attribute_is_test(cx: &Cx<'_, '_>, sib: Node<'_>) -> bool {
    let Some(attr) = first_named_child_matching(sib, &["attribute"]) else {
        return false;
    };
    if attr_last_segment(cx, attr) == "test" {
        return true;
    }
    let path = first_named_child(attr).map_or("", |p| text(cx, p));
    if path != "cfg" && !path.ends_with("::cfg") {
        return false;
    }
    cfg_implies_test(cx, attr)
}
fn is_bracketed_test(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    if n.kind() != "token_tree" {
        return false;
    }
    let mut cursor = n.walk();
    let kids: Vec<Node<'_>> = n.children(&mut cursor).collect();
    kids.len() == 3
        && text(cx, kids[0]) == "["
        && kids[1].kind() == "identifier"
        && text(cx, kids[1]) == "test"
        && text(cx, kids[2]) == "]"
}
fn is_test_fn_at(cx: &Cx<'_, '_>, kids: &[Node<'_>], i: usize) -> bool {
    kids.len() >= i + 3
        && text(cx, kids[i]) == "#"
        && is_bracketed_test(cx, kids[i + 1])
        && text(cx, kids[i + 2]) == "fn"
}
fn seq_has_test_fn(cx: &Cx<'_, '_>, tt: Node<'_>) -> bool {
    let mut cursor = tt.walk();
    let kids: Vec<Node<'_>> = tt.children(&mut cursor).collect();
    (0..kids.len()).any(|i| is_test_fn_at(cx, &kids, i))
}
fn macro_has_test_fn(cx: &Cx<'_, '_>, tt: Node<'_>) -> bool {
    let mut stack = vec![tt];
    while let Some(node) = stack.pop() {
        if node.kind() == "token_tree" && seq_has_test_fn(cx, node) {
            return true;
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    false
}
fn attribute_is_bench(cx: &Cx<'_, '_>, sib: Node<'_>) -> bool {
    let Some(attr) = first_named_child_matching(sib, &["attribute"]) else {
        return false;
    };
    if child_by_field(attr, "arguments").is_some() {
        return false;
    }
    let path = first_named_child(attr).map_or("", |p| text(cx, p));
    path == "bench" || path.ends_with("::bench")
}
fn has_sibling_attr(
    cx: &Cx<'_, '_>,
    n: Node<'_>,
    mut hit: impl FnMut(&Cx<'_, '_>, Node<'_>) -> bool,
) -> bool {
    any_preceding(
        n,
        "attribute_item",
        &["line_comment", "block_comment"],
        |sib| hit(cx, sib),
    )
}
fn has_test_attr(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    has_sibling_attr(cx, n, attribute_is_test)
}
fn has_bench_attr(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    has_sibling_attr(cx, n, attribute_is_bench)
}
fn push_ident_tree<'a>(root: Node<'a>, out: &mut Vec<Node<'a>>) {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "identifier" {
            out.push(cur);
        } else {
            stack.extend(named_children(cur));
        }
    }
}
fn push_mut_idents<'a>(root: Node<'a>, out: &mut Vec<Node<'a>>) {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "mut_pattern" {
            push_ident_tree(cur, out);
        } else {
            stack.extend(named_children(cur));
        }
    }
}
fn rust_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Param<'s>>) {
    let Some(list) = child_by_field(n, "parameters") else {
        return;
    };
    for child in named_children(list) {
        if child.kind() != "parameter" {
            continue;
        }
        let mut ids = Vec::new();
        if let Some(pat) = child_by_field(child, "pattern") {
            push_ident_tree(pat, &mut ids);
        }
        let ty = child_by_field(child, "type").map(|t| text(cx, t));
        let name = ids.first().map_or("", |id| text(cx, *id));
        out.push(Param {
            name,
            ty,
            bool_default: false,
        });
    }
}
fn unwrap_call(n: Node<'_>) -> Option<Node<'_>> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "call_expression" | "macro_invocation" => return Some(cur),
            "expression_statement"
            | "return_expression"
            | "try_expression"
            | "await_expression" => {
                cur = first_named_child(cur)?;
            }
            _ => return None,
        }
    }
}
fn sole_call_id(body: Node<'_>) -> Option<usize> {
    let only = only_named_child_matching(body, |c| {
        c.kind() != "line_comment" && c.kind() != "block_comment"
    })?;
    unwrap_call(only).map(|c| c.id())
}
fn write_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" => return Some((text(cx, cur), pos_of(cur))),
            "field_expression" => cur = child_by_field(cur, "value")?,
            "index_expression" => cur = first_named_child(cur)?,
            _ => return None,
        }
    }
}
fn callee_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" => return Some(text(cx, cur)),
            "field_expression" => cur = child_by_field(cur, "value")?,
            "scoped_identifier" => cur = child_by_field(cur, "path")?,
            _ => return None,
        }
    }
}
fn shaped_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    let shape = match n.kind() {
        "string_literal" | "raw_string_literal" => string_arg_shape(cx, n, &["string_content"]),
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ if binary_concat(cx, n, "+", &STR_KINDS) => ArgShape::Concat,
        _ => ArgShape::Other,
    };
    Arg {
        shape,
        start_line: pos_of(n).line,
        end_line: narrow(n.end_position().row) + 1,
    }
}
fn macro_args<'s>(cx: &Cx<'_, 's>, tt: Node<'_>) -> Vec<Arg<'s>> {
    let mut groups: Vec<Vec<Node<'_>>> = vec![Vec::new()];
    let mut cursor = tt.walk();
    for child in tt.children(&mut cursor) {
        if !child.is_named() {
            let t = text(cx, child);
            if t == "," {
                groups.push(Vec::new());
                continue;
            }
            if matches!(t, "(" | ")" | "[" | "]" | "{" | "}") {
                continue;
            }
        }
        if let Some(last) = groups.last_mut() {
            last.push(child);
        }
    }
    if groups
        .first()
        .is_some_and(|g| g.len() >= 2 && text(cx, g[0]) == "target" && text(cx, g[1]) == ":")
    {
        groups.remove(0);
    }
    groups
        .into_iter()
        .filter(|g| !g.is_empty())
        .enumerate()
        .map(|(i, g)| {
            let shape = if i == 0 {
                g.iter()
                    .find(|n| n.is_named())
                    .map_or(ArgShape::Other, |n| {
                        if n.kind() == "string_literal" {
                            string_arg_shape(cx, *n, &["string_content"])
                        } else {
                            ArgShape::Other
                        }
                    })
            } else {
                ArgShape::Other
            };
            let start = g.first().map_or(0, |n| pos_of(*n).line);
            let end = g.last().map_or(0, |n| narrow(n.end_position().row) + 1);
            Arg {
                shape,
                start_line: start,
                end_line: end,
            }
        })
        .collect()
}
fn with_prefix<'s>(prefix: &str, head: &'s str) -> Cow<'s, str> {
    if prefix.is_empty() {
        return Cow::Borrowed(head.trim_end_matches(':'));
    }
    let mut joined = prefix.to_owned() + head;
    joined.truncate(joined.trim_end_matches(':').len());
    Cow::Owned(joined)
}
fn leaf_import<'s>(cx: &Cx<'_, 's>, n: Node<'_>, prefix: &str, out: &mut Vec<Import<'s>>) {
    let (path, alias) = if n.kind() == "use_as_clause" {
        let alias = child_by_field(n, "alias").map_or("", |a| text(cx, a));
        if alias == "_" {
            return;
        }
        (
            child_by_field(n, "path").map_or("", |p| text(cx, p)),
            Some(alias),
        )
    } else {
        (text(cx, n), None)
    };
    let own = n
        .parent()
        .and_then(|list| list.parent())
        .and_then(|outer| child_by_field(outer, "path"))
        .map_or("", |scope| text(cx, scope));
    let (path, prefix) = if path == "self" && !own.is_empty() {
        (own, &prefix[..prefix.len().saturating_sub(own.len() + 2)])
    } else {
        (path, prefix)
    };
    let name = last_segment(path, &["::"]);
    let head = path[..path.len() - name.len()].trim_end_matches(':');
    out.push(Import {
        pos: pos_of(n),
        module: with_prefix(prefix, head),
        name,
        alias,
        kind: ImportKind::Named,
    });
}
fn wild_import<'s>(cx: &Cx<'_, 's>, n: Node<'_>, prefix: &str, out: &mut Vec<Import<'s>>) {
    let path = n.named_child(0).map_or("", |c| text(cx, c));
    out.push(Import {
        pos: pos_of(n),
        module: with_prefix(prefix, path),
        name: "*",
        alias: None,
        kind: ImportKind::Wildcard,
    });
}
fn parent_dir(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}
fn file_stem(path: &str) -> &str {
    let base = path.rfind('/').map_or(path, |i| &path[i + 1..]);
    base.rfind('.').map_or(base, |i| &base[..i])
}
fn join_path(base: &str, rest: String) -> String {
    if base.is_empty() {
        rest
    } else {
        base.to_owned() + "/" + rest.as_str()
    }
}
fn first_segment(value: &str) -> &str {
    let path = value.strip_prefix("self::").unwrap_or(value);
    path.split("::").next().map_or("", |s| s)
}
fn use_first<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "scoped_use_list" => {
                let path = child_by_field(current, "path")?;
                if text(cx, path) != "self" {
                    return Some(first_segment(text(cx, path)));
                }
                child_by_field(current, "list")?.named_child(0)?
            }
            "use_list" => current.named_child(0)?,
            _ => return Some(first_segment(text(cx, current))),
        };
    }
}
fn is_cell_token(tok: &str) -> bool {
    tok == "Mutex"
        || tok == "RwLock"
        || tok == "RefCell"
        || tok == "Cell"
        || tok.starts_with("Atomic")
}
fn is_cell_type(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    child_by_field(n, "type").is_some_and(|t| {
        text(cx, t)
            .split(|c: char| !c.is_alphanumeric())
            .any(is_cell_token)
    })
}
fn static_ambient<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if in_function {
        return;
    }
    if has_child_kind(n, "mutable_specifier") || is_cell_type(cx, n) {
        let name = child_by_field(n, "name").map_or("", |x| text(cx, x));
        out.push((AmbientKind::GlobalMutable, name, pos_of(n)));
    }
}
fn thread_local_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let Some(tree) = first_of_kind(n, &["token_tree"]) else {
        return "thread_local";
    };
    let mut cursor = tree.walk();
    let mut tokens = tree
        .children(&mut cursor)
        .skip_while(|t| t.kind() != "static");
    tokens
        .nth(1)
        .filter(|t| t.kind() == "identifier")
        .map_or("thread_local", |t| text(cx, t))
}
fn macro_ambient<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if in_function {
        return;
    }
    if last_segment(field_text(cx, n, "macro"), &["::"]) != "thread_local" {
        return;
    }
    out.push((
        AmbientKind::GlobalMutable,
        thread_local_name(cx, n),
        pos_of(n),
    ));
}
fn is_env_callee(callee: &str) -> bool {
    const ENVS: [&str; 4] = ["env::var", "env::var_os", "env::vars", "env::vars_os"];
    if ENVS.iter().any(|s| callee.ends_with(*s)) {
        return true;
    }
    const MAX_ENV_TAIL: usize = "env::vars_os".len();
    let mut tail = ['\0'; MAX_ENV_TAIL];
    let mut len = 0;
    for c in callee.chars().rev() {
        if c.is_whitespace() {
            continue;
        }
        if len == MAX_ENV_TAIL {
            break;
        }
        tail[len] = c;
        len += 1;
    }
    ENVS.iter()
        .any(|s| s.len() <= len && s.chars().rev().enumerate().all(|(i, c)| tail[i] == c))
}
fn env_ambient<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    let Some(func) = child_by_field(n, "function") else {
        return;
    };
    let callee = text(cx, func);
    if is_env_callee(callee) {
        out.push((AmbientKind::EnvRead, callee, pos_of(func)));
    }
}
impl LangHooks for RustHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let body = child_by_field(n, "body")?;
        let name_node = child_by_field(n, "name")?;
        let mut params = Vec::new();
        rust_params(cx, n, &mut params);
        let (doc, doc_pos) = preceding_comments(
            cx,
            n,
            &["line_comment", "block_comment"],
            &["attribute_item"],
        );
        let member = matches!(
            cx.innermost_container(),
            Some(Container::Trait { public: true })
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
            name: text(cx, name_node),
            name_pos: pos_of(name_node),
            public: has_child_kind(n, "visibility_modifier") || member,
            interface_bound: bound,
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
    fn is_else_if(&self, _n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        parent.is_some_and(|p| p.kind() == "else_clause")
    }
    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let Some(pat) = child_by_field(n, "pattern") else {
            return;
        };
        let mut found = Vec::new();
        if has_child_kind(n, "mutable_specifier") {
            push_ident_tree(pat, &mut found);
        } else {
            push_mut_idents(pat, &mut found);
        }
        for id in found {
            out.push((text(cx, id), pos_of(id)));
        }
    }
    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        match n.kind() {
            "assignment_expression" | "compound_assignment_expr" => {
                if let Some(left) = child_by_field(n, "left") {
                    out.extend(write_root(cx, left));
                }
            }
            "reference_expression" => {
                if has_child_kind(n, "mutable_specifier")
                    && let Some(val) = child_by_field(n, "value")
                {
                    out.extend(write_root(cx, val));
                }
            }
            "call_expression" => {
                let val = child_by_field(n, "function")
                    .filter(|f| f.kind() == "field_expression")
                    .and_then(|f| child_by_field(f, "value"));
                if let Some(val) = val {
                    out.extend(write_root(cx, val));
                }
            }
            _ => {}
        }
    }
    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        if n.kind() == "macro_invocation" {
            let mac = child_by_field(n, "macro")?;
            let tree = first_named_child_matching(n, &["token_tree"]);
            let args = tree.map_or(Vec::new(), |tt| macro_args(cx, tt));
            let head = CallHead {
                callee: text(cx, mac),
                kind: CallKind::Macro,
                callee_root: callee_root(cx, mac),
                args,
            };
            return Some(head);
        }
        let func = child_by_field(n, "function")?;
        let args = child_by_field(n, "arguments").map_or(Vec::new(), |a| {
            named_children(a)
                .into_iter()
                .map(|c| shaped_arg(cx, c))
                .collect()
        });
        Some(CallHead {
            callee: text(cx, func),
            kind: CallKind::Plain,
            callee_root: callee_root(cx, func),
            args,
        })
    }
    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if let Some(arg) = child_by_field(n, "argument") {
            let mut prefix = String::new();
            push_use(cx, arg, &mut prefix, out);
        }
    }
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        if n.kind() == "impl_item" {
            let bound = child_by_field(n, "trait").is_some();
            let mut impls = Vec::new();
            if let Some(tr) = child_by_field(n, "trait") {
                let base = match tr.kind() {
                    "scoped_type_identifier" => {
                        child_by_field(tr, "name").map_or("", |x| text(cx, x))
                    }
                    "generic_type" => child_by_field(tr, "type").map_or("", |x| text(cx, x)),
                    _ => text(cx, tr),
                };
                impls.push(ImplRef {
                    name: last_segment(base, &["::"]),
                    pos: pos_of(tr),
                });
            }
            let head = TypeHead {
                def: None,
                impls,
                container: Some(Container::Impl {
                    interface_bound: bound,
                }),
            };
            return Some(head);
        }
        let name_node = child_by_field(n, "name")?;
        let public = has_child_kind(n, "visibility_modifier");
        let abstraction = n.kind() == "trait_item";
        let def = TypeDef {
            name: text(cx, name_node),
            pos: pos_of(name_node),
            public,
            abstraction,
        };
        let container = abstraction.then_some(Container::Trait { public });
        Some(TypeHead {
            def: Some(def),
            impls: Vec::new(),
            container,
        })
    }
    fn container(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Container> {
        if n.kind() == "impl_item" {
            let bound = child_by_field(n, "trait").is_some();
            return Some(Container::Impl {
                interface_bound: bound,
            });
        }
        if n.kind() == "mod_item" && has_sibling_attr(cx, n, attribute_is_test) {
            return Some(Container::TestModule);
        }
        None
    }
    fn module_region(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<CodeRole> {
        if n.kind() == "mod_item" && has_sibling_attr(cx, n, attribute_is_test) {
            return Some(CodeRole::Test);
        }
        None
    }
    fn call_marker(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<CodeRole> {
        if n.kind() != "macro_invocation" {
            return None;
        }
        let name = child_by_field(n, "macro").map_or("", |mac| text(cx, mac));
        if last_segment(name, &["::"]) == "fuzz_target" {
            return Some(CodeRole::Bench);
        }
        if matches!(last_segment(name, &["::"]), "proptest" | "quickcheck")
            && first_named_child_matching(n, &["token_tree"])
                .is_some_and(|tt| macro_has_test_fn(cx, tt))
        {
            return Some(CodeRole::Test);
        }
        None
    }
    fn module_leading<'t>(&self, n: Node<'t>, _cx: &Cx<'_, '_>) -> Option<Node<'t>> {
        if n.kind() != "mod_item" {
            return None;
        }
        earliest_preceding(n, "attribute_item", &["line_comment", "block_comment"])
    }
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        let pat = child_by_field(n, "pattern")?;
        let inner = if pat.kind() == "match_pattern" {
            if child_by_field(pat, "condition").is_some() {
                return None;
            }
            let only = pat.named_child(0).unwrap_or(pat);
            if pat.named_child(1).is_some() {
                return None;
            }
            only
        } else {
            pat
        };
        if text(cx, inner) != "_" || inner.named_child_count() != 0 {
            return None;
        }
        let arm = WildcardArm {
            pos: pos_of(n),
            enum_like: sibling_enum(cx, n),
        };
        Some(arm)
    }
    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
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
            });
        }
        None
    }
    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
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
        let copy = FieldCopy {
            pos: pos_of(n),
            fields,
            source,
        };
        Some(copy)
    }
    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        if n.kind() == "static_item" {
            static_ambient(cx, n, in_function, out);
            return;
        }
        if n.kind() == "macro_invocation" {
            macro_ambient(cx, n, in_function, out);
            return;
        }
        if n.kind() == "call_expression" {
            env_ambient(cx, n, out);
        }
    }
    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        if n.kind() != "macro_invocation" {
            return None;
        }
        let last = child_by_field(n, "macro").map_or("", |m| last_segment(text(cx, m), &["::"]));
        (last == "paste" || last == "concat_idents").then_some("token-pasting macro")
    }
    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        if n.kind() != "macro_invocation" {
            return None;
        }
        let mac = field_text(cx, n, "macro");
        let last = last_segment(mac, &["::", "."]);
        let unreach = last == "unreachable";
        if last != "assert" && last != "debug_assert" && !unreach {
            return None;
        }
        let tt = first_named_child_matching(n, &["token_tree"])?;
        let mut cursor = tt.walk();
        let tops: Vec<Node<'_>> = tt
            .children(&mut cursor)
            .filter(|c| c.kind() != "token_tree")
            .collect();
        let has_message = if unreach {
            tops.iter().any(|c| {
                let t = text(cx, *c);
                t != "("
                    && t != ")"
                    && t != "["
                    && t != "]"
                    && t != "{"
                    && t != "}"
                    && !t.trim().is_empty()
            })
        } else {
            tops.iter().any(|c| text(cx, *c) == ",")
        };
        let cmp = tops
            .iter()
            .any(|c| text(cx, *c) == "==" || text(cx, *c) == "!=");
        Some(Assertion {
            pos: pos_of(n),
            has_message,
            comparison: cmp,
        })
    }
    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
        if root.kind() != "source_file" {
            return None;
        }
        let mut uses = Vec::new();
        let mut mods = Vec::new();
        for child in named_children(root) {
            match child.kind() {
                "line_comment" | "block_comment" | "attribute_item" => {}
                "use_declaration" => {
                    if !has_child_kind(child, "visibility_modifier") {
                        return None;
                    }
                    uses.push(child);
                }
                "mod_item" => {
                    if child_by_field(child, "body").is_some() {
                        return None;
                    }
                    mods.push(child_by_field(child, "name").map_or("", |m| text(cx, m)));
                }
                _ => return None,
            }
        }
        if uses.is_empty() {
            return None;
        }
        let dir = parent_dir(cx.path);
        let stem = file_stem(cx.path);
        let base = if stem == "mod" || stem == "lib" || stem == "main" {
            dir.to_owned()
        } else if dir.is_empty() {
            stem.to_owned()
        } else {
            dir.to_owned() + "/" + stem
        };
        let mut targets = Vec::new();
        for u in uses {
            let mut cands = Vec::new();
            if let Some(m) = child_by_field(u, "argument").and_then(|a| use_first(cx, a))
                && mods.contains(&m)
            {
                cands.push(join_path(&base, m.to_owned() + ".rs"));
                cands.push(join_path(&base, m.to_owned() + "/mod.rs"));
            }
            targets.push(cands);
        }
        Some(Barrel { targets })
    }
}
