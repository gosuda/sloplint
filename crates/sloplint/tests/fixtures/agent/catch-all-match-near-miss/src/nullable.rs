pub fn describe(opt: Option<u32>) -> &'static str {
    match opt {
        Some(x) => "some",
        _ => "other",
    }
}
