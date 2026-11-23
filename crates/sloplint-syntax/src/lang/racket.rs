//! Racket adapter: s-expression lists classified by head symbol.
//!
//! The `tree-sitter-racket` grammar implements the default readtable
//! only, so almost every form is a generic `list` node: definitions,
//! branches, loops and calls share one node kind. One kind carries
//! one walker role, so `list` maps to `Role::Function` and every
//! other list-shaped construct is classified by the head table below
//! but realized only where its role is available. See
//! the documented gaps (branches, loops, calls, types). Relative-string
//! `require` specs are recorded as import facts; bare module symbols and
//! installed-collection `(lib ...)` specs are not resolvable and stay out.
//!
//! The grammar defines fields on no node kind, so every lookup below
//! is positional over `named_children`, never `child_by_field`.
//! Comment nodes are named, so head and parameter scans skip the
//! comment kinds explicitly. Definition-shaped lists inside quoted or
//! commented-out datum are not code and report no function.

mod head;

use std::borrow::Cow;

use sloplint_core::facts::{Import, ImportKind, ImportScope, Param, PrimitiveKind, Visibility};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, FunctionHead, LangHooks, last_segment, named_children, pos_of,
    preceding_comments, strip_quotes, text,
};
use crate::spec::Role;

use head::{Head, lookup_head};

pub(crate) static TABLE: crate::lang::Table = &[
    ("list", Role::Function),
    ("comment", Role::Comment),
    ("block_comment", Role::Comment),
    ("sexp_comment", Role::Comment),
];

pub(crate) struct RacketHooks;

pub(crate) static HOOKS: RacketHooks = RacketHooks;

const COMMENT_KINDS: &[&str] = &["comment", "block_comment", "sexp_comment"];

fn is_comment(kind: &str) -> bool {
    COMMENT_KINDS.contains(&kind)
}

/// Named children with comments removed. Comments are named nodes in
/// this grammar, so head and parameter scans must skip them.
fn code_children(n: Node<'_>) -> Vec<Node<'_>> {
    named_children(n)
        .into_iter()
        .filter(|child| !is_comment(child.kind()))
        .collect()
}

/// First code child: the head position of a `list`. Iterates without
/// collecting so ancestor scans stay linear in each list's head.
fn head_child(n: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = n.walk();
    n.named_children(&mut cursor)
        .find(|child| !is_comment(child.kind()))
}

fn first_symbol<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    code_children(n)
        .into_iter()
        .find(|child| child.kind() == "symbol")
        .map(|symbol| text(cx, symbol))
}

/// Which family a pending escape belongs to: `unquote` escapes only
/// `quasiquote`; `unsyntax` escapes only `quasisyntax`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Escape {
    Quote,
    Syntax,
}

