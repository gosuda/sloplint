//! The per-language adapter seam and the node helpers every adapter shares.

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, Barrel, CallKind, FieldCopy, Handler, ImplRef, Import,
    Param, Pos, TypeDef, WildcardArm,
};
use std::cell::RefCell;
use std::collections::HashMap;

use sloplint_core::lang::Lang;
use tree_sitter::Node;

/// Explicit member state set by marker children, memoized across one walk.
///
/// A member's governing state is the last marker child before it inside the same
/// container. The first query for any member runs one forward pass over every child
/// of its container and records the state before each child, so later queries for
/// siblings are lookups rather than backward scans over the container.
#[derive(Default)]
pub(crate) struct MemberStates {
    before: RefCell<HashMap<usize, Option<bool>>>,
}

impl MemberStates {
    /// The state set by the last marker child of `container` before `member`, if any.
    ///
    /// `marker` reads one child and returns the state it sets, or `None` when the
    /// child sets nothing. `member` must be a child of `container`.
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

    /// Records the state before every child of `container` in one forward pass.
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

/// What one language's hooks know about the node currently being visited.
pub(crate) struct Cx<'t, 's> {
    pub src: &'s str,
    pub path: &'s str,
    pub lang: Lang,
    pub is_test_file: bool,
    /// The walk's memo of member states, shared by every node the walk visits.
    pub members: &'t MemberStates,
    containers: &'t [Container],
    exports: &'t [&'s str],
    /// The nodes entered so far under each open ancestor, outermost level first.
    ///
    /// The last entry of each level is the open ancestor itself, or the visited
    /// node on the innermost level, and the entries before it are its earlier
    /// siblings in tree order.
    levels: &'t [Vec<Node<'t>>],
}

impl<'t, 's> Cx<'t, 's> {
    /// A view of one file with no container open, nothing exported, and no node entered.
    pub(crate) fn file(
        src: &'s str,
        path: &'s str,
        lang: Lang,
        is_test_file: bool,
        members: &'t MemberStates,
    ) -> Self {
        Self {
            src,
            path,
            lang,
            is_test_file,
            members,
            containers: &[],
            exports: &[],
            levels: &[],
        }
    }

    /// The same view of the file with the walker's open containers, top level
    /// exports, and entered nodes attached.
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

impl Cx<'_, '_> {
    /// True when the file's top level exports `name` through a local export statement.
    pub(crate) fn is_exported(&self, name: &str) -> bool {
        self.exports
            .binary_search_by(|probe| (*probe).cmp(name))
            .is_ok()
    }

    /// The innermost open container; `None` at the file's top level.
    pub(crate) fn innermost_container(&self) -> Option<Container> {
        self.containers.last().copied()
    }

    /// True when any open container is a test module.
    pub(crate) fn in_test_module(&self) -> bool {
        self.containers.contains(&Container::TestModule)
    }

    /// Whether the nearest open class is exported; `None` when none is open.
    pub(crate) fn nearest_class_exported(&self) -> Option<bool> {
        self.containers.iter().rev().find_map(|c| match *c {
            Container::Class { exported, .. } => Some(exported),
            _ => None,
        })
    }

    /// Whether the nearest open class implements an interface; `None` when none is open.
    pub(crate) fn nearest_class_implements(&self) -> Option<bool> {
        self.containers.iter().rev().find_map(|c| match *c {
            Container::Class { implements, .. } => Some(implements),
            _ => None,
        })
    }

    /// Whether the nearest open implementation is interface-bound; `None` when none is open.
    pub(crate) fn nearest_impl_interface_bound(&self) -> Option<bool> {
        self.containers.iter().rev().find_map(|c| match *c {
            Container::Impl { interface_bound } => Some(interface_bound),
            _ => None,
        })
    }
}

/// A scope that changes how nested names, visibility, and tests are judged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Container {
    Impl { interface_bound: bool },
    Trait { public: bool },
    Class { exported: bool, implements: bool },
    TestModule,
}

