use std::collections::{HashMap, HashSet};
pub(crate) struct ScopedShadow<'s> {
    counts: HashMap<&'s str, u32>,
    stack: Vec<(usize, &'s str)>,
    tops: HashMap<&'s str, Vec<usize>>,
}
impl<'s> ScopedShadow<'s> {
    pub(crate) fn new() -> Self {
        Self {
            counts: HashMap::new(),
            stack: Vec::new(),
            tops: HashMap::new(),
        }
    }
    pub(crate) fn register(&mut self, name: &'s str, parent: usize) {
        let slot = self.counts.entry(name).or_insert(0);
        *slot = slot.saturating_add(1);
        let at = self.stack.partition_point(|(open, _)| *open <= parent);
        self.stack.insert(at, (parent, name));
        let scopes = self.tops.entry(name).or_default();
        let at = scopes.partition_point(|open| *open <= parent);
        scopes.insert(at, parent);
    }
    pub(crate) fn release(&mut self, depth: usize) -> Vec<&'s str> {
        let mut freed = Vec::new();
        while let Some((parent, name)) = self.stack.last().copied() {
            if parent != depth {
                break;
            }
            self.stack.pop();
            freed.push(name);
            if let Some(vec) = self.tops.get_mut(name) {
                vec.pop();
                if vec.is_empty() {
                    self.tops.remove(name);
                }
            }
            let empty = if let Some(slot) = self.counts.get_mut(name) {
                *slot = slot.saturating_sub(1);
                *slot == 0
            } else {
                false
            };
            if empty {
                self.counts.remove(name);
            }
        }
        freed
    }
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.counts.contains_key(name)
    }
    pub(crate) fn top_scope(&self, name: &str) -> Option<usize> {
        self.tops.get(name).and_then(|vec| vec.last().copied())
    }
}
pub(crate) struct FrameDecl {
    pub binding: usize,
    pub frames_at_decl: u32,
    pub scope: usize,
}
pub(crate) struct FnFrame<'s> {
    pub index: usize,
    pub nest: u32,
    pub depth: u32,
    pub frames: u32,
    pub decls: HashMap<&'s str, Vec<FrameDecl>>,
    pub shadow: ScopedShadow<'s>,
    pub only: ScopedShadow<'s>,
    pub top: usize,
    pub closure_base: u32,
    pub forked_base: u32,
    pub rebound: HashSet<&'s str>,
    pub params: HashSet<&'s str>,
    pub sole_call: Option<usize>,
    pub scored: Vec<bool>,
    pub block_scoped: bool,
}
impl<'s> FnFrame<'s> {
    pub(crate) fn release(&mut self, depth: usize) {
        let freed = self.shadow.release(depth);
        self.only.release(depth);
        if self.block_scoped {
            self.pop_decl_scopes(depth, &freed);
        }
    }
    pub(crate) fn pop_decl_scopes(&mut self, depth: usize, freed: &[&'s str]) {
        for name in freed {
            if let Some(stack) = self.decls.get_mut(name) {
                while stack.last().is_some_and(|top| top.scope == depth) {
                    stack.pop();
                }
                if stack.is_empty() {
                    self.decls.remove(name);
                }
            }
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Mark {
    Nest,
    Depth,
    Frame,
    Fn,
    Closure,
    Container,
}
