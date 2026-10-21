//! Fact emission for the walk state: every method that appends one fact.

use std::collections::HashMap;

use sloplint_core::facts::{Ambient, Call, DynamicSymbol, Function, MutBinding, Pos};
use sloplint_core::suppress::CommentText;

use crate::hooks::{Container, FunctionHead, is_directive_comment, narrow, pos_of, slice};
use crate::walk::{FnFrame, FrameDecl, Mark, WalkState};

impl<'s, 'a> WalkState<'s, 'a, '_> {
    pub(crate) fn enter_typedef(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.spec.hooks;
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
        let hooks = self.spec.hooks;
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
        let hooks = self.spec.hooks;
        let container = {
            let cx = self.make_cx();
            hooks.container(node, &cx)
        };
        if let Some(container) = container {
            self.push_container(node.id(), container);
        }
    }

    pub(crate) fn enter_match_arm(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.spec.hooks;
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
            self.spec.hooks.untyped_throws(node, &cx)
        };
        let index = self.facts.functions.len();
        self.facts.functions.push(Function {
            name: head.name,
            pos: head.name_pos,
            doc: head.doc,
            doc_pos: head.doc_pos,
            public: head.public,
            interface_bound: head.interface_bound,
            in_test: head.in_test,
            params: head.params,
            return_type: head.return_type,
            untyped_throws,
            missing_annotations: head.missing_annotations,
            cognitive: 0,
            max_depth: 0,
            deepest_line: 0,
            sole_call: None,
        });
        self.fn_stack.push(FnFrame {
            index,
            nest: 0,
            frames: 0,
            decls: HashMap::new(),
            sole_call: head.sole_call,
        });
        self.marks.push((node.id(), Mark::Fn));
    }

    pub(crate) fn push_call(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.spec.hooks;
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
            args: head.args,
        });
    }

    pub(crate) fn push_imports(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.spec.hooks;
        let mut imports = Vec::new();
        {
            let cx = self.make_cx();
            hooks.imports(node, &cx, &mut imports);
        }
        self.facts.imports.extend(imports);
    }

    pub(crate) fn push_assertion(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.spec.hooks;
        let assertion = {
            let cx = self.make_cx();
            hooks.assertion(node, &cx)
        };
        if let Some(assertion) = assertion {
            self.facts.assertions.push(assertion);
        }
    }

    pub(crate) fn push_dynamic(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.spec.hooks;
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
        let hooks = self.spec.hooks;
        let copy = {
            let cx = self.make_cx();
            hooks.struct_lit(node, &cx)
        };
        if let Some(copy) = copy {
            self.facts.field_copies.push(copy);
        }
    }

    pub(crate) fn push_handler(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.spec.hooks;
        let handler = {
            let cx = self.make_cx();
            hooks.handler(node, &cx)
        };
        if let Some(handler) = handler {
            self.facts.handlers.push(handler);
        }
    }

    pub(crate) fn collect_ambient(&mut self, node: tree_sitter::Node<'_>) {
        let hooks = self.spec.hooks;
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
        let hooks = self.spec.hooks;
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
        let hooks = self.spec.hooks;
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
                max_nested_depth: 0,
                nested_write_pos: None,
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
        let hooks = self.spec.hooks;
        let mut names = Vec::new();
        {
            let cx = self.make_cx();
            hooks.decl_names(node, &cx, &mut names);
        }
        for (name, pos) in names {
            self.write_or_declare(name, pos);
        }
        self.apply_write_roots(node);
    }

    fn write_or_declare(&mut self, name: &'s str, pos: Pos) {
        let Some(frame) = self.fn_stack.last_mut() else {
            return;
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
                max_nested_depth: 0,
                nested_write_pos: None,
            });
            let at_frames = frame.frames;
            frame.decls.insert(
                name,
                FrameDecl {
                    binding: index,
                    frames_at_decl: at_frames,
                },
            );
            return;
        };
        let depth = frame.frames.saturating_sub(frames_at_decl);
        self.record_write(binding, depth, pos);
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
        record.last_write_line = record.last_write_line.max(pos.line);
        if depth > record.max_nested_depth {
            record.max_nested_depth = depth;
            record.nested_write_pos = Some(pos);
        }
    }

    fn push_container(&mut self, mark: usize, container: Container) {
        if matches!(container, Container::TestModule) {
            self.facts.has_inline_tests = true;
        }
        self.containers.push(container);
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
