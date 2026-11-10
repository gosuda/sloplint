#[cfg(not(test))]
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

#[allow(clippy::test_helper)]
pub fn wrap_text(pad: bool, trim: bool) -> String {
    let mut out = String::new();
    if pad {
        out.push(' ');
    }
    if trim {
        out.pop();
    }
    out
}

pub fn clamp_range(low: bool, high: bool) -> bool {
    low && high
}

#[cfg(not(test))]
mod prod {
    pub fn pick_cell(row: bool, col: bool) -> usize {
        usize::from(row) + 2 * usize::from(col)
    }
}

#[test]
fn table_smoke() {
    assert_eq!(super::clamp_range(true, false), false);
}

#[cfg(test)]
mod tests {
    use super::clamp_range;

    #[test]
    fn clamps() {
        assert!(!clamp_range(true, false));
    }

    pub fn either(left: bool, right: bool) -> bool {
        left || right
    }
}
