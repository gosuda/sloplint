        #[cfg(test)]
        {
            let key = RelationKey {
                source,
                target,
                strictness,
                context: self.active_context.get(),
            };
            *self.diagnostic_visits.borrow_mut().entry(key).or_default() += 1;
            if self.computed_relations.get() == 1_000_000 {
                let cache = self.cache.borrow();
                let visiting = self.visiting.borrow();
                let aliases = self.diagnostic_alias_assumptions.borrow();
                let outcomes = self.diagnostic_outcomes.borrow();
                let mut hot: Vec<_> = self.diagnostic_visits.borrow().iter()
                    .map(|(&key, &count)| (count, key))
                    .collect();
                hot.sort_unstable_by(|left, right| right.cmp(left));
                eprintln!("PROFILE_TOTAL cache={} clean={} contexts={} alias_keys={}", cache.len(), self.clean.borrow().len(), self.contexts.borrow().len(), aliases.len());
                for (count, key) in hot.into_iter().take(8) {
                    eprintln!("PROFILE_HOT {count} {key:?}");
                    eprintln!("PROFILE_SOURCE {:?}", self.table.get(key.source));
                    eprintln!("PROFILE_TARGET {:?}", self.table.get(key.target));
                    eprintln!("PROFILE_OUTCOME {:?}", outcomes.get(&key));
                    if let Some(entries) = cache.get(&key) {
                        eprintln!("PROFILE_ENTRIES {}", entries.len());
                        for entry in entries.iter().take(2) {
                            eprintln!("PROFILE_ENTRY {entry:?}");
                            for dependency in entry.assumptions.iter().take(4) {
                                eprintln!(
                                    "PROFILE_DEPENDENCY {dependency:?} active={} alias={} outcome={:?}",
                                    visiting.contains(dependency),
                                    aliases.contains(dependency),
                                    outcomes.get(dependency),
                                );
                            }
                        }
                    }
                }
                panic!("diagnostic computation bound reached");
            }
        }
