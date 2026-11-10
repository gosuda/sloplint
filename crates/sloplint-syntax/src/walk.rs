use std::collections::HashMap;

use sloplint_core::facts::FileFacts;
use sloplint_core::finding::line_starts;
use sloplint_core::lang::Lang;
use sloplint_core::role::{CodeRole, RoleRegion};
use sloplint_core::suppress::CommentText;
use tree_sitter::Node;

use crate::hooks::{ChainRoots, Container, Cx, LangHooks, MemberStates, narrow, pos_of};
use crate::spec::{LangSpec, Role};

pub(crate) struct Walked<'s> {
    pub facts: FileFacts<'s>,
    pub comments: Vec<CommentText<'s>>,
}

impl LangSpec {
    pub(crate) fn walk<'s>(
        &self,
        hooks: &'static dyn LangHooks,
        input: &crate::FileInput<'s>,
        root: Node<'_>,
    ) -> Walked<'s> {
        self.walk_impl(hooks, input, true, root)
    }

    pub(crate) fn walk_comments<'s>(
        &self,
        hooks: &'static dyn LangHooks,
        input: &crate::FileInput<'s>,
        root: Node<'_>,
    ) -> Walked<'s> {
        self.walk_impl(hooks, input, false, root)
    }

    fn walk_impl<'s>(
        &self,
        hooks: &'static dyn LangHooks,
        input: &crate::FileInput<'s>,
        collect_facts: bool,
        root: Node<'_>,
    ) -> Walked<'s> {
        let mut state = WalkState {
            spec: self,
            hooks,
            src: input.source,
            path: input.path,
            lang: self.lang,
            collect_facts,
            facts: empty_facts(input, self.lang),
            comments: Vec::new(),
            fn_stack: Vec::new(),
            marks: Vec::new(),
            containers: Vec::new(),
            method_depth: 0,
            exports: Vec::new(),
            members: MemberStates::default(),
            chains: ChainRoots::default(),
            levels: Vec::new(),
            depth: 0,
            #[cfg(debug_assertions)]
            entered: Vec::new(),
        };
        state.run(root);
        state.finish();
        Walked {
            facts: state.facts,
            comments: state.comments,
        }
    }
}

/// The `lang` field is the grammar that produced the facts.
pub(crate) fn empty_facts<'s>(input: &crate::FileInput<'s>, lang: Lang) -> FileFacts<'s> {
    FileFacts {
        lang,
        path: input.path,
        source: input.source,
        line_starts: line_starts(input.source),
        role: input.role,
        regions: Vec::new(),
        line_count: line_count(input.source),
        package: String::new(),
        is_generated: is_generated(input.source),
        functions: Vec::new(),
        calls: Vec::new(),
        bindings: Vec::new(),
        imports: Vec::new(),
        types: Vec::new(),
        impls: Vec::new(),
        wildcard_arms: Vec::new(),
        handlers: Vec::new(),
        assertions: Vec::new(),
        field_copies: Vec::new(),
        dynamic_symbols: Vec::new(),
        ambient: Vec::new(),
        barrel: None,
        has_inline_tests: false,
    }
}

pub(crate) fn is_generated(source: &str) -> bool {
    let mut end = source.len().min(2048);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let head = source[..end].to_lowercase();
    head.contains("@generated")
        || head.contains("do not edit")
        || head.contains("automatically generated")
}

pub(crate) fn line_count(source: &str) -> u32 {
    narrow(source.lines().count())
}

pub(crate) struct FnFrame<'s> {
    pub index: usize,
    pub nest: u32,
    pub depth: u32,
    pub frames: u32,
    pub decls: HashMap<&'s str, FrameDecl>,
    pub sole_call: Option<usize>,
    pub scored: Vec<bool>,
}

pub(crate) struct FrameDecl {
    pub binding: usize,
    pub frames_at_decl: u32,
}

#[derive(Clone, Copy)]
pub(crate) enum Mark {
    Nest,
    Depth,
    Frame,
    Fn,
    Container,
}

