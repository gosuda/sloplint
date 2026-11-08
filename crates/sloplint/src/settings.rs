use std::path::PathBuf;

/// Ignores a relative `XDG_CACHE_HOME` (invalid per the XDG spec) so the cache never lands inside the checkout.
pub fn cache_base() -> Option<PathBuf> {
    absolute_var("XDG_CACHE_HOME").or_else(platform_cache_base)
}

fn absolute_var(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os(name)?);
    path.is_absolute().then_some(path)
}

#[cfg(windows)]
fn platform_cache_base() -> Option<PathBuf> {
    absolute_var("LOCALAPPDATA")
}

#[cfg(target_os = "macos")]
fn platform_cache_base() -> Option<PathBuf> {
    absolute_var("HOME").map(|home| home.join("Library").join("Caches"))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform_cache_base() -> Option<PathBuf> {
    absolute_var("HOME").map(|home| home.join(".cache"))
}