/// Whether this node sits inside datum, walking innermost to outermost.
///
/// A quasi form scans its whole template for escapes of its own family —
/// literal forms of the other family and `quote`/`syntax`/`box`/`graph`
/// wrappers are transparent to that scan — so a `quasiquote` consumes
/// the nearest enclosing `unquote` even through an `unsyntax` list or a
/// `quote`. The node's innermost escape decides its fate: when that
/// escape pairs with a quasi the node's fate becomes the quasi's own
/// context; when an outer escape pairs first the innermost one is trapped
/// inside the escape's body and can never pair, so literals enclosing it
/// decide. A `hash` literal never evaluates its contents, whatever
/// encloses it.
fn in_non_code(cx: &Cx<'_, '_>) -> bool {
    // Pending escapes, innermost at index 0. A quasi form pairs with the
    // topmost pending escape of its own family; a literal form is
    // transparent while it sits inside a pending escape's body — a quasi
    // still scans through it — and only decides datum at the end of the
    // walk or when the pending escapes become unpairable.
    let mut escapes: Vec<Escape> = Vec::new();
    // Smallest escape-count seen at a literal ancestor. A literal makes the
    // innermost escape datum only when it sits inside the pairing escape's
    // body — i.e. when it enclosed at most `pos` escapes where `pos` is the
    // pairing escape's index. A literal that also wraps the pairing escape
    // lives in the quasi's template and cannot hide the escape's body.
    let mut literal_min_enclosed = usize::MAX;
    for ancestor in cx.ancestors() {
        let head = match ancestor.kind() {
            "sexp_comment" | "hash" => return true,
            // A vector literal is data like a quotation: its contents are
            // datum unless an unquote escape pairs them with an enclosing
            // quasiquote, in which case the vector stays transparent.
            "vector" | "quote" | "syntax" | "box" | "graph" => {
                if escapes.is_empty() {
                    return true;
                }
                literal_min_enclosed = literal_min_enclosed.min(escapes.len());
                continue;
            }
            "quasiquote" => match escapes.iter().rposition(|e| *e == Escape::Quote) {
                Some(0) => {
                    escapes.clear();
                    literal_min_enclosed = usize::MAX;
                    continue;
                }
                Some(pos) => return literal_min_enclosed <= pos,
                None => return true,
            },
            "quasisyntax" => match escapes.iter().rposition(|e| *e == Escape::Syntax) {
                Some(0) => {
                    escapes.clear();
                    literal_min_enclosed = usize::MAX;
                    continue;
                }
                Some(pos) => return literal_min_enclosed <= pos,
                None => return true,
            },
            "unquote" | "unquote_splicing" => {
                escapes.push(Escape::Quote);
                continue;
            }
            "unsyntax" | "unsyntax_splicing" => {
                escapes.push(Escape::Syntax);
                continue;
            }
            "list" => match head_child(ancestor) {
                Some(head) if head.kind() == "symbol" => text(cx, head),
                _ => continue,
            },
            _ => continue,
        };
        match head {
            "quote" | "syntax" => {
                if escapes.is_empty() {
                    return true;
                }
                literal_min_enclosed = literal_min_enclosed.min(escapes.len());
            }
            "quasiquote" => match escapes.iter().rposition(|e| *e == Escape::Quote) {
                Some(0) => {
                    escapes.clear();
                    literal_min_enclosed = usize::MAX;
                }
                Some(pos) => return literal_min_enclosed <= pos,
                None => return true,
            },
            "quasisyntax" => match escapes.iter().rposition(|e| *e == Escape::Syntax) {
                Some(0) => {
                    escapes.clear();
                    literal_min_enclosed = usize::MAX;
                }
                Some(pos) => return literal_min_enclosed <= pos,
                None => return true,
            },
            "unquote" | "unquote-splicing" => escapes.push(Escape::Quote),
            "unsyntax" | "unsyntax-splicing" => escapes.push(Escape::Syntax),
            _ => {}
        }
    }
    // The innermost escape decides the node's fate. If it never paired,
    // the node is datum only when a literal wraps the escape — otherwise
    // `unquote`/`unsyntax` are ordinary callees whose argument is code.
    !escapes.is_empty() && literal_min_enclosed != usize::MAX
}

/// A `list` whose head splices its body definitions into the
/// enclosing scope: `begin` and the `splicing-*` family. `module`
/// forms are deliberately absent — a submodule's definitions are
/// not in the file's provide scope, and only program-level
/// `(provide ...)` forms are collected.
fn module_level_list(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    n.kind() == "list"
        && head_child(n).is_some_and(|head| {
            head.kind() == "symbol"
                && matches!(
                    text(cx, head),
                    "begin"
                        | "splicing-let"
                        | "splicing-let*"
                        | "splicing-letrec"
                        | "splicing-let-values"
                        | "splicing-let*-values"
                        | "splicing-letrec-values"
                        | "splicing-local"
                        | "splicing-parameterize"
                        | "splicing-syntax-parameterize"
                )
        })
}

