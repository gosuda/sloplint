#[test]
fn transformed_alias_source_assignments_preserve_union_order() {
    let mut observed = Vec::new();
    for (kind, leaf, rejected) in [("incompatible", "string", true), ("compatible", "T", false)] {
        for (order, target) in [
            (
                "shallow-first",
                "{ root: { next: B<string[]> } } | { root: B<string> }",
            ),
            (
                "root-first",
                "{ root: B<string> } | { root: { next: B<string[]> } }",
            ),
        ] {
            let name = format!("{kind}-{order}");
            let text = format!(
                "type A<T> = {{ next: A<T[]>; value: T }};\n\
                 type B<T> = {{ next: B<T[]>; value: {leaf} }};\n\
                 declare const value: {{ root: A<string> }};\n\
                 const result: {target} = value;\n"
            );
            println!("BINDING_ALIAS_SOURCE_BEGIN {name}\n{text}BINDING_ALIAS_SOURCE_END");
            let parsed = parsed(0, &text);
            assert!(
                parsed.diagnostics().is_empty(),
                "{name}: {:?}",
                parsed.diagnostics()
            );
            let checked = check_program_with_options(
                ProgramCheckInput {
                    files: std::slice::from_ref(&parsed),
                    edges: &[],
                },
                &crate::lint::LintTable::new(crate::lint::LintProfile::Default),
                ProgramCheckOptions::standard().with_strict_null_checks(true),
            );
            let codes = checker_codes_of(&checked);
            println!("BINDING_ALIAS_VERDICT {name} {codes:?}");
            observed.push((name, codes, rejected));
        }
    }
    for (name, codes, rejected) in observed {
        if rejected {
            assert_eq!(codes, [TYPE_NOT_ASSIGNABLE.as_str()], "{name}");
        } else {
            assert!(codes.is_empty(), "{name}: {codes:?}");
        }
    }
}
