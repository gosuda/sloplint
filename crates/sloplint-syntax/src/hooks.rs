use sloplint_core::facts::{
    AmbientKind, Arg, Assertion, Barrel, CallKind, FieldCopy, Handler, ImplRef, Import, Param, Pos,
    TypeDef, Visibility, WildcardArm,
};
use sloplint_core::role::{CodeRole, RoleRegion};
use tree_sitter::Node;

pub(crate) use crate::cx::{ChainRoots, Cx, FileNameSets, FileSetKind, Link, MemberStates};
pub(crate) use crate::emit::{attributed_region, region_of};
pub(crate) use crate::shapes::{binary_concat, string_arg_shape};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShadowKind {
    None,
    Decls,
    DeclsAndBinds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Container {
    Impl { interface_bound: bool },
    Trait { public: bool },
    Class { exported: bool, implements: bool },
    TestModule,
}

pub(crate) struct FunctionHead<'s> {
    pub name: &'s str,
    pub name_pos: Pos,
    pub visibility: Visibility,
    pub signature_forced: bool,
    pub entry_point: bool,
    pub in_test: bool,
    pub is_overload: bool,
    pub is_deprecated: bool,
    pub is_decorated: bool,
    pub is_trait_item: bool,
    pub region: Option<RoleRegion>,
    pub params: Vec<Param<'s>>,
    pub shadow_params: Vec<&'s str>,
    pub receiver: Option<&'s str>,
    pub return_type: Option<&'s str>,
    pub missing_annotations: Vec<&'s str>,
    pub doc: Vec<&'s str>,
    pub doc_pos: Option<Pos>,
    pub sole_call: Option<usize>,
}

pub(crate) struct CallHead<'s> {
    pub callee: &'s str,
    pub kind: CallKind,
    pub callee_root: Option<&'s str>,
    pub is_simple_path: bool,
    pub initializes_binding: bool,
    pub args: Vec<Arg<'s>>,
}

pub(crate) struct TypeHead<'s> {
    pub def: Option<TypeDef<'s>>,
    pub impls: Vec<ImplRef<'s>>,
    pub container: Option<Container>,
}

pub(crate) trait LangHooks: Sync {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>>;

    /// `parent` is the walker-tracked parent, not `n.parent()`.
    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        let _ = (n, parent, cx);
        None
    }

    fn untyped_throws(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
        let _ = (n, cx);
        false
    }

    fn is_else_if(&self, _n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        parent.is_some_and(|p| p.kind() == "else_clause")
    }

    fn is_ternary_alternate(&self, _n: Node<'_>, _parent: Option<Node<'_>>) -> bool {
        false
    }

    fn control_call(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
        let _ = (n, cx);
        false
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let _ = (n, cx, out);
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let _ = (n, cx, out);
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>>;

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        let _ = (n, cx, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        let _ = (n, cx);
        None
    }

    fn container(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Container> {
        let _ = (n, cx);
        None
    }

    fn call_marker(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<CodeRole> {
        let _ = (n, cx);
        None
    }

    fn module_region(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<CodeRole> {
        let _ = (n, cx);
        None
    }

    fn module_leading<'t>(&self, n: Node<'t>, cx: &Cx<'_, '_>) -> Option<Node<'t>> {
        let _ = (n, cx);
        None
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        let _ = (n, cx);
        None
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        let _ = (n, cx);
        None
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        let _ = (n, cx);
        None
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        let _ = (n, cx, in_function, out);
    }

    /// What the walker treats as shadowing an in-function `GlobalWrite`.
    /// Only block-scoped declarations opt in; dynamic languages stay off.
    fn shadow_filter(&self) -> ShadowKind {
        ShadowKind::None
    }

    fn shadow_decl_kind(&self, kind: &str) -> bool {
        let _ = kind;
        false
    }

    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        let _ = (n, cx, out);
    }

    fn ambient_bind(&self) -> bool {
        false
    }

    fn decl_visible_in_initializer(&self) -> bool {
        true
    }

    fn decl_piece(&self, kind: &str) -> bool {
        let _ = kind;
        false
    }

    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
        let _ = (node, cx);
        depth.saturating_sub(1)
    }

    fn block_scoped_binds(&self) -> bool {
        false
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        let _ = (n, cx);
        None
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        let _ = (n, cx);
        None
    }

    fn exported_names<'s>(&self, root: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        let _ = (root, cx, out);
    }

    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
        let _ = (root, cx);
        None
    }
}