/// Whether this `define` sits at file level: every ancestor below the
/// program splices its definitions into the enclosing scope. A define
/// inside any other form — including a submodule — is a local binding
/// and cannot be provided at file level, so it is never Exported.
fn is_module_level(cx: &Cx<'_, '_>) -> bool {
    cx.ancestors()
        .take_while(|ancestor| ancestor.kind() != "program")
        .all(|ancestor| module_level_list(cx, ancestor))
}

/// The signature of a `define`'s second child: `(name args ...)` for a
/// plain signature, `(name args ...)` nested deeper for each curried
/// layer of `(define ((f x) y) ...)`. Returns the innermost list, whose
/// head is the bound name and whose tail is `f`'s parameter list.
fn signature_list(n: Node<'_>) -> Option<Node<'_>> {
    let mut cur = n;
    loop {
        let inner = code_children(cur);
        match inner.first() {
            Some(first) if first.kind() == "symbol" => return Some(cur),
            Some(first) if first.kind() == "list" => cur = *first,
            _ => return None,
        }
    }
}

/// The lambda form a `define`/`define/contract` initializes to, if
/// any: only the last code child counts. In `define/contract` the
/// contract sits before the init and may itself carry lambdas
/// (`#:freevar` predicates), so scanning every list child would bind
/// a contract lambda as the value.
fn lambda_init<'a>(cx: &Cx<'_, '_>, kids: &'a [Node<'a>]) -> Option<Node<'a>> {
    let last = kids.last()?;
    (last.kind() == "list"
        && head_child(*last).is_some_and(|head| {
            head.kind() == "symbol" && matches!(lookup_head(text(cx, head)), Some(Head::Lambda))
        }))
    .then_some(*last)
}

/// The defined name of a top-level `define` list, if it binds one
/// function: `(define (name args ...) ...)` or
/// `(define name (lambda ...))`.
fn define_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let kids = code_children(n);
    let second = *kids.get(1)?;
    if second.kind() == "list" {
        let sig = signature_list(second)?;
        let name = code_children(sig).into_iter().next()?;
        return (name.kind() == "symbol").then(|| text(cx, name));
    }
    if second.kind() != "symbol" {
        return None;
    }
    lambda_init(cx, &kids).map(|_| text(cx, second))
}

fn is_bool_default(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    matches!(text(cx, n), "#t" | "#f" | "#true" | "#false" | "#T" | "#F")
}

/// Parameters of a `(args ...)` list: symbols, `[name default]`
/// shapes, `#:keyword`-marked names, and names past the dot.
fn params_of<'s>(cx: &Cx<'_, 's>, params: Node<'_>) -> Vec<Param<'s>> {
    params_of_nodes(cx, &code_children(params))
}

fn params_of_nodes<'s>(cx: &Cx<'_, 's>, nodes: &[Node<'_>]) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    let mut keyword_only = false;
    let mut variadic_next = false;
    for child in nodes {
        let child = *child;
        match child.kind() {
            "symbol" => {
                out.push(Param {
                    name: text(cx, child),
                    ty: None,
                    primitive_kind: PrimitiveKind::Other,
                    has_default: false,
                    bool_default: false,
                    keyword_only,
                    is_variadic: variadic_next,
                });
                keyword_only = false;
                variadic_next = false;
            }
            "keyword" => {
                keyword_only = true;
                variadic_next = false;
            }
            "dot" => {
                variadic_next = true;
            }
            "list" => {
                let mut inner = code_children(child).into_iter();
                let Some(name) = inner.next() else {
                    keyword_only = false;
                    variadic_next = false;
                    continue;
                };
                if name.kind() != "symbol" {
                    keyword_only = false;
                    variadic_next = false;
                    continue;
                }
                let default = inner.next();
                out.push(Param {
                    name: text(cx, name),
                    ty: None,
                    primitive_kind: PrimitiveKind::Other,
                    has_default: default.is_some(),
                    bool_default: default.is_some_and(|d| is_bool_default(cx, d)),
                    keyword_only,
                    is_variadic: variadic_next,
                });
                keyword_only = false;
                variadic_next = false;
            }
            _ => {
                keyword_only = false;
                variadic_next = false;
            }
        }
    }
    out
}

