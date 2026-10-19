//! The single iterative tree walk that turns node roles into file facts.

use std::collections::HashMap;

use sloplint_core::facts::FileFacts;
use sloplint_core::lang::Lang;
use sloplint_core::suppress::CommentText;
use tree_sitter::Node;

use crate::hooks::{Container, Cx, narrow, pos_of};
use crate::spec::{LangSpec, Role};

/// What one full walk produced: facts plus the suppressible comments.
pub(crate) struct Walked<'s> {
    pub facts: FileFacts<'s>,
    pub comments: Vec<CommentText<'s>>,
}

impl LangSpec {
    /// Walks one file's tree once, collecting every fact the roles produce.
    pub(crate) fn walk<'s>(&self, input: &crate::FileInput<'s>, root: Node<'_>) -> Walked<'s> {
        self.walk_impl(input, true, root)
    }

    /// Walks one file's tree collecting only the suppressible comments.
    pub(crate) fn walk_comments<'s>(
        &self,
        input: &crate::FileInput<'s>,
        root: Node<'_>,
    ) -> Walked<'s> {
        self.walk_impl(input, false, root)
    }

    fn walk_impl<'s>(
        &self,
        input: &crate::FileInput<'s>,
        collect_facts: bool,
        root: Node<'_>,
    ) -> Walked<'s> {
        let mut state = WalkState {
            spec: self,
            src: input.source,
            path: input.path,
            lang: input.lang,
            is_test_file: input.is_test_file,
            has_interface: input.has_interface,
            collect_facts,
            facts: empty_facts(input),
            comments: Vec::new(),
            fn_stack: Vec::new(),
            marks: Vec::new(),
            containers: Vec::new(),
            exports: Vec::new(),
        };
        state.run(root);
        Walked {
            facts: state.facts,
            comments: state.comments,
        }
    }
}

/// Empty facts for a file, carrying only its identity fields.
pub(crate) fn empty_facts<'s>(input: &crate::FileInput<'s>) -> FileFacts<'s> {
    FileFacts {
        lang: input.lang,
        path: input.path,
        source: input.source,
        is_test_file: input.is_test_file,
        is_entry_file: input.lang.is_entry_file(input.path),
        line_count: line_count(input.source),
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

/// The number of lines in a source string; an empty string has none.
pub(crate) fn line_count(source: &str) -> u32 {
    narrow(source.lines().count())
}

/// One open function: its fact index, scoring counters, and declared bindings.
pub(crate) struct FnFrame<'s> {
    pub index: usize,
    pub nest: u32,
    pub frames: u32,
    pub decls: HashMap<&'s str, FrameDecl>,
    pub sole_call: Option<usize>,
}

/// The latest binding declared under one name inside a function frame.
pub(crate) struct FrameDecl {
    pub binding: usize,
    pub frames_at_decl: u32,
}

/// Deferred state action for a node about to exit.
#[derive(Clone, Copy)]
pub(crate) enum Mark {
    Nest,
    Frame,
    Fn,
    Container,
}

/// All mutable walk state; the tree cursor lives only in [`WalkState::run`].
pub(crate) struct WalkState<'s, 'a> {
    pub spec: &'a LangSpec,
    pub src: &'s str,
    pub path: &'s str,
    pub lang: Lang,
    pub is_test_file: bool,
    pub has_interface: bool,
    pub collect_facts: bool,
    pub facts: FileFacts<'s>,
    pub comments: Vec<CommentText<'s>>,
    pub fn_stack: Vec<FnFrame<'s>>,
    pub marks: Vec<(usize, Mark)>,
    pub containers: Vec<Container>,
    pub exports: Vec<&'s str>,
}

impl<'s, 'a> WalkState<'s, 'a> {
    pub(crate) fn run(&mut self, root: Node<'_>) {
        if self.collect_facts {
            let hooks = self.spec.hooks;
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
            descending = false;
        }
        if self.collect_facts {
            let hooks = self.spec.hooks;
            let barrel = {
                let cx = self.make_cx();
                hooks.barrel(root, &cx)
            };
            self.facts.barrel = barrel;
        }
    }

