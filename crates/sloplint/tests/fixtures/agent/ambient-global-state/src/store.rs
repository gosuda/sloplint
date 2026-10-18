pub static mut COUNTER: u64 = 0;

pub fn service_home() -> String {
    std::env::var("HOME").unwrap_or_default()
}
