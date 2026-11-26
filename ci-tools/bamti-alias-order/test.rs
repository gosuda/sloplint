
    #[test]
    fn transformed_alias_union_order_is_stable() {
        let mut verdicts = Vec::new();
        for reversed in [false, true] {
            let mut table = TypeTable::new();
            let string = table.string();
            let strings = table.array(string);
            let left = SymbolId::new(9900);
            let left_parameter = SymbolId::new(9901);
            let right = SymbolId::new(9902);
            let right_parameter = SymbolId::new(9903);
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
            let source_alias = table.applied_alias(left, vec![string]);
            let target_alias = table.applied_alias(right, vec![string]);
            let nested_source = table.applied_alias(left, vec![strings]);
            let nested_target = table.applied_alias(right, vec![strings]);
            let source = table.object_type(vec![PropertyType::new("root", false, source_alias)]);
            let shallow = table.object_type(vec![PropertyType::new("next", false, nested_target)]);
            let (first, second) = if reversed {
                let second = table.object_type(vec![PropertyType::new("root", false, target_alias)]);
                let first = table.object_type(vec![PropertyType::new("root", false, shallow)]);
                (first, second)
            } else {
                let first = table.object_type(vec![PropertyType::new("root", false, shallow)]);
                let second = table.object_type(vec![PropertyType::new("root", false, target_alias)]);
                (first, second)
            };
            let target = table.union(&[first, second]);
            let Type::Union(members) = table.get(target) else {
                panic!("distinct union members must be retained");
            };
            assert_eq!(members[0], if reversed { second } else { first });
            let direct = TypeRelations::new(&table).assignable(source_alias, target_alias);
            let primed = TypeRelations::new(&table);
            assert!(!primed.assignable(nested_source, nested_target));
            let after_refutation = primed.assignable(source_alias, target_alias);
            let verdict = TypeRelations::new(&table).assignable(source, target);
            println!("ALIAS_ORDER reversed={reversed} direct={direct} primed={after_refutation} union={verdict}");
            verdicts.push(verdict);
        }
        assert_eq!(verdicts[0], verdicts[1], "transformed alias union order changed compatibility");
    }
