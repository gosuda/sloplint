pub enum LoadError {
    Missing,
    Invalid,
}

pub fn load_typed(path: &str) -> Result<String, LoadError> {
    std::fs::read_to_string(path).map_err(|_| LoadError::Missing)
}

fn main() {
    let _ = load_typed("app.toml");
}
