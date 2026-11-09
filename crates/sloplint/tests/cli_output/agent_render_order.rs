//! Agent lines keep report order across the renderer's parallel windows, at any job count.

use std::path::Path;
use std::process::{Command, Output};

use serde::Deserialize;

const FINDINGS: usize = 20000;

#[derive(Deserialize)]
struct Row {
    path: String,
    line: u32,
    column: u32,
}

#[derive(Deserialize)]
struct Document {
    findings: Vec<Row>,
}

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", dir.display()));
    }
    std::fs::write(&path, body).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

fn project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let mut source = String::new();
    for i in 0..FINDINGS {
        source.push_str(&format!("pub fn f{i}(flag: bool) {{}}\n"));
    }
    write(tmp.path(), "sloplint.toml", "");
    write(tmp.path(), "src/lib.rs", &source);
    tmp
}

fn sloplint(cwd: &Path, args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn sloplint: {e}"))
}

fn check(format: &str, jobs: u32) -> Vec<String> {
    vec![
        "check".to_string(),
        "--no-cache".to_string(),
        "--strictness".to_string(),
        "strict".to_string(),
        "--rule".to_string(),
        "complexity/boolean-params".to_string(),
        "--fail-on".to_string(),
        "never".to_string(),
        "--format".to_string(),
        format.to_string(),
        "--jobs".to_string(),
        jobs.to_string(),
    ]
}

fn agent_locations(stdout: &[u8]) -> Vec<(String, u32, u32)> {
    let text =
        std::str::from_utf8(stdout).unwrap_or_else(|e| panic!("agent stdout is not UTF-8: {e}"));
    text.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("src/lib.rs:")?;
            let mut fields = rest.splitn(3, ':');
            let line = fields.next()?.parse().ok()?;
            let column = fields.next()?.parse().ok()?;
            Some(("src/lib.rs".to_string(), line, column))
        })
        .collect()
}

#[test]
fn agent_lines_follow_report_order_across_render_windows() {
    let tmp = project();
    let one = sloplint(tmp.path(), &check("agent", 1));
    assert_eq!(
        one.status.code(),
        Some(0),
        "jobs 1 stderr: {}",
        String::from_utf8_lossy(&one.stderr)
    );
    let four = sloplint(tmp.path(), &check("agent", 4));
    assert_eq!(
        four.status.code(),
        Some(0),
        "jobs 4 stderr: {}",
        String::from_utf8_lossy(&four.stderr)
    );
    assert_eq!(
        one.stdout, four.stdout,
        "agent bytes differ between --jobs 1 and --jobs 4"
    );

    let json = sloplint(tmp.path(), &check("json", 4));
    assert_eq!(
        json.status.code(),
        Some(0),
        "json stderr: {}",
        String::from_utf8_lossy(&json.stderr)
    );
    let doc: Document = sonic_rs::from_slice(&json.stdout)
        .unwrap_or_else(|e| panic!("json stdout is not JSON: {e}"));
    let reported: Vec<(String, u32, u32)> = doc
        .findings
        .iter()
        .map(|row| (row.path.clone(), row.line, row.column))
        .collect();
    assert_eq!(
        reported.len(),
        FINDINGS,
        "fixture must report one boolean-params finding per line"
    );

    let rendered = agent_locations(&four.stdout);
    assert_eq!(
        rendered.len(),
        FINDINGS,
        "agent output must carry one finding line per reported finding"
    );
    assert_eq!(
        rendered, reported,
        "agent --jobs 4 location order differs from the JSON report order"
    );
}