    fn enter(&mut self, node: Node<'_>) {
        let role = self.spec.role_for(node.kind_id());
        if !self.collect_facts {
            if role == Role::Comment {
                self.on_comment(node);
            }
            return;
        }
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
            Role::ElseIf => self.score_top(1),
            Role::Loop => self.enter_loop(node),
            Role::Match | Role::Ternary => self.score_nested(node),
            Role::With => {
                if !self.fn_stack.is_empty() {
                    let line = pos_of(node).line;
                    self.bump_nest(node.id(), line);
                }
            }
            Role::Catch => self.enter_catch(node),
            _ => {}
        }
    }

    fn enter_if(&mut self, node: Node<'_>) {
        let hooks = self.spec.hooks;
        if !self.fn_stack.is_empty() {
            if hooks.is_else_if(node) {
                self.score_top(1);
            } else {
                let line = pos_of(node).line;
                self.score_and_nest(1, line, node.id());
            }
        }
        self.push_handler(node);
    }

    fn enter_loop(&mut self, node: Node<'_>) {
        if self.fn_stack.is_empty() {
            return;
        }
        let line = pos_of(node).line;
        self.score_and_nest(1, line, node.id());
        self.bump_frames(node.id());
    }

    fn enter_catch(&mut self, node: Node<'_>) {
        self.score_nested(node);
        self.push_handler(node);
    }

    fn score_nested(&mut self, node: Node<'_>) {
        if self.fn_stack.is_empty() {
            return;
        }
        let line = pos_of(node).line;
        self.score_and_nest(1, line, node.id());
    }

    fn enter_function_like(&mut self, role: Role, node: Node<'_>) {
        let hooks = self.spec.hooks;
        let head = if role == Role::Function {
            let cx = self.make_cx();
            hooks.function(node, &cx)
        } else {
            let cx = self.make_cx();
            hooks.closure_as_function(node, &cx)
        };
        match head {
            Some(head) => self.push_function(node, head),
            None => {
                if role == Role::Closure && !self.fn_stack.is_empty() {
                    let line = pos_of(node).line;
                    self.bump_nest(node.id(), line);
                    self.bump_frames(node.id());
                }
            }
        }
    }

    fn enter_call_site(&mut self, node: Node<'_>) {
        {
            let cx = self.make_cx();
            if self.spec.hooks.control_call(node, &cx) {
                self.score_nested(node);
            }
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
                }
            }
            Role::Bind => {
                if self.fn_stack.is_empty() {
                    self.collect_ambient(node);
                } else {
                    self.apply_bind(node);
                }
            }
            Role::Write => self.apply_write_roots(node),
            _ => {}
        }
    }

    pub(crate) fn exit(&mut self, node: Node<'_>) {
        let id = node.id();
        let mut index = self.marks.len();
        while index > 0 {
            index -= 1;
            if self.marks[index].0 != id {
                continue;
            }
            let mark = self.marks.remove(index).1;
            self.apply_mark(mark);
        }
    }

    fn apply_mark(&mut self, mark: Mark) {
        match mark {
            Mark::Nest => {
                if let Some(frame) = self.fn_stack.last_mut() {
                    frame.nest -= 1;
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
                self.containers.pop();
            }
        }
    }

    pub(crate) fn make_cx(&self) -> Cx<'_, 's> {
        Cx {
            src: self.src,
            path: self.path,
            lang: self.lang,
            is_test_file: self.is_test_file,
            has_interface: self.has_interface,
            containers: &self.containers,
            exports: &self.exports,
        }
    }

    fn score_top(&mut self, delta: u32) {
        let Some(frame) = self.fn_stack.last() else {
            return;
        };
        let index = frame.index;
        let function = &mut self.facts.functions[index];
        function.cognitive = function.cognitive.saturating_add(delta);
    }

    fn score_and_nest(&mut self, delta: u32, line: u32, mark: usize) {
        let nest = self.fn_stack.last().map_or(0, |frame| frame.nest);
        self.score_top(delta.saturating_add(nest));
        self.bump_nest(mark, line);
    }

    fn bump_nest(&mut self, mark: usize, line: u32) {
        let Some(frame) = self.fn_stack.last_mut() else {
            return;
        };
        frame.nest += 1;
        let (index, nest) = (frame.index, frame.nest);
        self.marks.push((mark, Mark::Nest));
        let function = &mut self.facts.functions[index];
        if nest > function.max_depth {
            function.max_depth = nest;
            function.deepest_line = line;
        }
    }

    fn bump_frames(&mut self, mark: usize) {
        let Some(frame) = self.fn_stack.last_mut() else {
            return;
        };
        frame.frames += 1;
        self.marks.push((mark, Mark::Frame));
    }
}
