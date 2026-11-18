//! LoGOs adapter: the Go rules over a Go-shaped grammar plus the LoGOs forms.
//!
//! The vendored `tree-sitter-logos` grammar extends the Go grammar, so every
//! Go node name the Go adapter reads is produced unchanged. This table adds
//! the LoGOs-only statements and expressions; the hooks reuse `go::GoHooks`
//! for everything except `lambda_expression`, which the Go grammar does not
//! have.

use sloplint_core::facts::{
    AmbientKind, Assertion, FieldCopy, Handler, Import, Param, Pos, PrimitiveKind, Visibility,
};
use tree_sitter::Node;

use super::go;
use crate::hooks::{
    CallHead, Container, Cx, FunctionHead, LangHooks, ShadowKind, TypeHead, child_by_field,
    named_children, pos_of, text,
};
use crate::spec::Role;

pub(crate) static TABLE: crate::lang::Table = &[
    ("function_declaration", Role::Function),
    ("method_declaration", Role::Function),
    ("func_literal", Role::Closure),
    ("lambda_expression", Role::Closure),
    ("if_statement", Role::If),
    ("const_if_statement", Role::If),
    ("expression_switch_statement", Role::Match),
    ("type_switch_statement", Role::Match),
    ("const_switch_statement", Role::Match),
    ("switch_expression", Role::Match),
    ("select_statement", Role::Match),
    ("for_statement", Role::Loop),
    ("const_for_statement", Role::Loop),
    ("call_expression", Role::Call),
    ("var_declaration", Role::Decl),
    ("short_var_declaration", Role::Bind),
    ("assignment_statement", Role::Write),
    ("inc_statement", Role::Write),
    ("return_statement", Role::Return),
    ("yield_statement", Role::Yield),
    ("dec_statement", Role::Write),
    ("import_declaration", Role::Import),
    ("type_spec", Role::TypeDef),
    ("comment", Role::Comment),
    ("default_case", Role::MatchArm),
    ("composite_literal", Role::StructLit),
];

pub(crate) struct LogosHooks;

pub(crate) static HOOKS: LogosHooks = LogosHooks;

fn is_test_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .ends_with("_test.lgo")
}

fn lambda_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> FunctionHead<'s> {
    let body = child_by_field(n, "body");
    let mut params = Vec::new();
    for child in named_children(n) {
        if body.is_some_and(|b| b.id() == child.id()) {
            break;
        }
        if child.kind() == "identifier" {
            params.push(Param {
                name: text(cx, child),
                ty: None,
                primitive_kind: PrimitiveKind::Other,
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: false,
            });
        }
    }
    FunctionHead {
        shadow_params: Vec::new(),
        name: "",
        name_pos: pos_of(n),
        visibility: Visibility::Private,
        signature_forced: false,
        entry_point: false,
        in_test: is_test_file(cx.path),
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        params,
        receiver: None,
        return_type: None,
        missing_annotations: Vec::new(),
        doc: Vec::new(),
        doc_pos: None,
        sole_call: None,
    }
}

impl LangHooks for LogosHooks {
    fn shadow_filter(&self) -> ShadowKind {
        go::HOOKS.shadow_filter()
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        go::HOOKS.function(n, cx)
    }

    fn closure_as_function<'s>(
        &self,
        n: Node<'_>,
        parent: Option<Node<'_>>,
        cx: &Cx<'_, 's>,
    ) -> Option<FunctionHead<'s>> {
        if n.kind() == "lambda_expression" {
            return Some(lambda_head(cx, n));
        }
        go::HOOKS.closure_as_function(n, parent, cx)
    }

    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        go::HOOKS.call(n, cx)
    }

    fn is_else_if(&self, n: Node<'_>, parent: Option<Node<'_>>) -> bool {
        go::HOOKS.is_else_if(n, parent)
    }

    fn decl_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        go::HOOKS.decl_names(n, cx, out);
    }

    fn decl_visible_in_initializer(&self) -> bool {
        go::HOOKS.decl_visible_in_initializer()
    }

    fn decl_piece(&self, kind: &str) -> bool {
        go::HOOKS.decl_piece(kind)
    }

    fn decl_scope_parent(&self, depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
        go::HOOKS.decl_scope_parent(depth, node, cx)
    }

    fn block_scoped_binds(&self) -> bool {
        go::HOOKS.block_scoped_binds()
    }

    fn shadow_decl_kind(&self, kind: &str) -> bool {
        go::HOOKS.shadow_decl_kind(kind)
    }

    fn shadow_names<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        go::HOOKS.shadow_names(n, cx, out);
    }

    fn write_roots<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
        go::HOOKS.write_roots(n, cx, out);
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        go::HOOKS.imports(n, cx, out);
    }

    fn typedef<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<TypeHead<'s>> {
        go::HOOKS.typedef(n, cx)
    }

    fn container(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Container> {
        go::HOOKS.container(n, cx)
    }

    fn match_arm(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<sloplint_core::facts::WildcardArm> {
        go::HOOKS.match_arm(n, cx)
    }

    fn handler<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<Handler<'s>> {
        go::HOOKS.handler(n, cx)
    }

    fn struct_lit<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FieldCopy<'s>> {
        go::HOOKS.struct_lit(n, cx)
    }

    fn ambient<'s>(
        &self,
        n: Node<'_>,
        cx: &Cx<'_, 's>,
        in_function: bool,
        out: &mut Vec<(AmbientKind, &'s str, Pos)>,
    ) {
        go::HOOKS.ambient(n, cx, in_function, out);
    }

    fn dynamic(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
        go::HOOKS.dynamic(n, cx)
    }

    fn assertion(&self, n: Node<'_>, cx: &Cx<'_, '_>) -> Option<Assertion> {
        go::HOOKS.assertion(n, cx)
    }
}

#[cfg(test)]
mod tests {
    use crate::spec::{Role, spec};

    #[test]
    fn logos_only_kinds_keep_their_roles() {
        let spec = spec(sloplint_core::lang::Lang::Logos);
        let source = "package p\n\nfunc f() {\n\tconst if true {\n\t\tyield 1\n\t}\n\tg := (x) => x\n\t_ = g\n}\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&spec.language)
            .expect("logos language loads");
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
                        ("const_if_statement", Role::If),
                        ("yield_statement", Role::Yield),
                        ("lambda_expression", Role::Closure),
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
