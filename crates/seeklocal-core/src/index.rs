use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
};

use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{
    Error, IndexReport, IndexStats, Result, SearchResult, SemanticStatus,
    discovery::{DiscoveryFailure, discover},
    extractor::{extract, metadata},
    model::MatchType,
};

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS folders (
    path TEXT PRIMARY KEY NOT NULL,
    indexed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS documents (
    id INTEGER PRIMARY KEY,
    root TEXT NOT NULL REFERENCES folders(path) ON DELETE CASCADE,
    path TEXT UNIQUE NOT NULL,
    file_name TEXT NOT NULL,
    extension TEXT NOT NULL,
    size INTEGER NOT NULL,
    modified_ns INTEGER NOT NULL,
    fingerprint TEXT NOT NULL,
    content TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS failures (
    id INTEGER PRIMARY KEY,
    root TEXT NOT NULL REFERENCES folders(path) ON DELETE CASCADE,
    path TEXT,
    category TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
    file_name,
    path,
    content,
    content='documents',
    content_rowid='id',
    tokenize='trigram'
);
CREATE TABLE IF NOT EXISTS semantic_chunks (
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    embedding BLOB NOT NULL,
    model TEXT NOT NULL,
    PRIMARY KEY(document_id, chunk_index, model)
);
CREATE INDEX IF NOT EXISTS semantic_chunks_model_document
ON semantic_chunks(model, document_id);
CREATE TRIGGER IF NOT EXISTS documents_insert AFTER INSERT ON documents BEGIN
    INSERT INTO documents_fts(rowid, file_name, path, content)
    VALUES (new.id, new.file_name, new.path, new.content);
END;
CREATE TRIGGER IF NOT EXISTS documents_delete AFTER DELETE ON documents BEGIN
    INSERT INTO documents_fts(documents_fts, rowid, file_name, path, content)
    VALUES ('delete', old.id, old.file_name, old.path, old.content);
END;
CREATE TRIGGER IF NOT EXISTS documents_update AFTER UPDATE ON documents BEGIN
    INSERT INTO documents_fts(documents_fts, rowid, file_name, path, content)
    VALUES ('delete', old.id, old.file_name, old.path, old.content);
    INSERT INTO documents_fts(rowid, file_name, path, content)
    VALUES (new.id, new.file_name, new.path, new.content);
END;
CREATE TRIGGER IF NOT EXISTS documents_semantic_stale AFTER UPDATE OF content ON documents BEGIN
    DELETE FROM semantic_chunks WHERE document_id = new.id;
END;
PRAGMA user_version = 1;
"#;

const SEMANTIC_MODEL_REPOSITORY: &str = "Qdrant/paraphrase-multilingual-MiniLM-L12-v2-onnx-Q";
const SEMANTIC_INDEX_VERSION: &str =
    "Qdrant/paraphrase-multilingual-MiniLM-L12-v2-onnx-Q#document-v1";
const SEMANTIC_MODEL_DOWNLOAD_BYTES: u64 = 267_000_000;
const SEMANTIC_SIMILARITY_FLOOR: f32 = 0.42;

pub struct SemanticModel(TextEmbedding);

impl SemanticModel {
    pub fn load(cache_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(cache_dir).map_err(|source| Error::Io {
            path: cache_dir.to_path_buf(),
            source,
        })?;
        let inference_threads = std::thread::available_parallelism()
            .map(|count| count.get().clamp(1, 4))
            .unwrap_or(2);
        let options = TextInitOptions::new(EmbeddingModel::ParaphraseMLMiniLML12V2Q)
            .with_cache_dir(cache_dir.to_path_buf())
            .with_show_download_progress(false)
            .with_intra_threads(inference_threads);
        TextEmbedding::try_new(options)
            .map(Self)
            .map_err(|error| Error::Semantic(error.to_string()))
    }
}

pub struct SearchIndex {
    database_path: PathBuf,
}

impl SearchIndex {
    pub fn open(database_path: impl Into<PathBuf>) -> Result<Self> {
        let index = Self {
            database_path: database_path.into(),
        };
        if let Some(parent) = index.database_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let connection = index.connection()?;
        connection.execute_batch(SCHEMA)?;
        Ok(index)
    }

    fn connection(&self) -> Result<Connection> {
        let connection = Connection::open(&self.database_path)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        Ok(connection)
    }

    pub fn stats(&self) -> Result<IndexStats> {
        let connection = self.connection()?;
        Ok(IndexStats {
            folders: count(&connection, "SELECT count(*) FROM folders")?,
            documents: count(&connection, "SELECT count(*) FROM documents")?,
            failures: count(&connection, "SELECT count(*) FROM failures")?,
        })
    }

    pub fn folders(&self) -> Result<Vec<String>> {
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT path FROM folders ORDER BY path COLLATE NOCASE")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }

    pub fn index_folder(&self, requested_root: &Path) -> Result<IndexReport> {
        let discovery = discover(requested_root)?;
        let root = display_path(&discovery.root);
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO folders(path, indexed_at) VALUES (?1, CURRENT_TIMESTAMP)\n             ON CONFLICT(path) DO UPDATE SET indexed_at = CURRENT_TIMESTAMP",
            [&root],
        )?;
        transaction.execute("DELETE FROM failures WHERE root = ?1", [&root])?;
        for failure in &discovery.failures {
            record_discovery_failure(&transaction, &root, failure)?;
        }

        let discovered_paths: HashSet<String> = discovery
            .files
            .iter()
            .map(|path| display_path(path))
            .collect();
        let mut indexed = 0;
        let mut unchanged = 0;
        let mut failures = discovery.failures.len();

        for path in &discovery.files {
            let path_text = display_path(path);
            let current_metadata = metadata(path);
            let is_unchanged = match current_metadata {
                Ok((size, modified_ns)) => {
                    document_is_unchanged(&transaction, &path_text, size, modified_ns)?
                }
                Err(error) => {
                    record_failure(
                        &transaction,
                        &root,
                        Some(&path_text),
                        error_category(&error),
                    )?;
                    failures += 1;
                    continue;
                }
            };
            if is_unchanged {
                unchanged += 1;
                continue;
            }

            match extract(path) {
                Ok(document) => {
                    let file_name = path
                        .file_name()
                        .map(|value| value.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let extension = path
                        .extension()
                        .map(|value| value.to_string_lossy().to_ascii_lowercase())
                        .unwrap_or_default();
                    let size = i64::try_from(document.size).unwrap_or(i64::MAX);
                    transaction.execute(
                        "INSERT INTO documents(root, path, file_name, extension, size, modified_ns, fingerprint, content)\n                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)\n                         ON CONFLICT(path) DO UPDATE SET root=excluded.root, file_name=excluded.file_name,\n                         extension=excluded.extension, size=excluded.size, modified_ns=excluded.modified_ns,\n                         fingerprint=excluded.fingerprint, content=excluded.content",
                        params![root, path_text, file_name, extension, size, document.modified_ns, document.fingerprint, document.content],
                    )?;
                    indexed += 1;
                }
                Err(error) => {
                    record_failure(
                        &transaction,
                        &root,
                        Some(&path_text),
                        error_category(&error),
                    )?;
                    failures += 1;
                }
            }
        }

        let existing = paths_for_root(&transaction, &root)?;
        let stale: Vec<String> = existing
            .into_iter()
            .filter(|path| !discovered_paths.contains(path))
            .collect();
        for path in &stale {
            transaction.execute("DELETE FROM documents WHERE path = ?1", [path])?;
        }
        transaction.commit()?;

        Ok(IndexReport {
            root,
            discovered: discovery.files.len(),
            indexed,
            unchanged,
            removed: stale.len(),
            failures,
        })
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let normalized = query.trim();
        if normalized.is_empty() {
            return Ok(Vec::new());
        }
        let safe_limit = i64::try_from(limit.clamp(1, 100)).unwrap_or(100);
        if normalized.chars().count() < 3 {
            return self.search_short_query(normalized, safe_limit);
        }
        let Some(fts_query) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let terms = normalized_terms(query);
        let candidate_limit = safe_limit.saturating_mul(5).min(500);
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT d.path, d.file_name, d.extension,\n                    snippet(documents_fts, 2, '<mark>', '</mark>', ' ... ', 28),\n                    bm25(documents_fts, 8.0, 2.0, 1.0), d.content\n             FROM documents_fts\n             JOIN documents d ON d.id = documents_fts.rowid\n             WHERE documents_fts MATCH ?1\n             ORDER BY 5 ASC, d.path ASC\n             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![fts_query, candidate_limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut results = Vec::new();
        for row in rows {
            let (path, file_name, extension, snippet, score, content) = row?;
            if !fields_match_terms(&file_name, &path, &content, &terms) {
                continue;
            }
            let filename_match = terms.iter().all(|term| text_matches_term(&file_name, term));
            results.push(SearchResult {
                path,
                file_name,
                extension,
                snippet,
                match_type: if filename_match {
                    MatchType::Filename
                } else {
                    MatchType::Content
                },
                score,
            });
        }
        results.sort_by(|left, right| {
            let left_filename = left.match_type == MatchType::Filename;
            let right_filename = right.match_type == MatchType::Filename;
            right_filename
                .cmp(&left_filename)
                .then_with(|| left.score.total_cmp(&right.score))
                .then_with(|| left.path.cmp(&right.path))
        });
        results.truncate(usize::try_from(safe_limit).unwrap_or(100));
        Ok(results)
    }

    fn search_short_query(&self, query: &str, limit: i64) -> Result<Vec<SearchResult>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT path, file_name, extension, content,\n                    CASE WHEN instr(lower(file_name), lower(?1)) > 0 THEN 0 ELSE 1 END\n             FROM documents\n             WHERE instr(lower(file_name), lower(?1)) > 0\n                OR instr(lower(content), lower(?1)) > 0\n             ORDER BY 5 ASC, path ASC",
        )?;
        let rows = statement.query_map([query], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut results = Vec::new();
        for row in rows {
            let (path, file_name, extension, content) = row?;
            let filename_match = text_matches_term(&file_name, query);
            if !filename_match && !text_matches_term(&content, query) {
                continue;
            }
            results.push(SearchResult {
                path,
                file_name,
                extension,
                snippet: highlighted_excerpt(&content, query),
                match_type: if filename_match {
                    MatchType::Filename
                } else {
                    MatchType::Content
                },
                score: if filename_match { 0.0 } else { 1.0 },
            });
            if results.len() >= usize::try_from(limit).unwrap_or(100) {
                break;
            }
        }
        Ok(results)
    }

    pub fn semantic_status(&self, cache_dir: &Path) -> Result<SemanticStatus> {
        let connection = self.connection()?;
        let total_documents = count(
            &connection,
            "SELECT count(*) FROM documents WHERE length(trim(content)) > 0",
        )?;
        let indexed_documents: i64 = connection.query_row(
            "SELECT count(DISTINCT document_id) FROM semantic_chunks WHERE model = ?1",
            [SEMANTIC_INDEX_VERSION],
            |row| row.get(0),
        )?;
        let indexed_documents = usize::try_from(indexed_documents).unwrap_or(usize::MAX);
        let model_available = model_files_available(cache_dir);
        Ok(SemanticStatus {
            model_available,
            ready: model_available && total_documents > 0 && indexed_documents == total_documents,
            indexed_documents,
            total_documents,
            model_name: SEMANTIC_MODEL_REPOSITORY.to_owned(),
            download_bytes: SEMANTIC_MODEL_DOWNLOAD_BYTES,
        })
    }

    pub fn rebuild_semantic_index<F>(
        &self,
        model: &mut SemanticModel,
        cache_dir: &Path,
        mut on_progress: F,
    ) -> Result<SemanticStatus>
    where
        F: FnMut(usize, usize),
    {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM semantic_chunks WHERE model <> ?1",
            [SEMANTIC_INDEX_VERSION],
        )?;
        let mut statement = connection.prepare(
            "SELECT d.id, d.content
             FROM documents d
             WHERE length(trim(d.content)) > 0
               AND NOT EXISTS (
                   SELECT 1 FROM semantic_chunks s
                   WHERE s.document_id = d.id AND s.model = ?1
               )
             ORDER BY d.id",
        )?;
        let documents = statement
            .query_map([SEMANTIC_INDEX_VERSION], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        drop(connection);

        const DOCUMENT_BATCH_SIZE: usize = 64;
        let total = documents.len();
        on_progress(0, total);
        let mut completed = 0;
        for document_batch in documents.chunks(DOCUMENT_BATCH_SIZE) {
            let prepared: Vec<(i64, String)> = document_batch
                .iter()
                .map(|(document_id, content)| (*document_id, semantic_document_summary(content)))
                .collect();
            let passages: Vec<&str> = prepared
                .iter()
                .map(|(_, summary)| summary.as_str())
                .collect();
            let embeddings = model
                .0
                .embed(&passages, Some(64))
                .map_err(|error| Error::Semantic(error.to_string()))?;
            if embeddings.len() != passages.len() {
                return Err(Error::Semantic(
                    "The embedding model returned an unexpected batch size.".to_owned(),
                ));
            }
            let mut connection = self.connection()?;
            let transaction = connection.transaction()?;
            for ((document_id, summary), embedding) in prepared.iter().zip(embeddings.iter()) {
                transaction.execute(
                    "DELETE FROM semantic_chunks WHERE document_id = ?1 AND model = ?2",
                    params![document_id, SEMANTIC_INDEX_VERSION],
                )?;
                transaction.execute(
                    "INSERT INTO semantic_chunks(document_id, chunk_index, content, embedding, model)
                     VALUES (?1, 0, ?2, ?3, ?4)",
                    params![
                        document_id,
                        summary,
                        embedding_to_bytes(embedding),
                        SEMANTIC_INDEX_VERSION
                    ],
                )?;
            }
            transaction.commit()?;
            completed += prepared.len();
            on_progress(completed, total);
        }

        self.semantic_status(cache_dir)
    }

    pub fn hybrid_search(
        &self,
        query: &str,
        limit: usize,
        model: &mut SemanticModel,
    ) -> Result<Vec<SearchResult>> {
        let normalized = query.trim();
        if normalized.is_empty() {
            return Ok(Vec::new());
        }
        let candidate_limit = limit.clamp(1, 100).saturating_mul(4).min(100);
        let lexical = self.search(normalized, candidate_limit)?;
        let semantic = self.semantic_search(normalized, candidate_limit, model)?;
        let mut fused: HashMap<String, (SearchResult, f64, bool, bool)> = HashMap::new();

        for (rank, result) in lexical.into_iter().enumerate() {
            let rank_score = 0.58 / (60.0 + rank as f64);
            let filename_bonus = if result.match_type == MatchType::Filename {
                0.003
            } else {
                0.0
            };
            fused.insert(
                result.path.clone(),
                (result, rank_score + filename_bonus, true, false),
            );
        }
        for (rank, result) in semantic.into_iter().enumerate() {
            let rank_score = 0.42 / (60.0 + rank as f64);
            fused
                .entry(result.path.clone())
                .and_modify(|entry| {
                    entry.1 += rank_score;
                    entry.3 = true;
                })
                .or_insert((result, rank_score, false, true));
        }

        let mut results: Vec<(SearchResult, f64, bool, bool)> = fused.into_values().collect();
        results.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.path.cmp(&right.0.path))
        });
        results.truncate(limit.clamp(1, 100));
        Ok(results
            .into_iter()
            .map(|(mut result, score, lexical_match, semantic_match)| {
                result.score = score;
                result.match_type = match (lexical_match, semantic_match, &result.match_type) {
                    (true, true, _) => MatchType::Hybrid,
                    (true, false, MatchType::Filename) => MatchType::Filename,
                    (true, false, _) => MatchType::Content,
                    (false, true, _) => MatchType::Semantic,
                    _ => result.match_type,
                };
                result
            })
            .collect())
    }

    fn semantic_search(
        &self,
        query: &str,
        limit: usize,
        model: &mut SemanticModel,
    ) -> Result<Vec<SearchResult>> {
        let query_input = [query.to_owned()];
        let query_embedding = model
            .0
            .embed(&query_input, Some(1))
            .map_err(|error| Error::Semantic(error.to_string()))?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Semantic("The model returned no query embedding.".to_owned()))?;
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT d.path, d.file_name, d.extension, s.content, s.embedding
             FROM semantic_chunks s
             JOIN documents d ON d.id = s.document_id
             WHERE s.model = ?1
             ORDER BY d.path, s.chunk_index",
        )?;
        let rows = statement.query_map([SEMANTIC_INDEX_VERSION], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })?;
        let mut by_document: HashMap<String, SearchResult> = HashMap::new();
        for row in rows {
            let (path, file_name, extension, content, bytes) = row?;
            let Some(embedding) = embedding_from_bytes(&bytes) else {
                continue;
            };
            let similarity = cosine_similarity(&query_embedding, &embedding);
            if similarity < SEMANTIC_SIMILARITY_FLOOR {
                continue;
            }
            let candidate = SearchResult {
                path: path.clone(),
                file_name,
                extension,
                snippet: semantic_excerpt(&content),
                match_type: MatchType::Semantic,
                score: f64::from(similarity),
            };
            by_document
                .entry(path)
                .and_modify(|existing| {
                    if candidate.score > existing.score {
                        *existing = candidate.clone();
                    }
                })
                .or_insert(candidate);
        }
        let mut results: Vec<SearchResult> = by_document.into_values().collect();
        results.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.path.cmp(&right.path))
        });
        results.truncate(limit);
        Ok(results)
    }

    pub fn remove_folder(&self, requested_root: &Path) -> Result<()> {
        let root = requested_root
            .canonicalize()
            .unwrap_or_else(|_| requested_root.to_path_buf());
        self.connection()?
            .execute("DELETE FROM folders WHERE path = ?1", [display_path(&root)])?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        let connection = self.connection()?;
        connection.execute("DELETE FROM folders", [])?;
        connection.execute("DELETE FROM failures", [])?;
        Ok(())
    }

    pub fn open_source(&self, requested_path: &Path, reveal: bool) -> Result<()> {
        let canonical = requested_path
            .canonicalize()
            .map_err(|source| Error::Path {
                path: requested_path.to_path_buf(),
                source,
            })?;
        let path = display_path(&canonical);
        let indexed: bool = self
            .connection()?
            .query_row("SELECT 1 FROM documents WHERE path = ?1", [&path], |_| {
                Ok(true)
            })
            .optional()?
            .unwrap_or(false);
        if !indexed {
            return Err(Error::NotIndexed);
        }
        launch_path(&canonical, reveal)
    }
}

fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn embedding_from_bytes(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let (dot, left_norm, right_norm) =
        left.iter()
            .zip(right)
            .fold((0.0_f32, 0.0_f32, 0.0_f32), |acc, (left, right)| {
                (
                    acc.0 + left * right,
                    acc.1 + left * left,
                    acc.2 + right * right,
                )
            });
    if left_norm <= f32::EPSILON || right_norm <= f32::EPSILON {
        0.0
    } else {
        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}

fn semantic_excerpt(content: &str) -> String {
    let mut excerpt: String = content.chars().take(280).collect();
    if content.chars().count() > 280 {
        excerpt.push_str(" ...");
    }
    excerpt
}

fn semantic_document_summary(content: &str) -> String {
    const SAMPLE_SIZE: usize = 240;
    let characters: Vec<char> = content.chars().collect();
    if characters.len() <= SAMPLE_SIZE * 3 {
        return content.to_owned();
    }
    let middle_start = characters.len() / 2 - SAMPLE_SIZE / 2;
    let tail_start = characters.len() - SAMPLE_SIZE;
    let head: String = characters[..SAMPLE_SIZE].iter().collect();
    let middle: String = characters[middle_start..middle_start + SAMPLE_SIZE]
        .iter()
        .collect();
    let tail: String = characters[tail_start..].iter().collect();
    format!("{head}\n…\n{middle}\n…\n{tail}")
}

fn model_files_available(cache_dir: &Path) -> bool {
    cache_dir.exists()
        && walkdir::WalkDir::new(cache_dir)
            .max_depth(8)
            .follow_links(false)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .any(|entry| {
                (entry.file_type().is_file() || entry.file_type().is_symlink())
                    && entry.file_name() == "model_optimized.onnx"
                    && std::fs::metadata(entry.path())
                        .map(|metadata| metadata.len() > 1_000_000)
                        .unwrap_or(false)
            })
}

fn count(connection: &Connection, sql: &str) -> Result<usize> {
    let value: i64 = connection.query_row(sql, [], |row| row.get(0))?;
    Ok(usize::try_from(value).unwrap_or(usize::MAX))
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn document_is_unchanged(
    transaction: &Transaction<'_>,
    path: &str,
    size: u64,
    modified_ns: i64,
) -> Result<bool> {
    let size = i64::try_from(size).unwrap_or(i64::MAX);
    Ok(transaction
        .query_row(
            "SELECT 1 FROM documents WHERE path = ?1 AND size = ?2 AND modified_ns = ?3",
            params![path, size, modified_ns],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false))
}

fn paths_for_root(transaction: &Transaction<'_>, root: &str) -> Result<Vec<String>> {
    let mut statement = transaction.prepare("SELECT path FROM documents WHERE root = ?1")?;
    let rows = statement.query_map([root], |row| row.get(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Error::from)
}

fn record_discovery_failure(
    transaction: &Transaction<'_>,
    root: &str,
    failure: &DiscoveryFailure,
) -> Result<()> {
    let path = failure.path.as_deref().map(display_path);
    record_failure(transaction, root, path.as_deref(), failure.category)
}

fn record_failure(
    transaction: &Transaction<'_>,
    root: &str,
    path: Option<&str>,
    category: &str,
) -> Result<()> {
    transaction.execute(
        "INSERT INTO failures(root, path, category) VALUES (?1, ?2, ?3)",
        params![root, path, category],
    )?;
    Ok(())
}

fn error_category(error: &Error) -> &'static str {
    match error {
        Error::FileTooLarge(_) => "too_large",
        Error::InvalidText(_) => "invalid_text",
        Error::Unsupported(_) => "unsupported",
        Error::Io { .. } => "read_failed",
        _ => "unknown",
    }
}

fn fts_query(query: &str) -> Option<String> {
    let terms = normalized_terms(query);
    (!terms.is_empty()).then(|| {
        terms
            .into_iter()
            .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ")
    })
}

fn normalized_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .filter_map(|term| {
            let cleaned: String = term
                .chars()
                .filter(|character| {
                    character.is_alphanumeric() || *character == '_' || *character == '-'
                })
                .collect();
            (!cleaned.is_empty()).then_some(cleaned)
        })
        .collect()
}

fn fields_match_terms(file_name: &str, path: &str, content: &str, terms: &[String]) -> bool {
    terms.iter().all(|term| {
        text_matches_term(file_name, term)
            || text_matches_term(path, term)
            || text_matches_term(content, term)
    })
}

fn text_matches_term(text: &str, term: &str) -> bool {
    if term.is_empty() {
        return false;
    }
    if !term.is_ascii() {
        return text.to_lowercase().contains(&term.to_lowercase());
    }

    let lowered_text = text.to_ascii_lowercase();
    let lowered_term = term.to_ascii_lowercase();
    lowered_text.match_indices(&lowered_term).any(|(start, _)| {
        start == 0
            || text[..start]
                .chars()
                .next_back()
                .is_none_or(|character| !character.is_ascii_alphanumeric())
    })
}

fn highlighted_excerpt(content: &str, query: &str) -> String {
    let Some(byte_start) = content.find(query).or_else(|| {
        content
            .to_ascii_lowercase()
            .find(&query.to_ascii_lowercase())
    }) else {
        return content.chars().take(120).collect();
    };
    let start_character = content[..byte_start].chars().count();
    let query_characters = query.chars().count();
    let excerpt_start = start_character.saturating_sub(36);
    let characters: Vec<char> = content.chars().collect();
    let match_end = (start_character + query_characters).min(characters.len());
    let excerpt_end = (match_end + 72).min(characters.len());
    let before: String = characters[excerpt_start..start_character].iter().collect();
    let matched: String = characters[start_character..match_end].iter().collect();
    let after: String = characters[match_end..excerpt_end].iter().collect();
    format!(
        "{}{}<mark>{}</mark>{}{}",
        if excerpt_start > 0 { "... " } else { "" },
        before,
        matched,
        after,
        if excerpt_end < characters.len() {
            " ..."
        } else {
            ""
        }
    )
}

#[cfg(target_os = "windows")]
fn launch_path(path: &Path, reveal: bool) -> Result<()> {
    let mut command = Command::new("explorer.exe");
    if reveal {
        command.arg(format!("/select,{}", path.display()));
    } else {
        command.arg(path);
    }
    command.spawn().map(|_| ()).map_err(Error::Launch)
}

#[cfg(target_os = "macos")]
fn launch_path(path: &Path, reveal: bool) -> Result<()> {
    let mut command = Command::new("open");
    if reveal {
        command.arg("-R");
    }
    command.arg(path).spawn().map(|_| ()).map_err(Error::Launch)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn launch_path(path: &Path, reveal: bool) -> Result<()> {
    let target = if reveal {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(Error::Launch)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
fn launch_path(_path: &Path, _reveal: bool) -> Result<()> {
    Err(Error::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        SearchIndex, cosine_similarity, embedding_from_bytes, embedding_to_bytes, fts_query,
        model_files_available, semantic_document_summary,
    };

    #[test]
    fn indexes_searches_and_removes_without_touching_sources() {
        let directory = tempdir().expect("temporary directory");
        let source = directory.path().join("source");
        fs::create_dir(&source).expect("source directory");
        let japanese = source.join("企画書.md");
        let english = source.join("privacy-notes.txt");
        fs::write(&japanese, "地域の図書館について調査する").expect("fixture");
        fs::write(&english, "All search remains private and offline.").expect("fixture");
        let index = SearchIndex::open(directory.path().join("index.db")).expect("index opens");

        let report = index.index_folder(&source).expect("indexing succeeds");
        assert_eq!(report.indexed, 2);
        assert_eq!(index.search("図書館", 10).expect("search").len(), 1);
        assert_eq!(index.search("地域", 10).expect("short search").len(), 1);
        assert_eq!(index.search("priv", 10).expect("prefix search").len(), 1);

        index.remove_folder(&source).expect("remove succeeds");
        assert!(japanese.exists());
        assert!(english.exists());
        assert_eq!(index.stats().expect("stats").documents, 0);
    }

    #[test]
    fn updates_and_deletes_incrementally() {
        let directory = tempdir().expect("temporary directory");
        let source = directory.path().join("source");
        fs::create_dir(&source).expect("source directory");
        let file = source.join("notes.txt");
        fs::write(&file, "first version").expect("fixture");
        let index = SearchIndex::open(directory.path().join("index.db")).expect("index opens");
        index.index_folder(&source).expect("initial index");
        let unchanged = index.index_folder(&source).expect("repeat index");
        assert_eq!(unchanged.unchanged, 1);

        fs::remove_file(&file).expect("remove source fixture");
        let removed = index.index_folder(&source).expect("reconcile index");
        assert_eq!(removed.removed, 1);
        assert_eq!(index.stats().expect("stats").documents, 0);
    }

    #[test]
    fn turns_user_input_into_safe_prefix_query() {
        assert_eq!(
            fts_query("hello OR 日本語"),
            Some("\"hello\"* AND \"OR\"* AND \"日本語\"*".into())
        );
        assert_eq!(fts_query("***"), None);
    }

    #[test]
    fn ascii_terms_match_word_starts_but_not_internal_trigrams() {
        let directory = tempdir().expect("temporary directory");
        let source = directory.path().join("source");
        fs::create_dir(&source).expect("source directory");
        fs::write(
            source.join("syntax.md"),
            "Programming language syntax notes.",
        )
        .expect("syntax fixture");
        fs::write(
            source.join("vehicle-expenses.md"),
            "Taxation and tax invoices for a vehicle.",
        )
        .expect("tax fixture");
        let index = SearchIndex::open(directory.path().join("index.db")).expect("index opens");
        index.index_folder(&source).expect("indexing succeeds");

        let results = index.search("tax", 10).expect("search succeeds");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_name, "vehicle-expenses.md");
    }

    #[test]
    fn semantic_summary_samples_the_whole_document() {
        let content = format!("{}MIDDLE{}ENDING", "A".repeat(500), "B".repeat(500));
        let summary = semantic_document_summary(&content);
        assert!(summary.starts_with('A'));
        assert!(summary.contains("MIDDLE"));
        assert!(summary.ends_with("ENDING"));
        assert!(summary.chars().count() < content.chars().count());
    }

    #[cfg(unix)]
    #[test]
    fn detects_a_cached_model_through_a_symlink() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().expect("temporary directory");
        let blob = directory.path().join("model-blob");
        fs::write(&blob, vec![0_u8; 1_000_001]).expect("model fixture");
        symlink(&blob, directory.path().join("model_optimized.onnx")).expect("model cache symlink");

        assert!(model_files_available(directory.path()));
    }

    #[test]
    fn embedding_storage_round_trips_and_cosine_is_stable() {
        let embedding = vec![0.2, -0.4, 0.8];
        let stored = embedding_to_bytes(&embedding);
        let restored = embedding_from_bytes(&stored).expect("valid embedding bytes");
        assert_eq!(embedding, restored);
        assert!((cosine_similarity(&embedding, &restored) - 1.0).abs() < 0.0001);
        assert_eq!(cosine_similarity(&embedding, &[1.0]), 0.0);
    }
}
