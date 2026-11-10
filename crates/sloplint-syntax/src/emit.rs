use std::collections::HashMap;

use sloplint_core::facts::{Ambient, Call, DynamicSymbol, Function, MutBinding, Pos};
use sloplint_core::role::CodeRole;
use sloplint_core::suppress::CommentText;

use crate::hooks::{
    Container, FunctionHead, attributed_region, is_directive_comment, narrow, pos_of, slice,
};
use crate::walk::{FnFrame, FrameDecl, Mark, WalkState};

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
            sole_call: head.sole_call,
            scored: vec![false],
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
        for (kind, name, pos) in found {
            self.facts.ambient.push(Ambient {
                pos,
                kind,
                name,
                in_function: owner,
            });
        }
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
        {
            let cx = self.make_cx();
            hooks.decl_names(node, &cx, &mut names);
        }
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
                frame.decls.insert(
                    name,
                    FrameDecl {
                        binding: index,
                        frames_at_decl: at_frames,
                    },
                );
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
        let mut fresh: Vec<&str> = Vec::new();
        for (name, pos) in names {
            if self.write_or_declare(name, pos) {
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

    fn write_or_declare(&mut self, name: &'s str, pos: Pos) -> bool {
        let Some(frame) = self.fn_stack.last_mut() else {
            return false;
        };
        let found = frame
            .decls
            .get(name)
            .map(|decl| (decl.binding, decl.frames_at_decl));
        let Some((binding, frames_at_decl)) = found else {
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
            frame.decls.insert(
                name,
                FrameDecl {
                    binding: index,
                    frames_at_decl: at_frames,
                },
            );
            return true;
        };
        let depth = frame.frames.saturating_sub(frames_at_decl);
        self.record_write(binding, depth, pos);
        false
    }

    fn apply_write(&mut self, name: &str, pos: Pos) {
        let mut target = None;
        for frame in self.fn_stack.iter().rev() {
            if let Some(decl) = frame.decls.get(name) {
                target = Some((
                    decl.binding,
                    frame.frames.saturating_sub(decl.frames_at_decl),
                ));
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
