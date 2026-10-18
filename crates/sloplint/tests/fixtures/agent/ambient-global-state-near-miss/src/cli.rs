pub static LIMIT: u64 = 100;

fn main() {
    let home = std::env::var("HOME").unwrap_or_default();
    let _ = (home, LIMIT);
}
