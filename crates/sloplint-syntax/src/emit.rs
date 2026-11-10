use std::collections::{HashMap, HashSet};

use sloplint_core::facts::{Ambient, AmbientKind, Call, DynamicSymbol, Function, MutBinding, Pos};
use sloplint_core::role::{CodeRole, RoleRegion};
use sloplint_core::suppress::CommentText;

use crate::hooks::{
    Container, FunctionHead, ShadowKind, is_directive_comment, narrow, pos_of, slice,
};
use crate::shadow::{FnFrame, FrameDecl, Mark, ScopedShadow};
use crate::spec::Role;
use crate::walk::WalkState;

impl<'s, 'a> WalkState<'s, 'a, '_> {
    pub(crate) fn enter_typedef(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let head = {
            let cx = self.make_cx();
            hooks.typedef(node, &cx)
        };
        let Some(head) = head else {
            return;
        };
        if let Some(def) = head.def {
            self.facts.types.push(def);
        }
        self.facts.impls.extend(head.impls);
        if let Some(container) = head.container {
            self.push_container(node.id(), container);
        }
    }

    pub(crate) fn enter_impl(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let head = {
            let cx = self.make_cx();
            hooks.typedef(node, &cx)
        };
        let mut contained = false;
        if let Some(head) = head {
            self.facts.impls.extend(head.impls);
            if let Some(container) = head.container {
                self.push_container(node.id(), container);
                contained = true;
            }
        }
        if !contained {
            let container = {
                let cx = self.make_cx();
                hooks.container(node, &cx)
            };
            if let Some(container) = container {
                self.push_container(node.id(), container);
            }
        }
    }

    pub(crate) fn enter_module(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let (container, region, leading) = {
            let cx = self.make_cx();
            (
                hooks.container(node, &cx),
                hooks.module_region(node, &cx),
                hooks.module_leading(node, &cx),
            )
        };
        if let Some(container) = container {
            self.push_container(node.id(), container);
        }
        if let Some(role) = region {
            self.push_region(attributed_region(node, leading, role));
        }
    }

    pub(crate) fn enter_match_arm(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let arm = {
            let cx = self.make_cx();
            hooks.match_arm(node, &cx)
        };
        if let Some(arm) = arm {
            self.facts.wildcard_arms.push(arm);
        }
        self.push_handler(node);
    }

    pub(crate) fn push_function(&mut self, node: tree_sitter::Node<'_>, head: FunctionHead<'s>) {
        let untyped_throws = {
            let cx = self.make_cx();
            self.hooks.untyped_throws(node, &cx)
        };
        let index = self.facts.functions.len();
        let mut params: HashSet<&'s str> = head
            .params
            .iter()
            .map(|param| param.name)
            .filter(|name| !name.is_empty())
            .collect();
        params.extend(
            head.shadow_params
                .iter()
                .copied()
                .filter(|name| !name.is_empty()),
        );
        let is_method = self.method_depth > 0 || {
            let cx = self.make_cx();
            matches!(
                self.hooks.container(node, &cx),
                Some(Container::Impl { .. } | Container::Trait { .. } | Container::Class { .. })
            )
        };
        let in_test = head.in_test
            || matches!(head.region, Some(region) if matches!(region.role, CodeRole::Test | CodeRole::Bench));
        self.facts.functions.push(Function {
            name: head.name,
            pos: head.name_pos,
            end_line: narrow(node.end_position().row) + 1,
            doc: head.doc,
            doc_pos: head.doc_pos,
            visibility: head.visibility,
            signature_forced: head.signature_forced,
            entry_point: head.entry_point,
            in_test,
            is_overload: head.is_overload,
            is_deprecated: head.is_deprecated,
            is_decorated: head.is_decorated,
            is_trait_item: head.is_trait_item,
            is_method,
            receiver: head.receiver,
            params: head.params,
            return_type: head.return_type,
            untyped_throws,
            missing_annotations: head.missing_annotations,
            cognitive: 0,
            max_depth: 0,
            deepest_line: 0,
            has_value_return: false,
            sole_call: None,
        });
        if let Some(region) = head.region {
            self.push_region(region);
        }
        self.fn_stack.push(FnFrame {
            index,
            nest: 0,
            depth: 0,
            frames: 0,
            decls: HashMap::new(),
            shadow: ScopedShadow::new(),
            closure_base: self.closure_depth,
            rebound: Default::default(),
            params,
            sole_call: head.sole_call,
            scored: vec![false],
            block_scoped: self.hooks.block_scoped_binds(),
        });
        self.assert_mark_entered(node.id());
        self.marks.push((node.id(), Mark::Fn));
    }

    pub(crate) fn push_call(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let head = {
            let cx = self.make_cx();
            hooks.call(node, &cx)
        };
        let Some(head) = head else {
            return;
        };
        let index = self.facts.calls.len();
        if let Some(frame) = self.fn_stack.last_mut()
            && frame.sole_call == Some(node.id())
        {
            self.facts.functions[frame.index].sole_call = Some(narrow(index));
        }
        self.facts.calls.push(Call {
            pos: pos_of(node),
            callee: head.callee,
            kind: head.kind,
            callee_root: head.callee_root,
            is_simple_path: head.is_simple_path,
            initializes_binding: head.initializes_binding,
            args: head.args,
        });
    }

    pub(crate) fn push_imports(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let mut imports = Vec::new();
        {
            let cx = self.make_cx();
            hooks.imports(node, &cx, &mut imports);
        }
        self.facts.imports.extend(imports);
    }

    pub(crate) fn push_assertion(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let assertion = {
            let cx = self.make_cx();
            hooks.assertion(node, &cx)
        };
        if let Some(assertion) = assertion {
            self.facts.assertions.push(assertion);
        }
    }

    pub(crate) fn push_dynamic(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let what = {
            let cx = self.make_cx();
            hooks.dynamic(node, &cx)
        };
        if let Some(what) = what {
            self.facts.dynamic_symbols.push(DynamicSymbol {
                pos: pos_of(node),
                what,
            });
        }
    }

    pub(crate) fn push_struct_lit(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let copy = {
            let cx = self.make_cx();
            hooks.struct_lit(node, &cx)
        };
        if let Some(copy) = copy {
            self.facts.field_copies.push(copy);
        }
    }

    pub(crate) fn push_handler(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let handler = {
            let cx = self.make_cx();
            hooks.handler(node, &cx)
        };
        if let Some(handler) = handler {
            self.facts.handlers.push(handler);
        }
    }

    pub(crate) fn collect_ambient(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let owner = self.fn_stack.last().map(|frame| narrow(frame.index));
        let mut found = Vec::new();
        {
            let cx = self.make_cx();
            hooks.ambient(node, &cx, owner.is_some(), &mut found);
        }
        self.record_rebinding(node, &found);
        for (kind, name, pos) in found {
            if kind == AmbientKind::GlobalWrite
                && owner.is_some()
                && hooks.shadow_filter() != ShadowKind::None
                && !name.contains('.')
                && self.is_shadowed(name, hooks.shadow_filter())
            {
                continue;
            }
            self.facts.ambient.push(Ambient {
                pos,
                kind,
                name,
                in_function: owner,
            });
        }
    }

    fn record_rebinding(
        &mut self,
        node: tree_sitter::Node<'_>,
        found: &[(AmbientKind, &'s str, Pos)],
    ) {
        if !matches!(
            node.kind(),
            "global_statement" | "nonlocal_statement" | "global_declaration"
        ) {
            return;
        }
        let Some(frame) = self.fn_stack.last_mut() else {
            return;
        };
        for (kind, name, _) in found {
            if *kind == AmbientKind::GlobalWrite {
                frame.rebound.insert(*name);
            }
        }
    }

    fn is_shadowed(&self, name: &str, mode: ShadowKind) -> bool {
        let mut declared = false;
        for frame in &self.fn_stack {
            if frame.rebound.contains(name) {
                return false;
            }
            let live = match mode {
                ShadowKind::None => false,
                _ => frame.shadow.contains(name),
            };
            if live || frame.params.contains(name) {
                declared = true;
            }
        }
        declared
    }

    pub(crate) fn apply_write_roots(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let mut roots = Vec::new();
        {
            let cx = self.make_cx();
            hooks.write_roots(node, &cx, &mut roots);
        }
        for (name, pos) in roots {
            self.apply_write(name, pos);
        }
    }

    pub(crate) fn push_decls(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let mut names = Vec::new();
        let parent = {
            let cx = self.make_cx();
            hooks.decl_names(node, &cx, &mut names);
            hooks.decl_scope_parent(self.depth, node, &cx)
        };
        self.insert_decls(names, parent);
    }

    pub(crate) fn insert_decls(&mut self, names: Vec<(&'s str, Pos)>, parent: usize) {
        let at_frames = self.fn_stack.last().map_or(0, |frame| frame.frames);
        for (name, pos) in names {
            let index = self.facts.bindings.len();
            self.facts.bindings.push(MutBinding {
                name,
                pos,
                last_write_line: pos.line,
                first_write_line: pos.line,
                write_lines: 0,
                loop_depth: 0,
            });
            if let Some(frame) = self.fn_stack.last_mut() {
                frame.decls.entry(name).or_default().push(FrameDecl {
                    binding: index,
                    frames_at_decl: at_frames,
                    scope: parent,
                });
                frame.shadow.register(name, parent);
            }
        }
    }

    pub(crate) fn apply_bind(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let mut names = Vec::new();
        {
            let cx = self.make_cx();
            hooks.decl_names(node, &cx, &mut names);
        }
        let parent = {
            let cx = self.make_cx();
            self.hooks.decl_scope_parent(self.depth, node, &cx)
        };
        let mut fresh: Vec<&str> = Vec::new();
        for (name, pos) in names {
            if self.write_or_declare(name, pos, parent) {
                fresh.push(name);
            }
        }
        let mut roots = Vec::new();
        {
            let cx = self.make_cx();
            self.hooks.write_roots(node, &cx, &mut roots);
        }
        for (name, pos) in roots {
            if fresh.contains(&name) {
                continue;
            }
            self.apply_write(name, pos);
        }
    }

    fn write_or_declare(&mut self, name: &'s str, pos: Pos, parent: usize) -> bool {
        let binds_scoped = self.hooks.block_scoped_binds();
        let existing = self
            .fn_stack
            .last()
            .and_then(|frame| frame.decls.get(name))
            .and_then(|stack| stack.last())
            .map(|decl| (decl.binding, decl.frames_at_decl, decl.scope));
        if let Some((binding, frames_at_decl, scope)) = existing
            && !(binds_scoped && scope != parent)
        {
            let depth = self
                .fn_stack
                .last()
                .map_or(0, |frame| frame.frames.saturating_sub(frames_at_decl));
            self.record_write(binding, depth, pos);
            return false;
        }
        let Some(frame) = self.fn_stack.last_mut() else {
            return false;
        };
        let index = self.facts.bindings.len();
        self.facts.bindings.push(MutBinding {
            name,
            pos,
            last_write_line: pos.line,
            first_write_line: pos.line,
            write_lines: 0,
            loop_depth: 0,
        });
        let at_frames = frame.frames;
        let scoped = self.hooks.shadow_filter() == ShadowKind::DeclsAndBinds;
        frame.decls.entry(name).or_default().push(FrameDecl {
            binding: index,
            frames_at_decl: at_frames,
            scope: parent,
        });
        if scoped {
            frame.shadow.register(name, parent);
        }
        true
    }

    fn apply_write(&mut self, name: &str, pos: Pos) {
        let binds_scoped = self.hooks.block_scoped_binds();
        let mut target = None;
        for frame in self.fn_stack.iter().rev() {
            if let Some(decl) = frame.decls.get(name).and_then(|stack| stack.last()) {
                target = Some((
                    decl.binding,
                    frame.frames.saturating_sub(decl.frames_at_decl),
                ));
                break;
            }
            if binds_scoped && frame.params.contains(name) {
                break;
            }
        }
        if let Some((binding, depth)) = target {
            self.record_write(binding, depth, pos);
        }
    }

    fn record_write(&mut self, binding: usize, depth: u32, pos: Pos) {
        let record = &mut self.facts.bindings[binding];
        let prev = record.last_write_line;
        record.last_write_line = prev.max(pos.line);
        if record.write_lines == 0 {
            record.first_write_line = pos.line;
            record.write_lines = 1;
        } else if pos.line > prev {
            record.write_lines += 1;
        }
        record.loop_depth = record.loop_depth.max(depth);
    }

    fn push_container(&mut self, mark: usize, container: Container) {
        if matches!(
            container,
            Container::Impl { .. } | Container::Trait { .. } | Container::Class { .. }
        ) {
            self.method_depth += 1;
        }
        self.containers.push(container);
        self.assert_mark_entered(mark);
        self.marks.push((mark, Mark::Container));
    }

    pub(crate) fn on_comment(&mut self, node: tree_sitter::Node<'_>) {
        let text = slice(self.src, node.start_byte(), node.end_byte());
        if !is_directive_comment(text) {
            return;
        }
        self.comments.push(CommentText {
            start_line: pos_of(node).line,
            end_line: narrow(node.end_position().row) + 1,
            text,
        });
    }
}

impl<'s, 'a> WalkState<'s, 'a, '_> {
    pub(crate) fn enter_function_like(&mut self, role: Role, node: tree_sitter::Node<'_>) {
        let hooks = self.hooks;
        let head = if role == Role::Function {
            let cx = self.make_cx();
            hooks.function(node, &cx)
        } else {
            let parent = self.visited_parent();
            let cx = self.make_cx();
            hooks.closure_as_function(node, parent, &cx)
        };
        if role == Role::Closure && head.is_none() {
            self.closure_depth += 1;
            self.marks.push((node.id(), Mark::Closure));
            self.shadow_closure_params(node);
        }
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
                    self.mark_value_return(self.closure_depth);
                }
            }
        }
    }
    fn shadow_closure_params(&mut self, node: tree_sitter::Node<'_>) {
        if self.fn_stack.is_empty() {
            return;
        }
        let mut names = Vec::new();
        {
            let cx = self.make_cx();
            self.hooks.shadow_names(node, &cx, &mut names);
        }
        if names.is_empty() {
            return;
        }
        let depth = self.depth;
        if let Some(frame) = self.fn_stack.last_mut() {
            for name in names {
                frame.shadow.register(name, depth);
            }
        }
    }

    fn first_named_of_kind<'n>(
        node: tree_sitter::Node<'n>,
        kind: &str,
    ) -> Option<tree_sitter::Node<'n>> {
        node.children(&mut node.walk())
            .find(|child| child.kind() == kind)
    }

    pub(crate) fn enter_return(&mut self, node: tree_sitter::Node<'_>) {
        if node.named_child_count() == 0 {
            return;
        }
        self.mark_value_return(self.closure_depth);
    }

    fn mark_value_return(&mut self, depth: u32) {
        let Some(frame) = self.fn_stack.last_mut() else {
            return;
        };
        if frame.closure_base == depth {
            self.facts.functions[frame.index].has_value_return = true;
        }
    }
}

pub(crate) fn region_of(n: tree_sitter::Node<'_>, role: CodeRole) -> RoleRegion {
    RoleRegion {
        start_line: narrow(n.start_position().row) + 1,
        end_line: narrow(n.end_position().row) + 1,
        role,
    }
}
pub(crate) fn attributed_region(
    item: tree_sitter::Node<'_>,
    leading: Option<tree_sitter::Node<'_>>,
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