pub(crate) fn any_preceding(
    n: Node<'_>,
    target: &str,
    skip: &[&str],
    mut hit: impl FnMut(Node<'_>) -> bool,
) -> bool {
    let mut cur = n.prev_sibling();
    while let Some(sib) = cur {
        if sib.kind() == target {
            if hit(sib) {
                return true;
            }
        } else if !skip.contains(&sib.kind()) {
            break;
        }
        cur = sib.prev_sibling();
    }
    false
}
pub(crate) fn earliest_preceding<'a>(n: Node<'a>, target: &str, skip: &[&str]) -> Option<Node<'a>> {
    let mut cur = n.prev_sibling();
    let mut found = None;
    while let Some(sib) = cur {
        if sib.kind() == target {
            found = Some(sib);
        } else if !skip.contains(&sib.kind()) {
            break;
        }
        cur = sib.prev_sibling();
    }
    found
}
/// Files are capped at `max_file_bytes` (2 MiB), so a real position never exceeds `u32::MAX`.
pub(crate) fn narrow(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

pub(crate) fn pos_of(n: Node<'_>) -> Pos {
    Pos {
        line: narrow(n.start_position().row) + 1,
        byte: narrow(n.start_byte()),
    }
}

pub(crate) fn text<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    slice(cx.src, n.start_byte(), n.end_byte())
}

pub(crate) fn trimmed<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    text(cx, n).trim()
}

pub(crate) fn named_children(n: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = n.walk();
    n.named_children(&mut cursor).collect()
}

pub(crate) fn code_children(n: Node<'_>) -> Vec<Node<'_>> {
    named_children(n)
        .into_iter()
        .filter(|child| child.kind() != "comment")
        .collect()
}

pub(crate) fn first_named_child<'a>(n: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = n.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        if child.is_named() {
            return Some(child);
        }
        if !cursor.goto_next_sibling() {
            return None;
        }
    }
}

pub(crate) fn last_named_child<'a>(n: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = n.walk();
    if !cursor.goto_last_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        if child.is_named() {
            return Some(child);
        }
        if !cursor.goto_previous_sibling() {
            return None;
        }
    }
}

pub(crate) fn find_named_child<'a>(
    n: Node<'a>,
    mut pred: impl FnMut(Node<'a>) -> bool,
) -> Option<Node<'a>> {
    let mut cursor = n.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        if child.is_named() && pred(child) {
            return Some(child);
        }
        if !cursor.goto_next_sibling() {
            return None;
        }
    }
}

pub(crate) fn rfind_named_child<'a>(
    n: Node<'a>,
    mut pred: impl FnMut(Node<'a>) -> bool,
) -> Option<Node<'a>> {
    let mut cursor = n.walk();
    if !cursor.goto_last_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        if child.is_named() && pred(child) {
            return Some(child);
        }
        if !cursor.goto_previous_sibling() {
            return None;
        }
    }
}

pub(crate) fn any_named_child(n: Node<'_>, mut pred: impl FnMut(Node<'_>) -> bool) -> bool {
    let mut cursor = n.walk();
    if !cursor.goto_first_child() {
        return false;
    }
    loop {
        let child = cursor.node();
        if child.is_named() && pred(child) {
            return true;
        }
        if !cursor.goto_next_sibling() {
            return false;
        }
    }
}

pub(crate) fn first_named_child_matching<'a>(n: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    find_named_child(n, |child| kinds.contains(&child.kind()))
}

pub(crate) fn count_named_child(n: Node<'_>, mut pred: impl FnMut(Node<'_>) -> bool) -> usize {
    let mut cursor = n.walk();
    if !cursor.goto_first_child() {
        return 0;
    }
    let mut count = 0;
    loop {
        let child = cursor.node();
        if child.is_named() && pred(child) {
            count += 1;
        }
        if !cursor.goto_next_sibling() {
            return count;
        }
    }
}

pub(crate) fn find_map_named_child<'a, T>(
    n: Node<'a>,
    mut f: impl FnMut(Node<'a>) -> Option<T>,
) -> Option<T> {
    let mut cursor = n.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        if child.is_named()
            && let Some(value) = f(child)
        {
            return Some(value);
        }
        if !cursor.goto_next_sibling() {
            return None;
        }
    }
}

pub(crate) fn only_named_child_matching<'a>(
    n: Node<'a>,
    mut pred: impl FnMut(Node<'a>) -> bool,
) -> Option<Node<'a>> {
    let mut cursor = n.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    let mut found = None;
    loop {
        let child = cursor.node();
        if child.is_named() && pred(child) {
            if found.is_some() {
                return None;
            }
            found = Some(child);
        }
        if !cursor.goto_next_sibling() {
            return found;
        }
    }
}

