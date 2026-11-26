                let visiting = self.visiting.borrow();
                entries.retain(|entry| {
                    entry.assumptions.iter().all(|key| visiting.contains(key))
                });
