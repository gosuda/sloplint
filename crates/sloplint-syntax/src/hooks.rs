//! The per-language adapter seam and the node helpers every adapter shares.

use sloplint_core::facts::{
    AmbientKind, Arg, ArgShape, Assertion, Barrel, CallKind, FieldCopy, Handler, ImplRef, Import,
    Param, Pos, TypeDef, WildcardArm,
};
use sloplint_core::lang::Lang;
use tree_sitter::Node;

/// What one language's hooks know about the node currently being visited.
pub(crate) struct Cx<'t, 's> {
    pub src: &'s str,
    pub path: &'s str,
    pub lang: Lang,
    pub is_test_file: bool,
    pub has_interface: bool,
    pub containers: &'t [Container],
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
    fn closure_as_function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        let _ = (n, cx);
        None
    }

    /// True when this `If`-role node is the else-if form of its parent.
    fn is_else_if(&self, n: Node<'_>) -> bool;

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
    let mut out = Vec::with_capacity(n.named_child_count());
    for i in 0..narrow(n.named_child_count()) {
        if let Some(child) = n.named_child(i) {
            out.push(child);
        }
    }
    out
}

/// The node's child reached through a named field.
pub(crate) fn child_by_field<'a>(n: Node<'a>, field: &str) -> Option<Node<'a>> {
    n.child_by_field_name(field)
}

/// The first child, named or anonymous, whose kind is in `kinds`.
pub(crate) fn first_of_kind<'a>(n: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    for i in 0..n.child_count() {
        let Some(child) = n.child(i) else {
            continue;
        };
        if kinds.contains(&child.kind()) {
            return Some(child);
        }
    }
    None
}

/// True when any child, named or anonymous, has the given kind.
pub(crate) fn has_child_kind(n: Node<'_>, kind: &str) -> bool {
    (0..n.child_count()).any(|i| n.child(i).is_some_and(|child| child.kind() == kind))
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
    for i in 0..n.child_count() {
        let Some(child) = n.child(i) else {
            continue;
        };
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

/// Collects the comment siblings that end on the line before the current
/// header start; `skip_kinds` (attributes, decorators) extend the header.
pub(crate) fn preceding_comments<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    comment_kinds: &[&str],
    skip_kinds: &[&str],
) -> (Vec<&'s str>, Option<Pos>) {
    let mut doc: Vec<&'s str> = Vec::new();
    let mut doc_pos = None;
    let mut header_row = n.start_position().row;
    let mut current = n.prev_sibling();
    while let Some(sibling) = current {
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
            doc.push(text(cx, sibling));
            doc_pos = Some(pos_of(sibling));
            header_row = sibling.start_position().row;
        } else if skip_kinds.contains(&kind) {
            header_row = sibling.start_position().row;
        } else {
            break;
        }
        current = sibling.prev_sibling();
    }
    doc.reverse();
    (doc, doc_pos)
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
