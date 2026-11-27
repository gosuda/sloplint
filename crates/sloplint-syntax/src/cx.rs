use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use tree_sitter::Node;

use sloplint_core::facts::Pos;
use sloplint_core::lang::Lang;

use crate::hooks::{Container, slice};

/// One declaration's names as `(start, len, pos)` byte ranges into the file source.
type DeclPieces = Vec<(usize, usize, Pos)>;

#[derive(Default)]
pub(crate) struct MemberStates {
    before: RefCell<HashMap<usize, Option<bool>>>,
    macro_locals: RefCell<HashMap<usize, bool>>,
    decl_pieces: RefCell<HashMap<usize, DeclPieces>>,
}

impl MemberStates {
    pub(crate) fn macro_body_is_local(
        &self,
        body: Node<'_>,
        compute: impl FnOnce() -> bool,
    ) -> bool {
        if let Some(known) = self.macro_locals.borrow().get(&body.id()).copied() {
            return known;
        }
        let local = compute();
        self.macro_locals.borrow_mut().insert(body.id(), local);
        local
    }

    /// Names of one declaration as `(start, len, pos)` byte ranges into the
    /// file source, computed once per declaration. Ranges re-slice the source
    /// instead of holding `&str` so this per-file state needs no lifetime.
    pub(crate) fn decl_pieces<R>(
        &self,
        decl: Node<'_>,
        compute: impl FnOnce() -> DeclPieces,
        with: impl FnOnce(&DeclPieces) -> R,
    ) -> R {
        if self.decl_pieces.borrow().contains_key(&decl.id()) {
            let known = self.decl_pieces.borrow();
            return with(&known[&decl.id()]);
        }
        let computed = compute();
        let result = with(&computed);
        self.decl_pieces.borrow_mut().insert(decl.id(), computed);
        result
    }

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
    literals: LiteralCache,
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

#[derive(Default)]
pub(crate) struct LiteralCache {
    cached: RefCell<HashMap<usize, Vec<String>>>,
}

impl LiteralCache {
    pub(crate) fn reuse<R>(
        &self,
        func: usize,
        compute: impl FnOnce() -> Vec<String>,
        with: impl FnOnce(&[String]) -> R,
    ) -> R {
        if let Some(names) = self.cached.borrow().get(&func) {
            return with(names);
        }
        let names = compute();
        let out = with(&names);
        self.cached.borrow_mut().insert(func, names);
        out
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileSetKind {
    Scope,
    Type,
    Member,
    Module,
    SleepModules,
    SleepFunctions,
    VirtualTimeModules,
    SleepDurationTypes,
}

impl FileSetKind {
    fn index(self) -> usize {
        match self {
            FileSetKind::Scope => 0,
            FileSetKind::Type => 1,
            FileSetKind::Member => 2,
            FileSetKind::Module => 3,
            FileSetKind::SleepModules => 4,
            FileSetKind::SleepFunctions => 5,
            FileSetKind::VirtualTimeModules => 6,
            FileSetKind::SleepDurationTypes => 7,
        }
    }
}

#[derive(Default)]
pub(crate) struct FileNameSets<'s> {
    slots: [OnceCell<Vec<&'s str>>; 8],
}

impl<'s> FileNameSets<'s> {
    pub(crate) fn for_kind(
        &self,
        kind: FileSetKind,
        compute: impl FnOnce() -> Vec<&'s str>,
    ) -> &[&'s str] {
        self.slots[kind.index()].get_or_init(compute)
    }
}

pub(crate) struct Cx<'t, 's> {
    pub src: &'s str,
    pub path: &'s str,
    pub lang: Lang,
    pub members: &'t MemberStates,
    chains: &'t ChainRoots,
    name_sets: &'t FileNameSets<'s>,
    containers: &'t [Container],
    exports: &'t [&'s str],
    pub(crate) forked_depth: u32,
    /// Per level, outermost first: earlier siblings in tree order, then the open ancestor; the innermost level ends at the visited node.
    pub(crate) levels: &'t [Vec<Node<'t>>],
}

impl<'t, 's> Cx<'t, 's> {
    pub(crate) fn file(
        src: &'s str,
        path: &'s str,
        lang: Lang,
        members: &'t MemberStates,
        chains: &'t ChainRoots,
        name_sets: &'t FileNameSets<'s>,
    ) -> Self {
        Self {
            src,
            path,
            lang,
            members,
            chains,
            name_sets,
            containers: &[],
            exports: &[],
            forked_depth: 0,
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

impl<'t, 's> Cx<'t, 's> {
    pub(crate) fn parent(&self) -> Option<Node<'t>> {
        let len = self.levels.len();
        if len < 2 {
            return None;
        }
        self.levels[len - 2].last().copied()
    }
    pub(crate) fn parent_of(&self, n: Node<'_>) -> Option<Node<'t>> {
        let id = n.id();
        if self
            .levels
            .last()
            .and_then(|level| level.last())
            .is_some_and(|last| last.id() == id)
        {
            return self.parent();
        }
        for pair in self.levels.windows(2) {
            if pair[1].last().is_some_and(|last| last.id() == id) {
                return pair[0].last().copied();
            }
        }
        None
    }
    pub(crate) fn ancestors(&self) -> impl Iterator<Item = Node<'t>> + '_ {
        self.levels[..self.levels.len().saturating_sub(1)]
            .iter()
            .rev()
            .filter_map(|level| level.last().copied())
    }
    pub(crate) fn root(&self) -> Option<Node<'t>> {
        self.levels.first()?.last().copied()
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
    pub(crate) fn file_names(
        &self,
        kind: FileSetKind,
        compute: impl FnOnce() -> Vec<&'s str>,
    ) -> &[&'s str] {
        self.name_sets.for_kind(kind, compute)
    }
    pub(crate) fn literal_targets<R>(
        &self,
        func: usize,
        compute: impl FnOnce() -> Vec<String>,
        with: impl FnOnce(&[String]) -> R,
    ) -> R {
        self.chains.literals.reuse(func, compute, with)
    }
}

impl Cx<'_, '_> {
    pub(crate) fn in_forked_scope(&self) -> bool {
        self.forked_depth > 0
    }

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
}
#[cfg(test)]
mod tests {
    use super::*;

    fn parse_cpp_declaration(src: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .expect("cpp grammar loads");
        parser.parse(src, None).expect("snippet parses")
    }

    #[test]
    fn decl_pieces_compute_once_per_declaration() {
        let tree = parse_cpp_declaration("int a = 1, b = 2;");
        let decl = tree.root_node().named_child(0).expect("one declaration");
        assert_eq!(decl.kind(), "declaration");
        let states = MemberStates::default();
        let calls = std::cell::Cell::new(0u32);
        let compute = || {
            calls.set(calls.get() + 1);
            vec![(4, 1, Pos { line: 1, byte: 4 })]
        };
        for _ in 0..3 {
            let seen = states.decl_pieces(decl, compute, |pieces| pieces.to_vec());
            assert_eq!(seen, vec![(4, 1, Pos { line: 1, byte: 4 })]);
        }
        assert_eq!(calls.get(), 1, "three lookups share one scan");
    }

    #[test]
    fn decl_pieces_key_on_declaration() {
        let tree = parse_cpp_declaration("int a = 1; int b = 2;");
        let root = tree.root_node();
        let first = root.named_child(0).expect("first declaration");
        let second = root.named_child(1).expect("second declaration");
        let states = MemberStates::default();
        let calls = std::cell::Cell::new(0u32);
        for decl in [first, second] {
            states.decl_pieces(
                decl,
                || {
                    calls.set(calls.get() + 1);
                    vec![]
                },
                |_| {},
            );
        }
        assert_eq!(calls.get(), 2, "each declaration scans once");
    }
}
