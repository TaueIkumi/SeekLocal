use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

use crate::{Error, Result};

const EXCLUDED_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    ".idea",
    ".vscode",
    "$RECYCLE.BIN",
    "System Volume Information",
];

#[derive(Debug)]
pub(crate) struct Discovery {
    pub root: PathBuf,
    pub files: Vec<PathBuf>,
    pub failures: Vec<DiscoveryFailure>,
}

#[derive(Debug)]
pub(crate) struct DiscoveryFailure {
    pub path: Option<PathBuf>,
    pub category: &'static str,
}

fn should_visit(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() || entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    !EXCLUDED_DIRECTORIES
        .iter()
        .any(|excluded| name.eq_ignore_ascii_case(excluded))
}

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "txt" | "md" | "markdown"
            )
        })
}

pub(crate) fn discover(root: &Path) -> Result<Discovery> {
    if !root.exists() {
        return Err(Error::FolderNotFound(root.to_path_buf()));
    }
    if !root.is_dir() {
        return Err(Error::NotADirectory(root.to_path_buf()));
    }
    let canonical_root = root.canonicalize().map_err(|source| Error::Path {
        path: root.to_path_buf(),
        source,
    })?;
    let mut files = Vec::new();
    let mut failures = Vec::new();

    for entry in WalkDir::new(&canonical_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(should_visit)
    {
        match entry {
            Ok(entry) if entry.file_type().is_file() && is_supported(entry.path()) => {
                files.push(entry.into_path());
            }
            Ok(_) => {}
            Err(error) => failures.push(DiscoveryFailure {
                path: error.path().map(Path::to_path_buf),
                category: "discovery",
            }),
        }
    }
    files.sort();
    Ok(Discovery {
        root: canonical_root,
        files,
        failures,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::discover;

    #[test]
    fn ignores_dependencies_and_symlink_loops() {
        let directory = tempdir().expect("temporary directory");
        fs::write(directory.path().join("visible.md"), "ok").expect("fixture");
        fs::create_dir(directory.path().join("node_modules")).expect("fixture directory");
        fs::write(directory.path().join("node_modules/hidden.txt"), "no").expect("fixture");
        #[cfg(unix)]
        std::os::unix::fs::symlink(directory.path(), directory.path().join("loop"))
            .expect("symlink");

        let result = discover(directory.path()).expect("discovery succeeds");
        assert_eq!(result.files.len(), 1);
        assert!(result.files[0].ends_with("visible.md"));
    }
}
