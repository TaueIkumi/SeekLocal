use std::{fs, path::Path, time::UNIX_EPOCH};

use crate::{Error, Result};

const MAX_FILE_SIZE: u64 = 16 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct ExtractedDocument {
    pub content: String,
    pub size: u64,
    pub modified_ns: i64,
    pub fingerprint: String,
}

pub(crate) fn metadata(path: &Path) -> Result<(u64, i64)> {
    let metadata = fs::metadata(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_nanos()).ok())
        .unwrap_or(0);
    Ok((metadata.len(), modified_ns))
}

pub(crate) fn extract(path: &Path) -> Result<ExtractedDocument> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| Error::Unsupported(path.to_path_buf()))?;
    if !matches!(extension.as_str(), "txt" | "md" | "markdown") {
        return Err(Error::Unsupported(path.to_path_buf()));
    }
    let (size, modified_ns) = metadata(path)?;
    if size > MAX_FILE_SIZE {
        return Err(Error::FileTooLarge(path.to_path_buf()));
    }
    let bytes = fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let fingerprint = blake3::hash(&bytes).to_hex().to_string();
    let content = String::from_utf8(bytes).map_err(|_| Error::InvalidText(path.to_path_buf()))?;
    Ok(ExtractedDocument {
        content,
        size,
        modified_ns,
        fingerprint,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::extract;

    #[test]
    fn extracts_japanese_unicode_and_emoji() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join("議事録📝.md");
        fs::write(&path, "# 会議\nprivacy first 🔐").expect("fixture");

        let result = extract(&path).expect("extract succeeds");
        assert!(result.content.contains("会議"));
        assert!(result.content.contains('🔐'));
        assert!(!result.fingerprint.is_empty());
    }
}
