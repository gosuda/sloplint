    #[test]
    fn shared_active_cycle_scales_after_context_cache_saturation() {
        let mut table = TypeTable::new();
        let arrays: Vec<_> = (0..100)
            .map(|index| {
                let literal = table.number_literal(&index.to_string());
                table.array(literal)
            })
            .collect();
        let alias = SymbolId::new(9500);
        let parameter = SymbolId::new(9501);
        table.declare_alias(alias, vec![parameter]);
        let value = table.named(parameter);
        let back = table.applied_alias(alias, vec![value]);
        let mut body = table.object_type(vec![
            PropertyType::new("back", false, back),
            PropertyType::new("value", false, value),
        ]);
        for _ in 0..12 {
            body = table.object_type(vec![
                PropertyType::new("left", false, body),
                PropertyType::new("right", false, body),
            ]);
        }
        table.publish_alias_template(alias, body);
        let literal = table.string_literal("value");
        let string = table.string();
        let source = table.applied_alias(alias, vec![literal]);
        let target = table.applied_alias(alias, vec![string]);
        let relations = TypeRelations::new(&table);
        'saturate: for &source in &arrays {
            for &target in &arrays {
                assert_eq!(relations.assignable(source, target), source == target);
                if relations.cached_pairs() == 4096 {
                    break 'saturate;
                }
            }
        }
        assert_eq!(relations.cached_pairs(), 4096);
        let before = relations.computed_relations();
        assert!(relations.assignable(source, target));
        let computed = relations.computed_relations() - before;
        assert!(
            computed < 64,
            "shared active cycle recomputed {computed} relations after saturation"
        );
    }

