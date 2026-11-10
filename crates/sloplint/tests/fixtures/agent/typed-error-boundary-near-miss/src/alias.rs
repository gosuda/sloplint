use anyhow::Result as AnyhowResult;

pub enum ApplyError {
    Missing,
    Invalid,
}

#[allow(unused_imports)]
fn _use_anyhow(x: AnyhowResult<()>) {
    let _ = x;
}

pub fn load_alias(path: &str) -> std::result::Result<String, ApplyError> {
    let _ = path;
    Err(ApplyError::Missing)
}
