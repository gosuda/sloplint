use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, Barrel, CallKind, FieldCopy, Handler, ImplRef, Import,
    Param, Pos, TypeDef, WildcardArm,
};
use sloplint_core::lang::Lang;
use sloplint_core::role::{CodeRole, RoleRegion};
use std::cell::RefCell;
use std::collections::HashMap;
use tree_sitter::Node;

#[derive(Default)]
pub(crate) struct MemberStates {
    before: RefCell<HashMap<usize, Option<bool>>>,
}

impl MemberStates {
    pub(crate) fn state_before(
        &self,
        container: Node<'_>,
        member: Node<'_>,
        marker: impl Fn(Node<'_>) -> Option<bool>,
    ) -> Option<bool> {
        let known = self.before.borrow().contains_key(&member.id());
        if !known {
            self.record(container, marker);
        }
        let before = self.before.borrow();
        let state = before.get(&member.id());
        debug_assert!(state.is_some(), "a member is a child of its container");
        state.copied().flatten()
    }

    fn record(&self, container: Node<'_>, marker: impl Fn(Node<'_>) -> Option<bool>) {
        let mut state = None;
        let mut pass = Vec::new();
        let mut cursor = container.walk();
        for child in container.children(&mut cursor) {
            pass.push((child.id(), state));
            if let Some(set) = marker(child) {
                state = Some(set);
            }
        }
        self.before.borrow_mut().extend(pass);
    }
}

pub(crate) enum Link<'n> {
    Next(Node<'n>),
    Root(Node<'n>),
}

#[derive(Default)]
pub(crate) struct ChainRoots {
    known: RefCell<HashMap<(u8, usize), (usize, usize)>>,
    path: RefCell<Vec<usize>>,
}

impl ChainRoots {
    fn root<'n>(
        &self,
        family: u8,
        start: Node<'n>,
        step: impl Fn(Node<'n>) -> Link<'n>,
    ) -> (usize, usize) {
        let mut path = std::mem::take(&mut *self.path.borrow_mut());
        path.clear();
        let mut current = start;
        let found = loop {
            let next = match step(current) {
                Link::Next(next) => next,
                Link::Root(root) => break (root.start_byte(), root.end_byte()),
            };
            path.push(current.id());
            let known = self.known.borrow().get(&(family, next.id())).copied();
            if let Some(known) = known {
                break known;
            }
            current = next;
        };
        if !path.is_empty() {
            self.known
                .borrow_mut()
                .extend(path.drain(..).map(|id| ((family, id), found)));
        }
        *self.path.borrow_mut() = path;
        found
    }
}

pub(crate) struct Cx<'t, 's> {
    pub src: &'s str,
    pub path: &'s str,
    pub lang: Lang,
    pub members: &'t MemberStates,
    chains: &'t ChainRoots,
    containers: &'t [Container],
    exports: &'t [&'s str],
    /// Per level, outermost first: earlier siblings in tree order, then the open ancestor; the innermost level ends at the visited node.
    levels: &'t [Vec<Node<'t>>],
}

impl<'t, 's> Cx<'t, 's> {
    pub(crate) fn file(
        src: &'s str,
        path: &'s str,
        lang: Lang,
        members: &'t MemberStates,
        chains: &'t ChainRoots,
    ) -> Self {
        Self {
            src,
            path,
            lang,
            members,
            chains,
            containers: &[],
            exports: &[],
            levels: &[],
        }
    }

    pub(crate) fn scoped(
        self,
        containers: &'t [Container],
        exports: &'t [&'s str],
        levels: &'t [Vec<Node<'t>>],
    ) -> Self {
        Self {
            containers,
            exports,
            levels,
            ..self
        }
    }
}

impl<'s> Cx<'_, 's> {
    /// Roots are memoized per `family`: each `step` function needs its own id and one fixed answer per node.
    pub(crate) fn chain_root<'n>(
        &self,
        family: u8,
        start: Node<'n>,
        step: impl Fn(Node<'n>) -> Link<'n>,
    ) -> &'s str {
        let (from, to) = self.chains.root(family, start, step);
        slice(self.src, from, to)
    }
}

impl Cx<'_, '_> {
    pub(crate) fn is_exported(&self, name: &str) -> bool {
        self.exports
            .binary_search_by(|probe| (*probe).cmp(name))
            .is_ok()
    }

    pub(crate) fn innermost_container(&self) -> Option<Container> {
        self.containers.last().copied()
    }

    pub(crate) fn inside_test_module(&self) -> bool {
        self.containers.contains(&Container::TestModule)
    }

    pub(crate) fn nearest_class_exported(&self) -> Option<bool> {
        self.containers.iter().rev().find_map(|c| match *c {
            Container::Class { exported, .. } => Some(exported),
            _ => None,
        })
    }

    pub(crate) fn nearest_class_implements(&self) -> Option<bool> {
        self.containers.iter().rev().find_map(|c| match *c {
            Container::Class { implements, .. } => Some(implements),
            _ => None,
        })
    }

    pub(crate) fn nearest_impl_interface_bound(&self) -> Option<bool> {
        self.containers.iter().rev().find_map(|c| match *c {
            Container::Impl { interface_bound } => Some(interface_bound),
            _ => None,
        })
    }
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
    pub public: bool,
    pub interface_bound: bool,
    pub region: Option<RoleRegion>,
    pub params: Vec<Param<'s>>,
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

pub(crate) fn region_of(n: Node<'_>, role: CodeRole) -> RoleRegion {
    RoleRegion {
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
        role,
    }
}
pub(crate) fn attributed_region(
    item: Node<'_>,
    leading: Option<Node<'_>>,
    role: CodeRole,
) -> RoleRegion {
    let mut start = item.start_position().row;
    if let Some(extra) = leading {
        start = start.min(extra.start_position().row);
    }
    RoleRegion {
        start_line: narrow(start) + 1,
        end_line: narrow(item.end_position().row) + 1,
        role,
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

pub(crate) fn string_arg_shape<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    fragment_kinds: &[&str],
) -> ArgShape<'s> {
    let mut segments: Vec<&'s str> = Vec::new();
    let mut cursor = n.walk();
    for child in n.children(&mut cursor) {
        if fragment_kinds.contains(&child.kind()) {
            segments.push(text(cx, child));
        }
    }
    ArgShape::Str {
        longest_static: sloplint_core::text::longest_static_chunk(&segments),
    }
}

pub(crate) fn binary_concat<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    op_text: &str,
    string_kinds: &[&str],
) -> bool {
    let (Some(left), Some(op), Some(right)) = (n.child(0), n.child(1), n.child(2)) else {
        return false;
    };
    if n.child_count() != 3 || text(cx, op) != op_text {
        return false;
    }
    string_kinds.contains(&left.kind()) || string_kinds.contains(&right.kind())
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