/// Everything a language adapter says about one function-like node.
pub(crate) struct FunctionHead<'s> {
    pub name: &'s str,
    pub name_pos: Pos,
    pub public: bool,
    pub interface_bound: bool,
    pub in_test: bool,
    pub params: Vec<Param<'s>>,
    pub return_type: Option<&'s str>,
    pub missing_annotations: Vec<&'s str>,
    pub doc: Vec<&'s str>,
    pub doc_pos: Option<Pos>,
    pub sole_call: Option<usize>,
}

/// Everything a language adapter says about one call-like node.
pub(crate) struct CallHead<'s> {
    pub callee: &'s str,
    pub kind: CallKind,
    pub callee_root: Option<&'s str>,
    pub args: Vec<Arg<'s>>,
}

/// The type declaration, implementation references, and container one type node contributes.
pub(crate) struct TypeHead<'s> {
    pub def: Option<TypeDef<'s>>,
    pub impls: Vec<ImplRef<'s>>,
    pub container: Option<Container>,
}

/// Node-kind hooks that turn one language's tree shapes into walker facts.
pub(crate) trait LangHooks: Sync {
    /// Reads a `Function`-role node as a named function; `None` skips it.
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>>;

    /// Reads a `Closure`-role node as a named function when the language names it.
    ///
    /// `parent` is the node's parent as the walker tracks it, so no hook climbs the tree.
    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        let _ = (n, parent, cx);
        None
    }

    /// True when a function-like node declares plain `throws` with no error type.
    fn untyped_throws(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
        let _ = (n, cx);
        false
    }

    /// True when this `If`-role node is the else-if form of `parent`, its walker-tracked parent.
    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool;

    /// True when a `Call`-role node spells a control construct (R's `switch`).
    fn control_call(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> bool {
        let _ = (n, cx);
        false
    }

    /// Names and positions bound by a `Decl` or `Bind` node.
    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let _ = (n, cx, out);
    }

    /// Write-target roots of a `Write`, `Bind`, or call node.
    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        let _ = (n, cx, out);
    }

    /// Reads a `Call`, `MacroCall`, or `NewCall` node as a call fact.
    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>>;

    /// Imports or re-exports contributed by an `Import`, `Export`, or call node.
    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        let _ = (n, cx, out);
    }

    /// Type facts, implementation references, and a container for a `TypeDef` node.
    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        let _ = (n, cx);
        None
    }

    /// The container a `Module`, `Impl`, or similar node opens.
    fn container(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Container> {
        let _ = (n, cx);
        None
    }

    /// Wildcard-arm fact for a `MatchArm` node.
    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        let _ = (n, cx);
        None
    }

    /// Error-handler fact for a `Catch`, `If`, or `MatchArm` node.
    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        let _ = (n, cx);
        None
    }

    /// Field-copy fact for a `StructLit` node or a call with keyword arguments.
    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        let _ = (n, cx);
        None
    }

    /// Ambient-state sites for global, static, member, and call nodes.
    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        let _ = (n, cx, in_function, out);
    }

    /// The dynamic-symbol mechanism a node resolves names through, if any.
    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        let _ = (n, cx);
        None
    }

    /// Assertion fact for an `Assert` node or an assertion-shaped call.
    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        let _ = (n, cx);
        None
    }

    /// Names the file's top level exports without a declaration, read once before the walk.
    fn exported_names<'s>(&self, root: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        let _ = (root, cx, out);
    }

    /// Barrel-file fact for the whole file, read once after the walk.
    fn barrel(&self, root: Node<'_>, cx: &Cx<'_, '_>) -> Option<Barrel> {
        let _ = (root, cx);
        None
    }
}

/// Narrows a byte-or-row quantity to the fact contract's `u32`.
///
/// The pipeline caps files at `max_file_bytes` (2 MiB), so a real position can
/// never exceed `u32::MAX`; the clamp only guards the impossible tail.
pub(crate) fn narrow(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// The 1-based line and byte offset where a node starts.
pub(crate) fn pos_of(n: Node<'_>) -> Pos {
    Pos {
        line: narrow(n.start_position().row) + 1,
        byte: narrow(n.start_byte()),
    }
}

/// The node's source text; empty when the byte range is not a valid slice.
pub(crate) fn text<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    slice(cx.src, n.start_byte(), n.end_byte())
}

