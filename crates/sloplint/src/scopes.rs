use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sloplint_core::text::normalize_rel_path;

use crate::error::RunError;

#[derive(Debug, Default)]
pub struct Scopes {
    paths: HashSet<PathBuf>,
}

impl Scopes {
    #[cfg(test)]
    pub(crate) fn from_relative(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            paths: paths.into_iter().collect(),
        }
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.paths.is_empty()
            || path
                .ancestors()
                .any(|ancestor| self.paths.contains(ancestor))
    }
}

pub fn scopes(root: &Path, cwd: &Path, paths: &[PathBuf]) -> Result<Scopes, RunError> {
    let mut resolved: HashSet<PathBuf> = HashSet::new();
    let mut whole_root = false;
    for path in paths {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            cwd.join(path)
        };
        let canonical = fs::canonicalize(&absolute).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                RunError::PathNotFound { path: path.clone() }
            } else {
                RunError::Io {
                    path: absolute.clone(),
                    source,
                }
            }
        })?;
        let relative = canonical
            .strip_prefix(root)
            .map_err(|_| RunError::PathOutsideRoot { path: path.clone() })?;
        let rel = normalize_rel_path(relative).ok_or_else(|| RunError::Io {
            path: path.clone(),
            source: io::Error::new(io::ErrorKind::InvalidData, "path is not UTF-8"),
        })?;
        if rel.is_empty() {
            whole_root = true;
        } else {
            resolved.insert(relative.to_path_buf());
        }
    }
    if whole_root {
        resolved.clear();
    }
    Ok(Scopes { paths: resolved })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::Scopes;

    #[test]
    fn non_utf8_skip_scope_membership_is_hermetic() {
        let valid = Path::new("src/lib.rs");
        let whole = Scopes::default();
        let src = Scopes::from_relative([PathBuf::from("src")]);
        let other = Scopes::from_relative([PathBuf::from("other")]);
        assert!(
            whole.contains(valid),
            "the whole scope contains the valid path"
        );
        assert!(src.contains(valid), "the src scope contains the valid path");
        assert!(
            !other.contains(valid),
            "the other scope excludes the valid path"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let bad = Path::new(std::ffi::OsStr::from_bytes(b"other/bad\xff.rs"));
            assert!(
                whole.contains(bad),
                "the whole scope contains the non-utf8 path"
            );
            assert!(
                !src.contains(bad),
                "the src scope excludes the non-utf8 path"
            );
            assert!(
                other.contains(bad),
                "the other scope contains the non-utf8 path"
            );
        }
        #[cfg(not(unix))]
        {
            let standin = Path::new("other/bad\u{FFFD}.rs");
            assert!(
                whole.contains(standin),
                "the whole scope contains the standin path"
            );
            assert!(
                !src.contains(standin),
                "the src scope excludes the standin path"
            );
            assert!(
                other.contains(standin),
                "the other scope contains the standin path"
            );
        }
    }

    #[test]
    fn lossy_equal_file_scope_is_hermetic() {
        let valid = Path::new("other/bad\u{FFFD}.rs");
        let link = Path::new("other/link.rs");
        let whole = Scopes::default();
        let explicit = Scopes::from_relative([valid.to_path_buf()]);
        let parent = Scopes::from_relative([PathBuf::from("other")]);
        assert!(
            whole.contains(valid),
            "the whole scope contains the valid path"
        );
        assert!(
            whole.contains(link),
            "the whole scope contains the link path"
        );
        assert!(
            explicit.contains(valid),
            "the explicit scope contains the valid path"
        );
        assert!(
            !explicit.contains(link),
            "the explicit scope excludes the link path"
        );
        assert!(
            parent.contains(valid),
            "the parent scope contains the valid path"
        );
        assert!(
            parent.contains(link),
            "the parent scope contains the link path"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let invalid = Path::new(std::ffi::OsStr::from_bytes(b"other/bad\xff.rs"));
            assert_eq!(
                invalid.to_string_lossy(),
                valid.to_string_lossy(),
                "the names are lossy-equal"
            );
            assert!(
                whole.contains(invalid),
                "the whole scope contains the lossy-equal path"
            );
            assert!(
                !explicit.contains(invalid),
                "lossy-equal names share no explicit file scope"
            );
            assert!(
                parent.contains(invalid),
                "the parent scope contains the lossy-equal path"
            );
        }
    }

    #[test]
    fn lossy_equal_dir_scope_is_hermetic() {
        let pipe = Path::new("parent/scope\u{FFFD}/pipe.rs");
        let child = Path::new("parent/scope\u{FFFD}/b.rs");
        let whole = Scopes::default();
        let explicit = Scopes::from_relative([PathBuf::from("parent/scope\u{FFFD}")]);
        let parent = Scopes::from_relative([PathBuf::from("parent")]);
        assert!(
            whole.contains(pipe),
            "the whole scope contains the pipe path"
        );
        assert!(
            explicit.contains(pipe),
            "the explicit scope contains the pipe path"
        );
        assert!(
            !explicit.contains(Path::new("parent/unrelated/b.rs")),
            "the explicit scope excludes the unrelated path"
        );
        assert!(
            parent.contains(pipe),
            "the parent scope contains the pipe path"
        );
        assert!(
            whole.contains(child),
            "the whole scope contains the child path"
        );
        assert!(
            explicit.contains(child),
            "the explicit scope contains the child path"
        );
        assert!(
            parent.contains(child),
            "the parent scope contains the child path"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let invalid_dir = Path::new(std::ffi::OsStr::from_bytes(b"parent/scope\xff"));
            assert_eq!(
                invalid_dir.to_string_lossy(),
                "parent/scope\u{FFFD}",
                "the directories are lossy-equal"
            );
            let invalid_child = Path::new(std::ffi::OsStr::from_bytes(b"parent/scope\xff/b.rs"));
            assert!(
                whole.contains(invalid_child),
                "the whole scope contains the lossy child"
            );
            assert!(
                !explicit.contains(invalid_child),
                "lossy-equal directories share no explicit scope"
            );
            assert!(
                parent.contains(invalid_child),
                "the parent scope contains the lossy child"
            );
        }
    }

    #[test]
    fn boundary_non_utf8_skip_scope_membership_is_hermetic() {
        let valid = Path::new("src/bad\u{FFFD}.rs");
        let whole = Scopes::default();
        assert!(
            whole.contains(valid),
            "the whole scope contains the valid path"
        );
        let src = Scopes::from_relative([PathBuf::from("src")]);
        assert!(src.contains(valid), "the src scope contains the valid path");
        let other = Scopes::from_relative([PathBuf::from("other")]);
        assert!(
            !other.contains(valid),
            "the other scope excludes the valid path"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let bad = Path::new(std::ffi::OsStr::from_bytes(b"src/bad\xff.rs"));
            assert!(
                whole.contains(bad),
                "the whole scope contains the non-utf8 path"
            );
            assert!(
                src.contains(bad),
                "the src scope contains the non-utf8 path"
            );
            assert!(
                !other.contains(bad),
                "the other scope excludes the non-utf8 path"
            );
            let explicit = Scopes::from_relative([valid.to_path_buf()]);
            assert!(
                explicit.contains(valid),
                "the explicit scope contains the valid path"
            );
            assert!(
                !explicit.contains(bad),
                "lossy-equal names share no explicit file scope"
            );
        }
        #[cfg(not(unix))]
        {
            let second = Path::new("src/other\u{FFFD}.rs");
            let explicit = Scopes::from_relative([valid.to_path_buf()]);
            assert!(
                !explicit.contains(second),
                "distinct replacement-character names share no explicit file scope"
            );
        }
    }
}
