pub fn service_home() -> String {
    std::env::var("HOME").unwrap_or_default()
}