pub(crate) struct WalkState<'s, 'a, 't> {
    pub spec: &'a LangSpec,
    pub hooks: &'static dyn LangHooks,
    pub src: &'s str,
    pub path: &'s str,
    pub lang: Lang,
    pub collect_facts: bool,
    pub facts: FileFacts<'s>,
    pub comments: Vec<CommentText<'s>>,
    pub fn_stack: Vec<FnFrame<'s>>,
    pub marks: Vec<(usize, Mark)>,
    pub containers: Vec<Container>,
    pub method_depth: u32,
    pub exports: Vec<&'s str>,
    pub members: MemberStates,
    pub chains: ChainRoots,
    pub levels: Vec<Vec<Node<'t>>>,
    pub depth: usize,
    #[cfg(debug_assertions)]
    pub entered: Vec<usize>,
}

impl<'s, 'a, 't> WalkState<'s, 'a, 't> {
    pub(crate) fn run(&mut self, root: Node<'t>) {
        if self.collect_facts {
            let hooks = self.hooks;
            let mut names = Vec::new();
            hooks.exported_names(root, &self.make_cx(), &mut names);
            names.sort_unstable();
            names.dedup();
            self.exports = names;
        }
        let mut cursor = root.walk();
        let mut descending = true;
        loop {
            if descending {
                let node = cursor.node();
                self.enter(node);
                if cursor.goto_first_child() {
                    self.depth += 1;
                    continue;
                }
            }
            let node = cursor.node();
            self.exit(node);
            if cursor.goto_next_sibling() {
                descending = true;
                continue;
            }
            if !cursor.goto_parent() {
                break;
            }
            if let Some(level) = self.levels.get_mut(self.depth) {
                level.clear();
            }
            self.depth -= 1;
            descending = false;
        }
        if self.collect_facts {
            let hooks = self.hooks;
            let barrel = {
                let cx = self.make_cx();
                hooks.barrel(root, &cx)
            };
            self.facts.barrel = barrel;
        }
    }

    fn enter(&mut self, node: Node<'t>) {
        self.open_node(node);
        let role = self.spec.role_for(node.kind_id());
        if !self.collect_facts {
            if role == Role::Comment {
                self.on_comment(node);
            }
            return;
        }
        if self.levels.len() <= self.depth {
            self.levels.resize_with(self.depth + 1, Vec::new);
        }
        self.levels[self.depth].push(node);
        match role {
            Role::Function | Role::Closure => self.enter_function_like(role, node),
            Role::If
            | Role::ElseIf
            | Role::Match
            | Role::Loop
            | Role::Catch
            | Role::Ternary
            | Role::With => {
                self.enter_flow(role, node);
            }
            Role::Call | Role::MacroCall | Role::NewCall => self.enter_call_site(node),
            Role::Return => self.enter_return(node),
            _ => self.enter_other(role, node),
        }
    }

    fn enter_other(&mut self, role: Role, node: Node<'_>) {
        match role {
            Role::Decl | Role::Bind | Role::Write => self.enter_binding(role, node),
            Role::Import | Role::Export => self.push_imports(node),
            Role::TypeDef => self.enter_typedef(node),
            Role::Impl => self.enter_impl(node),
            Role::Module => self.enter_module(node),
            Role::Comment => self.on_comment(node),
            _ => self.enter_small(role, node),
        }
    }

    fn enter_small(&mut self, role: Role, node: Node<'_>) {
        match role {
            Role::MatchArm => self.enter_match_arm(node),
            Role::StructLit => self.push_struct_lit(node),
            Role::Assert => self.push_assertion(node),
            Role::Member | Role::Subscript | Role::Global | Role::StaticItem => {
                self.push_dynamic(node);
                self.collect_ambient(node);
            }
            _ => {}
        }
    }

    fn enter_flow(&mut self, role: Role, node: Node<'_>) {
        match role {
            Role::If => self.enter_if(node),
            Role::ElseIf => {}
            Role::Loop => self.enter_loop(node),
            Role::Match | Role::Ternary => self.enter_depth_only(node),
            Role::With => {}
            Role::Catch => self.enter_catch(node),
            _ => {}
        }
    }

