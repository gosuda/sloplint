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
    /// How many live decls sit at each scope, so `pop_decl_scopes` can
    /// tell in O(1) that an exit closes nothing.
    pub decl_scope_count: HashMap<usize, usize>,
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
    /// Closes the scope ending at `depth` and returns the fact indexes of
    /// bindings whose declarations it held, so callers can stamp their
    /// `end_line`. Non-block-scoped languages keep their decls and return
    /// an empty list, leaving those bindings open-ended.
    pub(crate) fn release(&mut self, depth: usize) -> Vec<usize> {
        self.shadow.release(depth);
        self.only.release(depth);
        if self.block_scoped {
            self.pop_decl_scopes(depth)
        } else {
            Vec::new()
        }
    }
    /// Pops every decl whose scope is `depth`, whatever registered it.
    /// Decls added by the bind path never enter `self.shadow` — languages
    /// without `DeclsAndBinds` filtering skip that registration — so freed
    /// shadow names cannot drive the pop. `decl_scope_count` keeps the
    /// exit path O(1) when no decl closes at this depth (the common case):
    /// scanning `decls` on every node exit would be quadratic over a flat
    /// block of locals.
    pub(crate) fn pop_decl_scopes(&mut self, depth: usize) -> Vec<usize> {
        if !self.decl_scope_count.contains_key(&depth) {
            return Vec::new();
        }
        let mut closed = Vec::new();
        self.decls.retain(|_, stack| {
            while stack.last().is_some_and(|top| top.scope == depth) {
                if let Some(decl) = stack.pop() {
                    closed.push(decl.binding);
                }
            }
            !stack.is_empty()
        });
        self.decl_scope_count.remove(&depth);
        closed
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
