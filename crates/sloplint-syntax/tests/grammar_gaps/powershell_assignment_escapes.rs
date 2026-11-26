//! Native conditional ownership across continuation escapes.

use super::powershell::run_clean;

fn assert_conditional(source: &str, elseifs: usize, assigned: bool) {
    let analysis = run_clean(source);
    assert!(
        analysis
            .facts
            .calls
            .iter()
            .all(|call| !matches!(call.callee, "else" | "elseif")),
        "{source:?}"
    );
    let language: tree_sitter::Language = tree_sitter_powershell::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("powershell language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let mut stack = vec![tree.root_node()];
    let mut conditionals = Vec::new();
    let mut assignments = Vec::new();
    while let Some(node) = stack.pop() {
        if node.kind() == "if_statement" {
            conditionals.push(node);
        } else if node.kind() == "assignment_expression" {
            assignments.push(node);
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    assert_eq!(conditionals.len(), 1, "{source:?}");
    let conditional = conditionals[0];
    assert!(
        conditional.child_by_field_name("else_clause").is_some(),
        "{source:?}"
    );
    let actual = conditional
        .child_by_field_name("elseif_clauses")
        .map_or(0, |clauses| {
            let mut cursor = clauses.walk();
            clauses
                .named_children(&mut cursor)
                .filter(|node| node.kind() == "elseif_clause")
                .count()
        });
    assert_eq!(actual, elseifs, "{source:?}");
    if assigned {
        assert_eq!(assignments.len(), 1, "{source:?}");
        let value = assignments[0]
            .child_by_field_name("value")
            .expect("conditional assignment has a value");
        assert_eq!(value.id(), conditional.id(), "{source:?}");
    } else {
        assert!(assignments.is_empty(), "{source:?}");
    }
}

#[test]
fn escaped_lines_keep_conditional_values_and_branches_together() {
    for (ending_name, ending) in [("lf", "\n"), ("crlf", "\r\n")] {
        for (gap_name, gap) in [
            ("one", format!("{ending}`{ending}")),
            ("comment", format!("{ending}# note{ending}`{ending}")),
            ("indented", format!("{ending}  `{ending}")),
            ("two", format!("{ending}`{ending}`{ending}")),
        ] {
            for (tail_name, tail, elseifs) in [
                ("else", "else { 0 }", 0),
                ("elseif", "elseif ($false) { 2 } else { 0 }", 1),
            ] {
                for (context, prefix, suffix, assigned) in [
                    ("standalone", "", "", false),
                    ("assignment", "$result = ", "", true),
                    ("parenthesized-assignment", "($result = ", ")", true),
                    ("subexpression", "$(", ")", false),
                    ("hashtable-value", "@{ value = ", "\n}", false),
                    ("script-block", "{ $result = ", "\n}", true),
                ] {
                    let suffix = suffix.replace('\n', ending);
                    let source = format!("{prefix}if ($true) {{ 1 }}{gap}{tail}{suffix}{ending}");
                    assert_conditional(&source, elseifs, assigned);
                    println!(
                        "NATIVE_BRANCH_CASE branch-{ending_name}-{gap_name}-{tail_name}-{context}"
                    );
                }
            }
        }
    }
}