/// The node's source text with surrounding whitespace removed.
pub(crate) fn trimmed<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    text(cx, n).trim()
}

/// The node's named children in order.
pub(crate) fn named_children(n: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = n.walk();
    n.named_children(&mut cursor).collect()
}

/// The node's first named child.
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

/// The node's last named child.
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

/// The first named child satisfying `pred`, in order.
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

/// The last named child satisfying `pred`, in order.
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

/// True when any named child satisfies `pred`.
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

/// The first named child whose kind is in `kinds`.
pub(crate) fn first_named_child_matching<'a>(n: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    find_named_child(n, |child| kinds.contains(&child.kind()))
}

/// The number of named children satisfying `pred`.
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

/// The first mapped value from the named children, in order; `None` when
/// every child maps to `None`.
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

/// The single named child satisfying `pred`; `None` for zero or two or more.
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

/// The node's child reached through a named field.
pub(crate) fn child_by_field<'a>(n: Node<'a>, field: &str) -> Option<Node<'a>> {
    n.child_by_field_name(field)
}

/// The first child, named or anonymous, whose kind is in `kinds`.
pub(crate) fn first_of_kind<'a>(n: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    let mut cursor = n.walk();
    n.children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()))
}

/// True when any child, named or anonymous, has the given kind.
pub(crate) fn has_child_kind(n: Node<'_>, kind: &str) -> bool {
    let mut cursor = n.walk();
    n.children(&mut cursor).any(|child| child.kind() == kind)
}

/// The text after the last occurrence of any separator (`::`, `.`, `/`, `#`, `\`).
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

/// Removes one matching pair of surrounding quotes; other text passes through.
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

/// Follows the given child fields from a write target down to its root name.
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

/// The `Str` shape built from the node's fragment-kind children.
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

/// True when the node is a three-child binary expression whose operator is
/// `op_text` and at least one operand is a string-kind node.
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

/// True for a plain comment that issues a `sloplint-allow` directive.
///
/// Doc comments (`///`, `//!`, `/**`, `/*!`, `(**`, `(*!`) only describe a
/// directive and never issue one, so they stay documentation.
pub(crate) fn is_directive_comment(text: &str) -> bool {
    if !text.contains("sloplint-allow") {
        return false;
    }
    let head = text.trim_start();
    !["///", "//!", "/**", "/*!", "(**", "(*!"]
        .iter()
        .any(|prefix| head.starts_with(prefix))
}

/// Collects the comment siblings that end on the line before the current
/// header start; `skip_kinds` (attributes, decorators) and `sloplint-allow`
/// directive comments extend the header without joining the doc.
///
/// `n` is the header anchor: the visited node or one of its open ancestors, such
/// as the export statement or decorated definition that wraps a declaration.
///
/// # Panics
///
/// Panics when `n` is neither the visited node nor one of its ancestors, which
/// is an adapter bug: only those nodes have recorded earlier siblings.
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

/// Shared skeleton behind JavaScript `sole_call_of` and TypeScript
/// `ts_sole_call`: a body that is itself a call, or a block holding exactly
/// one statement that resolves to a call.
///
/// `block_stmt` resolves a `statement_block` to its single meaningful child
/// and `stmt_call` resolves one statement, so each grammar keeps its own
/// child filtering where the grammars genuinely differ. Both grammars name
/// the block `statement_block`.
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

/// Slices source text at a byte range; empty when the range is not a valid slice.
pub(crate) fn slice(src: &str, start: usize, end: usize) -> &str {
    let end = end.min(src.len());
    let start = start.min(end);
    if src.is_char_boundary(start) && src.is_char_boundary(end) {
        &src[start..end]
    } else {
        ""
    }
}
