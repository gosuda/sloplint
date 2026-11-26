    #[test]
    fn completed_alias_approximations_do_not_prove_changed_arguments() {
        let mut table = TypeTable::new();
        let string = table.string();
        let strings = table.array(string);
        let left = SymbolId::new(9600);
        let left_parameter = SymbolId::new(9601);
        let right = SymbolId::new(9602);
        let right_parameter = SymbolId::new(9603);
        for (alias, parameter) in [(left, left_parameter), (right, right_parameter)] {
            table.declare_alias(alias, vec![parameter]);
            let value = table.named(parameter);
            let array = table.array(value);
            let recursive = table.applied_alias(alias, vec![array]);
            let body = table.object_type(vec![
                PropertyType::new("next", false, recursive),
                PropertyType::new("value", false, if alias == left { value } else { string }),
            ]);
            table.publish_alias_template(alias, body);
        }
        let source = table.applied_alias(left, vec![string]);
        let target = table.applied_alias(right, vec![string]);
        let nested_source = table.applied_alias(left, vec![strings]);
        let nested_target = table.applied_alias(right, vec![strings]);
        let relations = TypeRelations::new(&table);
        let _ = relations.assignable(source, target);
        assert!(!TypeRelations::new(&table).assignable(nested_source, nested_target));
        assert!(
            !relations.assignable(nested_source, nested_target),
            "a completed outer comparison promoted an unproved changed-argument alias"
        );
    }