fn blank_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> FunctionHead<'s> {
    let (doc, doc_pos) = preceding_comments(cx, n, &["comment", "block_comment"], &[]);
    FunctionHead {
        name: "",
        name_pos: pos_of(n),
        visibility: Visibility::Private,
        signature_forced: false,
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        params: Vec::new(),
        shadow_params: Vec::new(),
        receiver: None,
        return_type: None,
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: None,
    }
}

fn define_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>, name: &'s str) -> FunctionHead<'s> {
    let mut head = blank_head(cx, n);
    head.name = name;
    head.name_pos = pos_of(n);
    head.visibility = visibility_of(cx, name);
    let kids = code_children(n);
    match kids.get(1).map(|node| node.kind()) {
        // `(define (name args ...) ...)`: parameters follow the name in
        // the innermost signature list (curried defines nest deeper).
        Some("list") => {
            if let Some(sig) = signature_list(kids[1]) {
                let inner = code_children(sig);
                head.params = params_of_nodes(cx, inner.get(1..).unwrap_or(&[]));
            }
        }
        // `(define name (lambda (args ...) ...))`: parameters live in
        // the lambda form, which carries no name of its own.
        Some("symbol") => {
            if let Some(lambda) = lambda_init(cx, &kids)
                && let Some(params_node) = code_children(lambda).into_iter().nth(1)
            {
                head.params = params_of(cx, params_node);
            }
        }
        _ => {}
    }
    head
}

fn visibility_of(cx: &Cx<'_, '_>, name: &str) -> Visibility {
    if name.is_empty() || !cx.is_exported(name) {
        Visibility::Private
    } else {
        Visibility::Exported
    }
}

fn push_require_spec<'s>(cx: &Cx<'_, 's>, spec: Node<'_>, out: &mut Vec<Import<'s>>) {
    push_spec_in(cx, None, spec, out);
}

fn push_spec_in<'s>(
    cx: &Cx<'_, 's>,
    anchor: Option<&str>,
    spec: Node<'_>,
    out: &mut Vec<Import<'s>>,
) {
    match spec.kind() {
        "string" => {
            let raw = strip_quotes(text(cx, spec));
            if raw.is_empty() {
                return;
            }
            match anchor {
                Some(dir) => push_rebased_require(cx, spec, dir, raw, out),
                None => push_string_require(cx, spec, raw, out),
            }
        }
        "list" => {
            let mut kids = code_children(spec).into_iter();
            let Some(head) = kids.next() else {
                return;
            };
            if head.kind() != "symbol" {
                return;
            }
            let head = text(cx, head);
            if matches!(head, "lib" | "planet") {
                return;
            }
            if head == "relative-in" {
                push_relative_in(cx, kids.collect(), out);
                return;
            }
            for child in kids {
                push_spec_in(cx, anchor, child, out);
            }
        }
        _ => {}
    }
}

/// `(relative-in base spec ...)` resolves each wrapped spec against the
/// base's directory instead of the importing file. The base is only the
/// resolution anchor, never an import. A base that is not a relative
/// string cannot be rebased, so the wrapped specs stay out instead of
/// resolving against the importing file.
fn push_relative_in<'s>(cx: &Cx<'_, 's>, kids: Vec<Node<'_>>, out: &mut Vec<Import<'s>>) {
    let mut kids = kids.into_iter();
    let Some(base) = kids.next() else {
        return;
    };
    if base.kind() != "string" {
        return;
    }
    let anchor = strip_quotes(text(cx, base));
    if !anchor.starts_with('.') {
        return;
    }
    let dir = match anchor.rfind('/') {
        Some(i) => &anchor[..i],
        None => "",
    };
    for child in kids {
        push_spec_in(cx, Some(dir), child, out);
    }
}

