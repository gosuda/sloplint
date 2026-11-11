//! Persistence refusal: baselines never escape the project root.

#[cfg(unix)]
use super::symlink;
use super::{
    SAMPLE, assert_refused, assert_sentinel, baseline_write, mkdir, outer_project, plant,
    ratchet_gate, run_ok, set_baseline, tempdir, write,
};

#[test]
fn audit_persistence_baseline_dotdot_is_refused() {
    let (outer, root) = outer_project();
    set_baseline(&root, "../outside-marker.txt");
    let outside = outer.path().join("outside-marker.txt");
    plant(&outside);
    assert_refused(&baseline_write(&root), "dotdot baseline write");
    assert_sentinel(&outside, "outside marker overwritten by baseline write");
    assert_refused(&ratchet_gate(&root), "dotdot ratchet read");
    assert_sentinel(&outside, "outside marker overwritten by ratchet read");
}

#[test]
fn audit_persistence_baseline_absolute_is_refused() {
    let (outer, root) = outer_project();
    let outside = outer.path().join("outside-abs.txt");
    plant(&outside);
    let target = outside.to_str().unwrap_or_else(|| panic!("non-utf8"));
    set_baseline(&root, target);
    assert_refused(&baseline_write(&root), "absolute baseline write");
    assert_sentinel(&outside, "outside marker overwritten by absolute baseline");
}

#[cfg(unix)]
#[test]
fn audit_persistence_baseline_parent_symlink_is_refused() {
    let (outer, root) = outer_project();
    let outside = outer.path().join("outside");
    mkdir(&outside);
    symlink(&outside, root.join("link"));
    set_baseline(&root, "link/baseline.json");
    assert_refused(&baseline_write(&root), "parent symlink baseline write");
    assert!(
        !outside.join("baseline.json").exists(),
        "baseline escaped through parent symlink"
    );
}

#[cfg(unix)]
#[test]
fn audit_persistence_baseline_leaf_symlink_is_refused() {
    let (outer, root) = outer_project();
    let marker = outer.path().join("marker.txt");
    plant(&marker);
    symlink(&marker, root.join("baseline-link.json"));
    set_baseline(&root, "baseline-link.json");
    assert_refused(&baseline_write(&root), "leaf symlink baseline write");
    assert_sentinel(&marker, "outside marker overwritten through leaf symlink");
}

#[cfg(unix)]
#[test]
fn audit_persistence_baseline_dangling_link_is_refused() {
    let (outer, root) = outer_project();
    let outside = outer.path().join("outside");
    mkdir(&outside);
    symlink(&outside, root.join("link"));
    symlink("link/missing", root.join("cand"));
    set_baseline(&root, "cand");
    assert_refused(&baseline_write(&root), "dangling link baseline write");
    assert!(
        !outside.join("missing").exists(),
        "baseline write escaped through dangling link"
    );
}

#[test]
fn audit_persistence_baseline_nested_in_root_round_trips() {
    let (_tmp, root) = tempdir();
    write(&root, "sample.rs", SAMPLE);
    mkdir(&root.join("nested"));
    set_baseline(&root, "nested/baseline.json");
    run_ok(&baseline_write(&root));
    assert!(
        root.join("nested/baseline.json").is_file(),
        "nested in-root baseline was not written"
    );
    let gated = run_ok(&ratchet_gate(&root));
    assert_eq!(gated.findings.len(), 0, "in-root baseline hid nothing");
    write(&root, "extra.rs", "pub fn manage(flag: bool) {}\n");
    let gated = run_ok(&ratchet_gate(&root));
    assert!(
        gated.findings.iter().all(|f| f.path.as_ref() == "extra.rs"),
        "ratchet reported stale or missing paths: {:?}",
        gated.findings.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
    assert!(
        !gated.findings.is_empty(),
        "new in-root violation was hidden by the baseline"
    );
}