    fn visited_parent(&self) -> Option<Node<'t>> {
        let above = self.depth.checked_sub(1)?;
        self.levels.get(above)?.last().copied()
    }

    fn enter_if(&mut self, node: Node<'_>) {
        if !self.fn_stack.is_empty() {
            let hooks = self.hooks;
            if !hooks.is_else_if(node, self.visited_parent()) {
                let line = pos_of(node).line;
                self.score_first();
                self.open_nest(line, node.id());
            }
        }
        self.push_handler(node);
    }

    fn enter_loop(&mut self, node: Node<'_>) {
        if self.fn_stack.is_empty() {
            return;
        }
        let line = pos_of(node).line;
        self.score_first();
        self.open_nest(line, node.id());
        self.bump_frames(node.id());
    }

    fn enter_catch(&mut self, node: Node<'_>) {
        if !self.fn_stack.is_empty() {
            let line = pos_of(node).line;
            self.score_first();
            self.open_nest(line, node.id());
        }
        self.push_handler(node);
    }

    fn enter_depth_only(&mut self, node: Node<'_>) {
        if self.fn_stack.is_empty() {
            return;
        }
        let line = pos_of(node).line;
        self.open_depth(line, node.id());
    }

    fn enter_function_like(&mut self, role: Role, node: Node<'_>) {
        let hooks = self.hooks;
        let head = if role == Role::Function {
            let cx = self.make_cx();
            hooks.function(node, &cx)
        } else {
            let parent = self.visited_parent();
            let cx = self.make_cx();
            hooks.closure_as_function(node, parent, &cx)
        };
        if let Some(head) = head {
            self.push_function(node, head);
            let body = node
                .child_by_field_name("body")
                .or_else(|| Self::first_named_of_kind(node, "function_body"));
            if let Some(body) = body {
                let inner = if body.kind() == "function_body" {
                    body.named_child(0)
                } else {
                    Some(body)
                };
                if let Some(inner) = inner
                    && !matches!(
                        inner.kind(),
                        "statement_block" | "block" | "indented_block" | "compound_statement"
                    )
                {
                    self.set_value_return();
                }
            }
        }
    }

    fn first_named_of_kind<'n>(node: Node<'n>, kind: &str) -> Option<Node<'n>> {
        node.children(&mut node.walk())
            .find(|child| child.kind() == kind)
    }

    fn enter_return(&mut self, node: Node<'_>) {
        if self.fn_stack.is_empty() {
            return;
        }
        if node.named_child_count() > 0 {
            self.set_value_return();
        }
    }

    fn set_value_return(&mut self) {
        let Some(frame) = self.fn_stack.last_mut() else {
            return;
        };
        let index = frame.index;
        self.facts.functions[index].has_value_return = true;
    }

    fn enter_call_site(&mut self, node: Node<'_>) {
        let controls = {
            let cx = self.make_cx();
            self.hooks.control_call(node, &cx)
        };
        if controls {
            self.enter_depth_only(node);
        }
        let marker = {
            let cx = self.make_cx();
            self.hooks.call_marker(node, &cx)
        };
        if let Some(role) = marker {
            self.push_region(crate::hooks::region_of(node, role));
        }
        self.push_call(node);
        self.push_imports(node);
        self.push_assertion(node);
        self.push_dynamic(node);
        self.collect_ambient(node);
        self.apply_write_roots(node);
        self.push_struct_lit(node);
    }

    fn enter_binding(&mut self, role: Role, node: Node<'_>) {
        match role {
            Role::Decl => {
                if self.fn_stack.is_empty() {
                    self.collect_ambient(node);
                } else {
                    self.push_decls(node);
                    self.collect_ambient(node);
                }
            }
            Role::Bind => {
                if self.fn_stack.is_empty() {
                    self.collect_ambient(node);
                } else {
                    self.apply_bind(node);
                }
            }
            Role::Write => {
                self.apply_write_roots(node);
                self.collect_ambient(node);
            }
            _ => {}
        }
    }

    pub(crate) fn exit(&mut self, node: Node<'_>) {
        let id = node.id();
        self.close_node(id);
        while let Some(&(mark_node, mark)) = self.marks.last() {
            if mark_node != id {
                break;
            }
            self.marks.pop();
            self.apply_mark(mark);
        }
    }

    #[cfg(debug_assertions)]
    fn open_node(&mut self, node: Node<'_>) {
        self.entered.push(node.id());
    }

    #[cfg(not(debug_assertions))]
    fn open_node(&mut self, _node: Node<'_>) {}

    #[cfg(debug_assertions)]
    fn close_node(&mut self, id: usize) {
        debug_assert_eq!(
            self.entered.pop(),
            Some(id),
            "nodes exit innermost first, so each node's marks sit on top of the stack"
        );
    }

    #[cfg(not(debug_assertions))]
    fn close_node(&mut self, _id: usize) {}

    #[cfg(debug_assertions)]
    pub(crate) fn assert_mark_entered(&self, mark: usize) {
        debug_assert_eq!(
            self.entered.last(),
            Some(&mark),
            "a mark is keyed to the node being entered, or a later push would bury it"
        );
    }

    #[cfg(not(debug_assertions))]
    pub(crate) fn assert_mark_entered(&self, _mark: usize) {}

    fn apply_mark(&mut self, mark: Mark) {
        match mark {
            Mark::Nest => {
                if let Some(frame) = self.fn_stack.last_mut() {
                    frame.nest -= 1;
                    frame.scored.pop();
                }
            }
            Mark::Depth => {
                if let Some(frame) = self.fn_stack.last_mut() {
                    frame.depth -= 1;
                }
            }
            Mark::Frame => {
                if let Some(frame) = self.fn_stack.last_mut() {
                    frame.frames -= 1;
                }
            }
            Mark::Fn => {
                self.fn_stack.pop();
            }
            Mark::Container => {
                if self.containers.pop().is_some_and(|container| {
                    matches!(
                        container,
                        Container::Impl { .. } | Container::Trait { .. } | Container::Class { .. }
                    )
                }) {
                    self.method_depth -= 1;
                }
            }
        }
    }

    pub(crate) fn finish(&mut self) {
        self.facts.regions.sort_by_key(|region| region.start_line);
        self.facts.has_inline_tests = self
            .facts
            .regions
            .iter()
            .any(|region| region.role == CodeRole::Test);
    }

    pub(crate) fn push_region(&mut self, region: RoleRegion) {
        self.facts.regions.push(region);
    }

    pub(crate) fn make_cx(&self) -> Cx<'_, 's> {
        Cx::file(self.src, self.path, self.lang, &self.members, &self.chains).scoped(
            &self.containers,
            &self.exports,
            &self.levels[..self.levels.len().min(self.depth + 1)],
        )
    }

    fn score_first(&mut self) {
        let (index, nest, first) = match self.fn_stack.last_mut() {
            None => return,
            Some(frame) => {
                let nest = frame.nest;
                let first = nest > 0 && !frame.scored.last().copied().unwrap_or(true);
                if nest > 0
                    && let Some(slot) = frame.scored.last_mut()
                {
                    *slot = true;
                }
                (frame.index, nest, first)
            }
        };
        let delta = if nest == 0 {
            0
        } else if first {
            1 + nest
        } else {
            1
        };
        if delta > 0 {
            let function = &mut self.facts.functions[index];
            function.cognitive = function.cognitive.saturating_add(delta);
        }
    }

    fn open_nest(&mut self, line: u32, mark: usize) {
        self.assert_mark_entered(mark);
        let Some(frame) = self.fn_stack.last_mut() else {
            return;
        };
        frame.nest += 1;
        frame.scored.push(false);
        frame.depth += 1;
        let (index, depth) = (frame.index, frame.depth);
        self.marks.push((mark, Mark::Nest));
        self.marks.push((mark, Mark::Depth));
        self.note_depth(index, depth, line);
    }

    fn open_depth(&mut self, line: u32, mark: usize) {
        self.assert_mark_entered(mark);
        let Some(frame) = self.fn_stack.last_mut() else {
            return;
        };
        frame.depth += 1;
        let (index, depth) = (frame.index, frame.depth);
        self.marks.push((mark, Mark::Depth));
        self.note_depth(index, depth, line);
    }

    fn note_depth(&mut self, index: usize, depth: u32, line: u32) {
        let function = &mut self.facts.functions[index];
        if depth > function.max_depth {
            function.max_depth = depth;
            function.deepest_line = line;
        }
    }

    fn bump_frames(&mut self, mark: usize) {
        self.assert_mark_entered(mark);
        let Some(frame) = self.fn_stack.last_mut() else {
            return;
        };
        frame.frames += 1;
        self.marks.push((mark, Mark::Frame));
    }
}
