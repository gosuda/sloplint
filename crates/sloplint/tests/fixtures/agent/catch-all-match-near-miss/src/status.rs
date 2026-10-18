pub fn classify(code: u32) -> &'static str {
    match code {
        0 => "zero",
        1 => "one",
        _ => "many",
    }
}