/// Joins a `relative-in` anchor directory with a wrapped spec lexically:
/// `..` pops the anchor, `.` and empty parts stay put.
fn rebase_relative(dir: &str, spec: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    for seg in spec.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(seg),
        }
    }
    parts.join("/")
}

fn push_rebased_require<'s>(
    cx: &Cx<'_, 's>,
    spec: Node<'_>,
    dir: &str,
    raw: &'s str,
    out: &mut Vec<Import<'s>>,
) {
    let rebased = rebase_relative(dir, raw);
    if rebased.is_empty() {
        return;
    }
    out.push(Import {
        pos: pos_of(spec),
        module: Cow::Owned(rebased),
        name: last_segment(raw, &["/"]),
        alias: None,
        kind: ImportKind::Namespace,
        scope: ImportScope::Module,
        in_test: is_test_path(cx.path),
        is_reexport: false,
        external_root: None,
    });
}

fn push_string_require<'s>(
    cx: &Cx<'_, 's>,
    spec: Node<'_>,
    module: &'s str,
    out: &mut Vec<Import<'s>>,
) {
    if module.is_empty() {
        return;
    }
    out.push(Import {
        pos: pos_of(spec),
        module: Cow::Borrowed(module),
        name: last_segment(module, &["/"]),
        alias: None,
        kind: ImportKind::Namespace,
        scope: ImportScope::Module,
        in_test: is_test_path(cx.path),
        is_reexport: false,
        external_root: None,
    });
}

fn is_test_path(path: &str) -> bool {
    let mut segments = path.split('/');
    let file = segments.next_back().unwrap_or(path);
    if file == "test.rkt" || file == "tests.rkt" {
        return true;
    }
    segments.any(|segment| segment == "test" || segment == "tests")
}

impl LangHooks for RacketHooks {
    fn call<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<CallHead<'s>> {
        let _ = (n, cx);
        // Unreachable: `list` serves `Role::Function`, so the walker
        // never consults `call`. Racket calls stay unmapped.
        None
    }

    fn imports<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
        if n.kind() != "list" || in_non_code(cx) {
            return;
        }
        let mut kids = code_children(n).into_iter();
        let Some(head) = kids.next() else {
            return;
        };
        if head.kind() != "symbol" || text(cx, head) != "require" {
            return;
        }
        for spec in kids {
            push_require_spec(cx, spec, out);
        }
    }

    fn function<'s>(&self, n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
        if n.kind() != "list" || in_non_code(cx) {
            return None;
        }
        let head = head_child(n)?;
        if head.kind() != "symbol" {
            return None;
        }
        match lookup_head(text(cx, head)) {
            Some(Head::Define) => {
                let name = define_name(cx, n)?;
                let mut head = define_head(cx, n, name);
                if !is_module_level(cx) {
                    head.visibility = Visibility::Private;
                }
                Some(head)
            }
            Some(Head::Lambda) => {
                let mut kids = code_children(n).into_iter();
                let params_node = kids.nth(1)?;
                let mut head = blank_head(cx, n);
                if params_node.kind() == "list" {
                    head.params = params_of(cx, params_node);
                    Some(head)
                } else if params_node.kind() == "symbol" {
                    head.params = vec![Param {
                        name: text(cx, params_node),
                        ty: None,
                        primitive_kind: PrimitiveKind::Other,
                        has_default: false,
                        bool_default: false,
                        keyword_only: false,
                        is_variadic: true,
                    }];
                    Some(head)
                } else {
                    None
                }
            }
            Some(Head::Let) => {
                // Named `let`: `(let loop ([i 0]) ...)` is a recursive
                // local function bound to the loop label.
                let mut kids = code_children(n).into_iter();
                let second = kids.nth(1)?;
                if second.kind() != "symbol" {
                    return None;
                }
                let third = kids.next()?;
                if third.kind() != "list" {
                    return None;
                }
                let mut head = blank_head(cx, n);
                head.name = text(cx, second);
                head.name_pos = pos_of(n);
                Some(head)
            }
            _ => None,
        }
    }

    fn exported_names<'s>(&self, root: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
        if root.kind() != "program" {
            return;
        }
        for child in named_children(root) {
            if child.kind() != "list" {
                continue;
            }
            let mut kids = code_children(child).into_iter();
            let Some(head) = kids.next() else {
                continue;
            };
            if head.kind() != "symbol" || text(cx, head) != "provide" {
                continue;
            }
            for spec in kids {
                match spec.kind() {
                    "symbol" => out.push(text(cx, spec)),
                    "list" => provide_list_names(cx, root, spec, out),
                    _ => {}
                }
            }
        }
    }
}

