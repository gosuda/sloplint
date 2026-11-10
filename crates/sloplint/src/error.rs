use std::io;
use std::path::{Path, PathBuf};

use sloplint_core::ConfigError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("path is outside the project root: {}", path.display())]
    PathOutsideRoot { path: PathBuf },
    #[error("path not found: {}", path.display())]
    PathNotFound { path: PathBuf },
    #[error("baseline file not found: {}; run 'sloplint baseline'", path.display())]
    BaselineMissing { path: PathBuf },
    #[error("baseline path is outside the project root: {}", path.display())]
    BaselineOutsideRoot { path: PathBuf },
    #[error("baseline {}: {message}", path.display())]
    BaselineCorrupt { path: PathBuf, message: String },
    #[error("{} exists and is not a sloplint baseline; refusing to overwrite it (delete the file to record a new baseline)", path.display())]
    NotABaseline { path: PathBuf },
    #[error("{} already exists", path.display())]
    ConfigExists { path: PathBuf },
}

impl RunError {
    pub fn exit_code(&self) -> u8 {
        match self {
            RunError::Io { .. } => 3,
            _ => 2,
        }
    }
}

pub fn io_error(path: &Path, source: io::Error) -> RunError {
    RunError::Io {
        path: path.to_path_buf(),
        source,
    }
}
