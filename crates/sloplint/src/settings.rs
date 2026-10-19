//! Settings taken from the process environment, read in one place so the rest of the pipeline receives them as plain values.

use std::path::PathBuf;

/// The user's cache directory: an absolute `XDG_CACHE_HOME` on every platform, else the platform default.
///
/// A relative `XDG_CACHE_HOME` is invalid per the XDG base directory
/// specification and is ignored, so the cache never lands relative to the
/// working directory (which may be the checkout). `None` means no per-user
/// directory is available.
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
