//! Dafny match-scrutinee classification tests: the catch-all-match rule
//! reads `ScrutineeKind`, so field reads must stay `Field` while computed
//! values (updates, negations, call results) classify as `Other`.

use super::dafny::run_clean;
use sloplint_core::facts::ScrutineeKind;

#[test]
fn datatype_update_scrutinee_is_not_a_field() {
    // `match p.(x := 1)` computes a new value — the update target `x`
    // follows `(`, not `.`. Classifying it as a field access would let
    // catch-all-match excuse a wildcard arm on a computed scrutinee.
    let analysis = run_clean(concat!(
        "datatype P = P(x: int)\n",
        "method M(p: P)\n",
        "{\n",
        "  match p.(x := 1)\n",
        "  case P(0) => print 0;\n",
        "  case _ => print 1;\n",
        "}\n",
    ));
    assert_eq!(analysis.facts.wildcard_arms.len(), 1);
    assert_eq!(
        analysis.facts.wildcard_arms[0].scrutinee,
        ScrutineeKind::Other
    );
}

#[test]
fn commented_field_access_still_reads_as_a_field() {
    // `obj./* note */state` parks a comment between the dot and the field
    // name; the scrutinee is still a field access.
    let analysis = run_clean(concat!(
        "datatype P = P(x: int)\n",
        "method M(p: P)\n",
        "{\n",
        "  match p./* note */x\n",
        "  case 0 => print 0;\n",
        "  case _ => print 1;\n",
        "}\n",
    ));
    assert_eq!(analysis.facts.wildcard_arms.len(), 1);
    assert_eq!(
        analysis.facts.wildcard_arms[0].scrutinee,
        ScrutineeKind::Field
    );
}

#[test]
fn negated_scrutinee_is_not_a_field() {
    // `match -(p./* note */x)` wraps the field access in a computing
    // operator: the scrutinee value is a negation, not the field, so a
    // wildcard arm must stay silent under catch-all-match.
    let analysis = run_clean(concat!(
        "datatype P = P(x: int)\n",
        "method M(p: P)\n",
        "{\n",
        "  match -(p./* note */x)\n",
        "  case 0 => print 0;\n",
        "  case _ => print 1;\n",
        "}\n",
    ));
    assert_eq!(analysis.facts.wildcard_arms.len(), 1);
    assert_eq!(
        analysis.facts.wildcard_arms[0].scrutinee,
        ScrutineeKind::Other
    );
}

#[test]
fn negated_base_suffix_is_not_a_field() {
    // `-p.x` parses as `(-p).x`: a member of the negated value, not a
    // field of a named scrutinee — catch-all-match must stay silent.
    let analysis = run_clean(concat!(
        "datatype P = P(x: int)\n",
        "method M(p: P)\n",
        "{\n",
        "  match -p.x\n",
        "  case _ => print 1;\n",
        "}\n",
    ));
    assert_eq!(analysis.facts.wildcard_arms.len(), 1);
    assert_eq!(
        analysis.facts.wildcard_arms[0].scrutinee,
        ScrutineeKind::Other
    );
}
#[test]
fn this_receiver_field_still_reads_as_a_field() {
    // `match this.state` reads a field of the current instance — a named
    // scrutinee, so catch-all-match still applies.
    let analysis = run_clean(concat!(
        "class C {\n",
        "  var state: int\n",
        "  method M()\n",
        "  {\n",
        "    match this.state\n",
        "    case 0 => print 0;\n",
        "    case _ => print 1;\n",
        "  }\n",
        "}\n",
    ));
    assert_eq!(analysis.facts.wildcard_arms.len(), 1);
    assert_eq!(
        analysis.facts.wildcard_arms[0].scrutinee,
        ScrutineeKind::Field
    );
}

#[test]
fn commented_paren_scrutinee_still_reads_as_a_field() {
    // `(/* note */ p).state` — the comment inside the parens must not
    // hide the receiver; the scrutinee is still a field read.
    let analysis = run_clean(concat!(
        "datatype P = P(x: int)\n",
        "method M(p: P)\n",
        "{\n",
        "  match (/* note */ p).x\n",
        "  case 0 => print 0;\n",
        "  case _ => print 1;\n",
        "}\n",
    ));
    assert_eq!(analysis.facts.wildcard_arms.len(), 1);
    assert_eq!(
        analysis.facts.wildcard_arms[0].scrutinee,
        ScrutineeKind::Field
    );
}


#[test]
fn called_receiver_is_a_computed_scrutinee() {
    // `match this.Get().state` — the receiver threads a call, so the
    // scrutinee is computed even though it ends in `.state`.
    let analysis = run_clean(concat!(
        "class C {\n",
        "  var state: int\n",
        "  method Get() returns (x: int) { return 0; }\n",
        "  method M()\n",
        "  {\n",
        "    match this.Get().state\n",
        "    case 0 => print 0;\n",
        "    case _ => print 1;\n",
        "  }\n",
        "}\n",
    ));
    assert_eq!(analysis.facts.wildcard_arms.len(), 1);
    assert_eq!(
        analysis.facts.wildcard_arms[0].scrutinee,
        ScrutineeKind::Other
    );
}

#[test]
fn indexed_receiver_is_a_computed_scrutinee() {
    // `match p.arr[0]` — an index read mid-receiver is not a field read.
    let analysis = run_clean(concat!(
        "datatype P = P(arr: array<int>)\n",
        "method M(p: P)\n",
        "{\n",
        "  match p.arr[0]\n",
        "  case 0 => print 0;\n",
        "  case _ => print 1;\n",
        "}\n",
    ));
    assert_eq!(analysis.facts.wildcard_arms.len(), 1);
    assert_eq!(
        analysis.facts.wildcard_arms[0].scrutinee,
        ScrutineeKind::Other
    );
}
