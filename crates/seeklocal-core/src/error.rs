use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Folder not found: {0}")]
    FolderNotFound(PathBuf),
    #[error("Path is not a readable folder: {0}")]
    NotADirectory(PathBuf),
    #[error("File exceeds the 16 MiB limit: {0}")]
    FileTooLarge(PathBuf),
    #[error("File is not valid UTF-8 text: {0}")]
    InvalidText(PathBuf),
    #[error("Unsupported file type: {0}")]
    Unsupported(PathBuf),
    #[error("The file is not present in the index")]
    NotIndexed,
    #[error("Could not validate path {path}: {source}")]
    Path {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Could not read file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Could not access the local index: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("The operating system could not open the file: {0}")]
    Launch(std::io::Error),
    #[error("Opening files is not supported on this platform")]
    UnsupportedPlatform,
    #[error("Local meaning search failed: {0}")]
    Semantic(String),
}

pub type Result<T> = std::result::Result<T, Error>;
