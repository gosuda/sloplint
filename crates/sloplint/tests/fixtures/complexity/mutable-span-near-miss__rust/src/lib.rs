pub fn collect(items: &[i32]) -> Vec<i32> {
    let mut segments = Vec::new();
    for item in items {
        segments.push(item + 1);
    }
    segments
}
