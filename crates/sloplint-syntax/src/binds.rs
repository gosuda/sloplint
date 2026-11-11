use sloplint_core::facts::{MutBinding, Pos};

use crate::hooks::ShadowKind;
use crate::shadow::FrameDecl;
use crate::walk::WalkState;

impl<'s, 'a> WalkState<'s, 'a, '_> {
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
        let sorted = fresh.len() > 1;
        if sorted {
            fresh.sort_unstable();
            fresh.dedup();
        }
        for (name, pos) in roots {
            let skip = if sorted {
                fresh.binary_search(&name).is_ok()
            } else {
                fresh.first().is_some_and(|found| *found == name)
            };
            if skip {
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
        if binds_scoped
            && self
                .fn_stack
                .last()
                .is_some_and(|frame| frame.params.contains(name) && parent == frame.top)
        {
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
        let mut target = None;
        for frame in self.fn_stack.iter().rev() {
            if frame.only.top_scope(name).is_some_and(|scope| {
                frame
                    .decls
                    .get(name)
                    .and_then(|stack| stack.last())
                    .is_none_or(|decl| scope > decl.scope)
            }) {
                return;
            }
            if let Some(decl) = frame.decls.get(name).and_then(|stack| stack.last()) {
                target = Some((
                    decl.binding,
                    frame.frames.saturating_sub(decl.frames_at_decl),
                ));
                break;
            }
            if frame.params.contains(name) {
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
}
