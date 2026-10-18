pub fn render_table(header: bool, footer: bool) -> String {
    let mut out = String::new();
    if header {
        out.push_str("h");
    }
    if footer {
        out.push_str("f");
    }
    out
}