pub(crate) fn child_by_field<'a>(n: Node<'a>, field: &str) -> Option<Node<'a>> {
    n.child_by_field_name(field)
}

pub(crate) fn first_of_kind<'a>(n: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    let mut cursor = n.walk();
    n.children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()))
}

pub(crate) fn has_child_kind(n: Node<'_>, kind: &str) -> bool {
    let mut cursor = n.walk();
    n.children(&mut cursor).any(|child| child.kind() == kind)
}

pub(crate) fn last_segment<'a>(text: &'a str, seps: &[&str]) -> &'a str {
    let mut best: Option<(usize, usize)> = None;
    for sep in seps {
        if let Some(at) = text.rfind(sep)
            && best.is_none_or(|(pos, _)| at > pos)
        {
            best = Some((at, sep.len()));
        }
    }
    match best {
        Some((at, len)) => &text[at + len..],
        None => text,
    }
}

pub(crate) fn strip_quotes(text: &str) -> &str {
    let bytes = text.as_bytes();
    let Some(&first) = bytes.first() else {
        return text;
    };
    let Some(&last) = bytes.last() else {
        return text;
    };
    if bytes.len() < 2 || !matches!(first, b'"' | b'\'' | b'`') || last != first {
        return text;
    }
    &text[1..text.len() - 1]
}

pub(crate) fn ident_root<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    object_fields: &[&str],
) -> Option<(&'s str, Pos)> {
    let mut current = n;
    loop {
        if current.named_child_count() == 0 {
            return Some((text(cx, current), pos_of(current)));
        }
        let next = object_fields
            .iter()
            .find_map(|field| current.child_by_field_name(field))?;
        current = next;
    }
}

pub(crate) fn spine_has_call(n: Node<'_>, object_field: &str, transparent: &[&str]) -> bool {
    let mut current = n;
    loop {
        match current.kind() {
            "call_expression" | "new_expression" => return true,
            "member_expression" | "subscript_expression" | "selector_expression" => {
                let Some(next) = child_by_field(current, object_field) else {
                    return false;
                };
                current = next;
            }
            kind if transparent.contains(&kind) => {
                let Some(next) = current.named_child(0) else {
                    return false;
                };
                current = next;
            }
            _ => return false,
        }
    }
}

pub(crate) fn is_directive_comment(text: &str) -> bool {
    if !text.contains("sloplint-allow") {
        return false;
    }
    let head = text.trim_start();
    !["///", "//!", "/**", "/*!", "(**", "(*!"]
        .iter()
        .any(|prefix| head.starts_with(prefix))
}

pub(crate) fn preceding_comments<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    comment_kinds: &[&str],
    skip_kinds: &[&str],
) -> (Vec<&'s str>, Option<Pos>) {
    let (anchor, earlier) = cx
        .levels
        .iter()
        .rev()
        .filter_map(|level| level.split_last())
        .find(|(last, _)| last.id() == n.id())
        .expect("a header anchor is the visited node or one of its ancestors");
    let mut doc: Vec<&'s str> = Vec::new();
    let mut doc_pos = None;
    let mut header_row = anchor.start_position().row;
    for &sibling in earlier.iter().rev() {
        let kind = sibling.kind();
        if comment_kinds.contains(&kind) {
            let end = sibling.end_position();
            let last_row = if end.column == 0 {
                end.row.saturating_sub(1)
            } else {
                end.row
            };
            if last_row + 1 != header_row {
                break;
            }
            let comment = text(cx, sibling);
            if !is_directive_comment(comment) {
                doc.push(comment);
                doc_pos = Some(pos_of(sibling));
            }
            header_row = sibling.start_position().row;
        } else if skip_kinds.contains(&kind) {
            header_row = sibling.start_position().row;
        } else {
            break;
        }
    }
    doc.reverse();
    (doc, doc_pos)
}

pub(crate) fn sole_call_id(
    body: Node<'_>,
    block_stmt: impl FnOnce(Node<'_>) -> Option<Node<'_>>,
    stmt_call: impl FnOnce(Node<'_>) -> Option<usize>,
) -> Option<usize> {
    if body.kind() == "call_expression" {
        return Some(body.id());
    }
    if body.kind() != "statement_block" {
        return None;
    }
    stmt_call(block_stmt(body)?)
}

pub(crate) fn slice(src: &str, start: usize, end: usize) -> &str {
    let end = end.min(src.len());
    let start = start.min(end);
    if src.is_char_boundary(start) && src.is_char_boundary(end) {
        &src[start..end]
    } else {
        ""
    }
}
