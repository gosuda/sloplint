        #[cfg(test)]
        {
            let count = self.computed_relations.get();
            if count.is_multiple_of(100_000) {
                let cache = self.cache.borrow();
                let visiting = self.visiting.borrow();
                let pending = cache
                    .values()
                    .flatten()
                    .filter(|entry| !entry.assumptions.is_empty())
                    .count();
                let expired = cache
                    .values()
                    .flatten()
                    .filter(|entry| {
                        entry.assumptions.iter().any(|key| !visiting.contains(key))
                    })
                    .count();
                eprintln!(
                    "CACHE_PROGRESS count={count} cache={} clean={} active={} contexts={} pending={pending} expired={expired} source={source:?} target={target:?}",
                    cache.len(),
                    self.clean.borrow().len(),
                    visiting.len(),
                    self.contexts.borrow().len(),
                );
                for frame in self.dependency_stack.borrow().iter().rev().take(4) {
                    eprintln!("CACHE_FRAME assumptions={:?} erasures={:?} env={}", frame.assumptions, frame.erasure_requirements, frame.env_sensitive);
                }
            }
            assert!(count < 1_000_000, "diagnostic computation bound reached");
        }
