//! Lean 4 adapter: definitions as functions, `fun` as the closure.
//!
//! The vendored `Julian/tree-sitter-lean` grammar names nodes after Lean's
//! own `SyntaxNodeKind`s, so definition commands (`def`, `theorem` which
//! also covers `lemma`, `abbrev`, `opaque`, `axiom`, `instance`, `example`,
//! `constant`, `where_aux_def` helpers) lower directly to functions with
//! parameters taken from the `binders` children. `fun` is the only
//! closure form. `match` and tactic `induction` are matches with
//! `match_alt` arms; `if_then_else` is the branch. Application is generic
//! (`app`, including nested chains and tactic-head applications the
//! grammar leaves as plain terms), which is the call. There is no
//! dedicated loop node: `for`/`in` and `while` inside `do` blocks parse as
//! generic `app` chains, so loops stay unmapped.

use sloplint_core::facts::{Import, Pos, Visibility, WildcardArm};
use tree_sitter::Node;

use crate::hooks::{CallHead, Cx, FunctionHead, LangHooks, TypeHead, child_by_field};
use crate::spec::Role;

mod calls;
mod funcs;
mod items;

pub(crate) static TABLE: crate::lang::Table = &[
    ("def", Role::Function),
    ("theorem", Role::Function),
    ("abbrev", Role::Function),
    ("opaque", Role::Function),
    ("axiom", Role::Function),
    ("instance", Role::Function),
    ("example", Role::Function),
    ("constant", Role::Function),
    ("where_aux_def", Role::Function),
    ("fun", Role::Closure),
    ("if_then_else", Role::If),
    ("match", Role::Match),
    ("induction", Role::Match),
    ("app", Role::Call),
    ("let", Role::Decl),
    ("have", Role::Decl),
    ("block_assign", Role::Write),
    ("import", Role::Import),
    ("open", Role::Import),
    ("export", Role::Import),
    ("inductive", Role::TypeDef),
    ("structure", Role::TypeDef),
    ("line_comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("doc_comment", Role::Comment),
    ("module_doc_comment", Role::Comment),
    ("match_alt", Role::MatchArm),
];

pub(crate) struct LeanHooks;

pub(crate) static HOOKS: LeanHooks = LeanHooks;

pub(crate) const COMMENT_KINDS: &[&str] = &[
    "line_comment",
    "block_comment",
    "doc_comment",
    "module_doc_comment",
];

pub(crate) const FUNCTION_KINDS: &[&str] = &[
    "def",
    "theorem",
    "abbrev",
    "opaque",
    "axiom",
    "instance",
    "example",
    "constant",
    "where_aux_def",
];

/// Compiler-shipped roots; anything else names an external package.
pub(crate) const LEAN_STDLIB: &[&str] = &["Init", "Lean", "Std"];

/// The `declaration` wrapper holding definition commands also holds the
/// `decl_modifiers` (`private`) and `attributes` (`@[simp]`) siblings.
pub(crate) fn declaration_of<'a>(n: Node<'_>, cx: &'a Cx<'_, '_>) -> Option<Node<'a>> {
    let parent = cx.parent_of(n)?;
    (parent.kind() == "declaration").then_some(parent)
}

/// Term fields Lean wraps with their keyword (`field('type', seq(':', term))`,
/// `field('body', seq(':=', term))`): the raw field node is the keyword token,
/// so take the first named child carrying the field instead.
pub(crate) fn field_term<'a>(n: Node<'a>, field: &str) -> Option<Node<'a>> {
    let mut cursor = n.walk();
    n.children_by_field_name(field, &mut cursor)
        .find(|child| child.is_named())
}

pub(crate) fn lean_visibility<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Visibility {
    funcs::lean_visibility(cx, n)
}

impl LangHooks for LeanHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        FUNCTION_KINDS
            .contains(&n.kind())
            .then(|| funcs::function_head(cx, n))
    }

    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        _parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        funcs::closure_head(cx, n)
    }

    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        parent.is_some_and(|parent| {
            parent.kind() == "if_then_else"
                && child_by_field(parent, "else").is_some_and(|alt| alt.id() == n.id())
        })
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::decl_names_head(cx, n, out);
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::write_roots_head(cx, n, out);
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        calls::call_head(cx, n)
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        items::imports_head(cx, n, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        items::typedef_head(cx, n)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<WildcardArm> {
        items::match_arm_head(n, cx)
    }
}

#[cfg(test)]
mod tests {
    use crate::spec::{Role, spec};

    #[test]
    fn lean_only_kinds_keep_their_roles() {
        let spec = spec(sloplint_core::lang::Lang::Lean);
        let source = "-- identity\ndef id (x : Nat) : Nat := x\ndef useIt : Nat := id 0\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&spec.language)
            .expect("lean language loads");
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
                        ("def", Role::Function),
                        ("explicit_binder", Role::None),
                        ("identifier", Role::None),
                        ("app", Role::Call),
                        ("line_comment", Role::Comment),
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
