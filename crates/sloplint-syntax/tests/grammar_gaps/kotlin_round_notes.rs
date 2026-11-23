//! Round-notes consistency for the Kotlin grammar: `SLOPLINT.md` must name
//! each merge-point conflict group twice, and every named group must be a
//! live entry in the `conflicts` list `grammar.js` declares.

/// `SLOPLINT.md` names each grammar round twice: once in the header summary
/// and once as a `## Round N` section. The two must agree, and the conflict
/// groups the section names for that round must be live entries in the
/// `conflicts` list `grammar.js` declares. The groups come from the notes
/// (renaming one there fails the test) and commented-out lines never count
/// as live entries (commenting one out fails the test).
#[test]
fn sloplint_md_round_notes_match_the_grammar() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/tree-sitter-kotlin-ng");
    let notes = std::fs::read_to_string(dir.join("SLOPLINT.md"))
        .expect("vendor/tree-sitter-kotlin-ng/SLOPLINT.md must be readable");
    let grammar = std::fs::read_to_string(dir.join("grammar.js"))
        .expect("vendor/tree-sitter-kotlin-ng/grammar.js must be readable");

    let round = notes
        .lines()
        .find_map(|line| {
            line.strip_prefix("Round ")
                .and_then(|rest| rest.split(' ').next())
                .and_then(|n| n.parse::<u32>().ok())
                .filter(|_| line.contains("if`/`else` branch bodies"))
        })
        .expect("SLOPLINT.md must open with a 'Round N ... if/else branch bodies' summary");
    assert!(
        notes.contains(&format!(
            "## Round {round} (loop statements as if/else branch bodies)"
        )),
        "the summary calls the branch-body change Round {round}, so the dedicated section must carry the same number"
    );

    let section_at = notes
        .find(&format!(
            "## Round {round} (loop statements as if/else branch bodies)"
        ))
        .expect("the Round section must exist for the conflict check");
    let section = &notes[section_at..];
    let section = &section[..section.find("\n## ").unwrap_or(section.len())];
    assert!(
        section.contains("conflict declarations covering the new merge points"),
        "the Round section must name the merge-point conflicts it adds"
    );
    let list_at = section
        .find("covering the new merge points")
        .expect("the Round section must list its conflict groups in parentheses");
    let list = section[list_at + "covering the new merge points".len()..].trim_start();
    let list = list
        .strip_prefix('(')
        .expect("the conflict group list must open right after the phrase");
    let groups_end = list.find(')').expect("the conflict group list must close");
    let groups: Vec<String> = list[..groups_end]
        .split(',')
        .map(|group| {
            let names: Vec<String> = group
                .split('/')
                .map(|name| name.trim().trim_matches('`').trim_matches('\'').to_string())
                .collect();
            assert!(
                !names.iter().any(|name| name.is_empty()),
                "every conflict group in the notes must name its rules, got {group:?}"
            );
            format!(
                "[{}]",
                names
                    .iter()
                    .map(|name| format!("$.{name}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect();
    assert!(
        !groups.is_empty(),
        "the Round section must list at least one conflict group"
    );

    let conflicts_at = grammar
        .find("conflicts:")
        .expect("grammar.js must declare a conflicts list");
    let conflicts_end = grammar[conflicts_at..]
        .find("\n  ],")
        .expect("grammar.js conflicts list must close")
        + conflicts_at;
    let conflicts = &grammar[conflicts_at..conflicts_end];
    let live: Vec<&str> = conflicts
        .lines()
        .map(|line| line.split("//").next().unwrap_or("").trim())
        .collect();
    for declared in &groups {
        assert!(
            live.iter()
                .any(|line| line.trim_end_matches(',') == declared),
            "grammar.js conflicts must declare {declared} as a live entry for the merged if-then state"
        );
    }
}
