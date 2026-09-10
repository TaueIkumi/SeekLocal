use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexReport {
    pub root: String,
    pub discovered: usize,
    pub indexed: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub failures: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub folders: usize,
    pub documents: usize,
    pub failures: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SemanticStatus {
    pub model_available: bool,
    pub ready: bool,
    pub indexed_documents: usize,
    pub total_documents: usize,
    pub model_name: String,
    pub download_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub path: String,
    pub file_name: String,
    pub extension: String,
    pub snippet: String,
    pub match_type: MatchType,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MatchType {
    Filename,
    Content,
    Semantic,
    Hybrid,
}
