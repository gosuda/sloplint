    #[test]
    fn shared_recursive_graph_reuses_completed_exact_cycles() {
        let mut table = TypeTable::new();
        let alias = SymbolId::new(9200);
        let parameter = SymbolId::new(9201);
        table.declare_alias(alias, vec![parameter]);
        let value = table.named(parameter);
        let recursive = table.applied_alias(alias, vec![value]);
        let body = table.object_type(vec![
            PropertyType::new("next", false, recursive),
            PropertyType::new("value", false, value),
        ]);
        table.publish_alias_template(alias, body);
        let literal = table.string_literal("value");
        let string = table.string();
        let mut source = table.applied_alias(alias, vec![literal]);
        let mut target = table.applied_alias(alias, vec![string]);
        for _ in 0..12 {
            source = table.object_type(vec![
                PropertyType::new("left", false, source),
                PropertyType::new("right", false, source),
            ]);
            target = table.object_type(vec![
                PropertyType::new("left", false, target),
                PropertyType::new("right", false, target),
            ]);
        }
        let relations = TypeRelations::new(&table);
        assert!(relations.assignable(source, target));
        let computed = relations.computed_relations();
        assert!(
            computed < 64,
            "shared recursive graph recomputed {computed} relations"
        );
        assert!(relations.assignable(source, target));
        assert_eq!(relations.computed_relations(), computed);
    }

    #[test]
    fn shared_transformed_recursive_graph_reuses_completed_cycles() {
        let mut table = TypeTable::new();
        let alias = SymbolId::new(9200);
        let parameter = SymbolId::new(9201);
        table.declare_alias(alias, vec![parameter]);
        let value = table.named(parameter);
        let array = table.array(value);
        let recursive = table.applied_alias(alias, vec![array]);
        let body = table.object_type(vec![
            PropertyType::new("next", false, recursive),
            PropertyType::new("value", false, value),
        ]);
        table.publish_alias_template(alias, body);
        let literal = table.string_literal("value");
        let string = table.string();
        let mut source = table.applied_alias(alias, vec![literal]);
        let mut target = table.applied_alias(alias, vec![string]);
        for _ in 0..12 {
            source = table.object_type(vec![
                PropertyType::new("left", false, source),
                PropertyType::new("right", false, source),
            ]);
            target = table.object_type(vec![
                PropertyType::new("left", false, target),
                PropertyType::new("right", false, target),
            ]);
        }
        let relations = TypeRelations::new(&table);
        assert!(relations.assignable(source, target));
        let computed = relations.computed_relations();
        assert!(
            computed < 64,
            "shared transformed recursive graph recomputed {computed} relations"
        );
        assert!(relations.assignable(source, target));
        assert_eq!(relations.computed_relations(), computed);
    }

    #[test]
    fn transformed_recursive_aliases_do_not_reuse_provisional_success() {
        let mut table = TypeTable::new();
        let left = SymbolId::new(9210);
        let left_parameter = SymbolId::new(9211);
        let right = SymbolId::new(9212);
        let right_parameter = SymbolId::new(9213);
        for (alias, parameter) in [(left, left_parameter), (right, right_parameter)] {
            table.declare_alias(alias, vec![parameter]);
            let value = table.named(parameter);
            let array = table.array(value);
            let recursive = table.applied_alias(alias, vec![array]);
            let body = table.object_type(vec![
                PropertyType::new("a", false, recursive),
                PropertyType::new("z", false, value),
            ]);
            table.publish_alias_template(alias, body);
        }
        let string = table.string();
        let number = table.number();
        let strings = table.array(string);
        let numbers = table.array(number);
        let source = table.applied_alias(left, vec![string]);
        let target = table.applied_alias(right, vec![number]);
        let nested_source = table.applied_alias(left, vec![strings]);
        let nested_target = table.applied_alias(right, vec![numbers]);
        let valid = table.applied_alias(right, vec![string]);
        let relations = TypeRelations::new(&table);
        assert!(!relations.assignable(source, target));
        assert!(
            !relations.assignable(nested_source, nested_target),
            "failed transformed alias branch left a provisional success"
        );
        assert!(!TypeRelations::new(&table).assignable(nested_source, nested_target));
        assert!(relations.assignable(source, valid));
    }

