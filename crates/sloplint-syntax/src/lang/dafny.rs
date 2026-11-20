//! Dafny adapter: verification-aware language over its own grammar.
//!
//! Function-like units are `method` (with `lemma`), `function` (with
//! `predicate`), `constructor`, and `iterator` declarations. Specification
//! clauses (`requires`, `ensures`, `invariant`, `decreases`, `modifies`,
//! `reads`, and the refinement/`witness` family) are unmapped and skipped:
//! they constrain proofs, they are not branches, loops, or calls, and
//! counting them would punish verified code for being specified.
//! `assert` statements map to the assert role with a
//! silent shape for the same reason; only `expect`, which takes an optional
//! message, feeds the message rule.

use sloplint_core::facts::{Import, Pos};
use tree_sitter::Node;

use crate::hooks::{CallHead, Cx, FunctionHead, LangHooks, TypeHead, trimmed};
use crate::spec::Role;
mod calls;
mod funcs;
mod items;
mod shapes;

pub(crate) static TABLE: crate::lang::Table = &[
    ("method_decl", Role::Function),
    ("function_decl", Role::Function),
    ("constructor_decl", Role::Function),
    ("iterator_decl", Role::Function),
    ("lambda_expression", Role::Closure),
    ("if_statement", Role::If),
    ("if_expression", Role::If),
    ("fn_if_expression", Role::If),
    ("match_statement", Role::Match),
    ("match_expression", Role::Match),
    ("fn_match_expression", Role::Match),
    ("while_statement", Role::Loop),
    ("for_statement", Role::Loop),
    ("forall_statement", Role::Loop),
    ("call_expression", Role::Call),
    ("suffix_expression", Role::Call),
    ("print_statement", Role::Call),
    ("local_var_decl", Role::Decl),
    ("var_decl_statement", Role::Decl),
    ("fn_let_expression", Role::Decl),
    ("let_expression", Role::Decl),
    ("let_or_fail_expression", Role::Decl),
    ("assign_statement", Role::Write),
    ("include_directive", Role::Import),
    ("module_import", Role::Import),
    ("class_decl", Role::TypeDef),
    ("trait_decl", Role::TypeDef),
    ("datatype_decl", Role::TypeDef),
    ("newtype_decl", Role::TypeDef),
    ("synonym_type_decl", Role::TypeDef),
    ("line_comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("case_stmt_clause", Role::MatchArm),
    ("case_expr_clause", Role::MatchArm),
    ("fn_case_expr_clause", Role::MatchArm),
    ("return_statement", Role::Return),
    ("yield_statement", Role::Yield),
    ("assert_statement", Role::Assert),
    ("expect_statement", Role::Assert),
    ("fn_assert_expression", Role::Assert),
    ("fn_expect_expression", Role::Assert),
];

pub(crate) struct DafnyHooks;

pub(crate) static HOOKS: DafnyHooks = DafnyHooks;

impl LangHooks for DafnyHooks {
    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        funcs::function_head(cx, n)
    }

    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        let _ = parent;
        if n.kind() != "lambda_expression" {
            return None;
        }
        Some(funcs::closure_head(cx, n))
    }

    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        // `if` has no field names here, so the else alternative is the
        // parent's last named child — anything earlier (e.g. an
        // if-expression in the condition) is not an else-if.
        let is_last = parent.is_some_and(|p| {
            u32::try_from(p.named_child_count().saturating_sub(1))
                .ok()
                .and_then(|i| p.named_child(i))
                .is_some_and(|last| last.id() == n.id())
        });
        match n.kind() {
            "if_statement" => is_last && parent.is_some_and(|p| p.kind() == "if_statement"),
            // `else if` expression chains wrap each alternative in the
            // unary-expression chain (`equiv_expression` down to
            // `unary_expression`), so the inner if's direct parent is never
            // the outer if — climb the wrappers and check that the top of
            // the chain is the outer if's else branch.
            "if_expression" | "fn_if_expression" => {
                let mut cur = n;
                loop {
                    let Some(p) = cur.parent() else { return false };
                    match p.kind() {
                        "if_expression" | "fn_if_expression" => {
                            return u32::try_from(p.named_child_count().saturating_sub(1))
                                .ok()
                                .and_then(|i| p.named_child(i))
                                .is_some_and(|last| last.id() == cur.id());
                        }
                        "unary_expression" | "as_expression" | "bitvector_factor" | "factor"
                        | "term" | "shift_term" | "relational_expression"
                        | "logical_expression" | "implies_explies_expression"
                        | "equiv_expression" => cur = p,
                        _ => return false,
                    }
                }
            }
            _ => false,
        }
    }

    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
        // `local_var_decl` is a child of `var_decl_statement`, so the
        // declared name scopes to the block above the statement — the
        // default depth-1 would scope `var x := 0` to itself and later
        // `x := ...` writes would be missed.
        if node.kind() == "local_var_decl"
            && node
                .parent()
                .is_some_and(|p| p.kind() == "var_decl_statement")
        {
            return depth.saturating_sub(2);
        }
        if node.kind() == "for_statement" {
            // The loop variable scopes to the statement itself: it pops
            // when the `for` exits. The body block's own depth cannot be
            // used — the header's expressions sit at that depth too and
            // their exits would free the binding before the body runs.
            return depth;
        }
        let _ = cx;
        depth.saturating_sub(1)
    }

    fn skip_subtree(&self, node: Node<'_>, cx: &Cx<'_, '_>) -> bool {
        // Specification clauses constrain proofs, not runtime behavior —
        // their expressions are not lintable code.
        if node.kind() == "witness_clause" {
            // `witness e` supplies a compiled default; only
            // `ghost witness e` is proof-only.
            return trimmed(cx, node).starts_with("ghost");
        }
        matches!(
            node.kind(),
            "requires_clause"
                | "yield_requires_clause"
                | "ensures_clause"
                | "yield_ensures_clause"
                | "invariant_clause"
                | "decreases_clause"
                | "modifies_clause"
                | "reads_clause"
                | "refines_clause"
                | "replaces_clause"
        )
    }

    fn binds_flow_targets(&self, kind: &str) -> bool {
        kind == "for_statement"
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::decl_names_for(cx, n, out);
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        items::write_roots_for(cx, n, out);
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        items::imports_for(cx, n, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        items::typedef_for(cx, n)
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        calls::call_head(cx, n)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<sloplint_core::facts::WildcardArm> {
        items::match_arm_for(cx, n)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<sloplint_core::facts::Assertion> {
        items::assertion_for(cx, n)
    }

    fn block_scoped_binds(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::spec::{Role, spec};

    #[test]
    fn dafny_only_kinds_keep_their_roles() {
        let spec = spec(sloplint_core::lang::Lang::Dafny);
        let source = "// bump the counter\n/* block note */\nmodule M {\n\tmethod Bump(delta: int) returns (r: int)\n\t\trequires delta >= 0\n\t{\n\t\tvar t := delta;\n\t\tvar f := (x: int) => x * 2;\n\t\tvar cond := if t > 0 then 1 else 0;\n\t\tvar picked := match t case 0 => \"z\" case _ => \"o\";\n\t\texpect t >= 0, \"non-negative\";\n\t\tif t > 10 {\n\t\t\tprint \"big\";\n\t\t} else if t > 5 {\n\t\t\tprint \"mid\";\n\t\t}\n\t\twhile t > 0\n\t\t\tinvariant t >= 0\n\t\t{\n\t\t\tt := t - 1;\n\t\t}\n\t\tfor i := 0 to 10 {\n\t\t\tt := t + i;\n\t\t}\n\t\tmatch t\n\t\tcase 0 => print \"zero\";\n\t\tcase _ => print \"other\";\n\t\treturn t;\n\t}\n\tfunction Fib(n: nat): nat\n\t{\n\t\tassert n >= 0;\n\t\texpect n <= 20, \"small\";\n\t\tvar k := n;\n\t\tif k < 2 then k else Fib(k - 1) + Fib(k - 2)\n\t}\n\tfunction Describe(t: int): string\n\t{\n\t\tmatch t case 0 => \"zero\" case _ => \"other\"\n\t}\n\tlemma Helper(x: int)\n\t\trequires x > 0\n\t{\n\t\tassert x > 0;\n\t}\n}\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&spec.language)
            .expect("dafny language loads");
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
                        ("method_decl", Role::Function),
                        ("function_decl", Role::Function),
                        ("lambda_expression", Role::Closure),
                        ("if_statement", Role::If),
                        ("if_expression", Role::If),
                        ("fn_if_expression", Role::If),
                        ("match_statement", Role::Match),
                        ("match_expression", Role::Match),
                        ("fn_match_expression", Role::Match),
                        ("while_statement", Role::Loop),
                        ("for_statement", Role::Loop),
                        ("suffix_expression", Role::Call),
                        ("print_statement", Role::Call),
                        ("local_var_decl", Role::Decl),
                        ("var_decl_statement", Role::Decl),
                        ("fn_let_expression", Role::Decl),
                        ("assign_statement", Role::Write),
                        ("case_stmt_clause", Role::MatchArm),
                        ("case_expr_clause", Role::MatchArm),
                        ("fn_case_expr_clause", Role::MatchArm),
                        ("return_statement", Role::Return),
                        ("assert_statement", Role::Assert),
                        ("expect_statement", Role::Assert),
                        ("fn_assert_expression", Role::Assert),
                        ("fn_expect_expression", Role::Assert),
                        ("line_comment", Role::Comment),
                        ("block_comment", Role::Comment),
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