/// Names a `(provide ...)` sub-list exports: `(all-defined-out)`
/// expands to every top-level defined function, `(contract-out
/// [name ...] ...)` to each clause name, `(rename-out [in out] ...)`
/// to each external name. Anything else is skipped.
fn provide_list_names<'s>(cx: &Cx<'_, 's>, root: Node<'_>, spec: Node<'_>, out: &mut Vec<&'s str>) {
    let mut kids = code_children(spec).into_iter();
    let Some(head) = kids.next() else {
        return;
    };
    if head.kind() != "symbol" {
        return;
    }
    match text(cx, head) {
        "all-defined-out" => {
            for child in named_children(root) {
                if child.kind() != "list" {
                    continue;
                }
                let mut inner = code_children(child).into_iter();
                let Some(inner_head) = inner.next() else {
                    continue;
                };
                if inner_head.kind() == "symbol"
                    && matches!(lookup_head(text(cx, inner_head)), Some(Head::Define))
                    && let Some(name) = define_name(cx, child)
                {
                    out.push(name);
                }
            }
        }
        "contract-out" => {
            for clause in kids {
                if clause.kind() == "list"
                    && let Some(name) = first_symbol(cx, clause)
                {
                    out.push(name);
                }
            }
        }
        "rename-out" => {
            // Visibility is resolved against the internal binding, so
            // record the local name — `(rename-out [local alias])`
            // makes `local` an exported function.
            for pair in kids {
                if pair.kind() != "list" {
                    continue;
                }
                let mut inner = code_children(pair).into_iter();
                if let Some(local) = inner.next()
                    && local.kind() == "symbol"
                {
                    out.push(text(cx, local));
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::spec::{Role, spec};

    #[test]
    fn racket_kinds_keep_their_roles() {
        let spec = spec(sloplint_core::lang::Lang::Racket);
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&spec.language)
            .expect("racket language loads");
        let tree = parser
            .parse("(define (f x) x)\n; note\n", None)
            .expect("tree");
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
                    for (want_kind, want_role) in
                        [("list", Role::Function), ("comment", Role::Comment)]
                    {
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

    #[test]
    fn head_table_names_the_definition_forms() {
        for (head, _) in [
            ("define", super::Head::Define),
            ("define/contract", super::Head::Define),
            ("lambda", super::Head::Lambda),
            ("λ", super::Head::Lambda),
            ("let", super::Head::Let),
            ("if", super::Head::Branch),
            ("match", super::Head::Match),
            ("for/list", super::Head::Loop),
            ("with-handlers", super::Head::Error),
            ("struct", super::Head::Type),
            ("require", super::Head::Import),
            ("module+", super::Head::Module),
        ] {
            assert!(super::lookup_head(head).is_some(), "{head} must classify");
        }
        assert!(
            super::lookup_head("my-helper").is_none(),
            "ordinary calls stay unclassified"
        );
    }
}
