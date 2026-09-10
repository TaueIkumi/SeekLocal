use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use seeklocal_core::{
    IndexReport, IndexStats, SearchIndex, SearchResult, SemanticModel, SemanticStatus,
};
use serde::Serialize;
use tauri::{Emitter, Manager};

#[derive(Clone)]
struct AppState {
    database_path: PathBuf,
    semantic_cache_path: PathBuf,
    semantic_model: Arc<Mutex<Option<SemanticModel>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SemanticProgress {
    phase: &'static str,
    completed: usize,
    total: usize,
}

fn index(state: &AppState) -> Result<SearchIndex, String> {
    SearchIndex::open(&state.database_path).map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_stats(state: tauri::State<'_, AppState>) -> Result<IndexStats, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        index(&state)?.stats().map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The indexing process stopped unexpectedly.".to_owned())?
}

#[tauri::command]
async fn get_folders(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        index(&state)?.folders().map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The indexing process stopped unexpectedly.".to_owned())?
}

#[tauri::command]
async fn index_folder(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<IndexReport, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        index(&state)?
            .index_folder(Path::new(&path))
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The indexing process stopped unexpectedly.".to_owned())?
}

#[tauri::command]
async fn search_documents(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: usize,
    semantic: bool,
) -> Result<Vec<SearchResult>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let index = index(&state)?;
        if !semantic {
            return index
                .search(&query, limit)
                .map_err(|error| error.to_string());
        }
        let status = index
            .semantic_status(&state.semantic_cache_path)
            .map_err(|error| error.to_string())?;
        if !status.ready {
            return Err(
                "Meaning search is not ready. Prepare or update its local index first.".to_owned(),
            );
        }
        let mut guard = state
            .semantic_model
            .lock()
            .map_err(|_| "The local meaning model is unavailable.".to_owned())?;
        if guard.is_none() {
            *guard = Some(
                SemanticModel::load(&state.semantic_cache_path)
                    .map_err(|error| error.to_string())?,
            );
        }
        index
            .hybrid_search(
                &query,
                limit,
                guard
                    .as_mut()
                    .ok_or_else(|| "The local meaning model is unavailable.".to_owned())?,
            )
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The search process stopped unexpectedly.".to_owned())?
}

#[tauri::command]
async fn get_semantic_status(state: tauri::State<'_, AppState>) -> Result<SemanticStatus, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        index(&state)?
            .semantic_status(&state.semantic_cache_path)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "Could not inspect the local meaning index.".to_owned())?
}

#[tauri::command]
async fn prepare_semantic_search(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<SemanticStatus, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let index = index(&state)?;
        let mut guard = state
            .semantic_model
            .lock()
            .map_err(|_| "The local meaning model is unavailable.".to_owned())?;
        if guard.is_none() {
            let _ = app.emit(
                "semantic-progress",
                SemanticProgress {
                    phase: "download",
                    completed: 0,
                    total: 0,
                },
            );
            *guard = Some(
                SemanticModel::load(&state.semantic_cache_path)
                    .map_err(|error| error.to_string())?,
            );
        }
        index
            .semantic_status(&state.semantic_cache_path)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "Meaning model download stopped unexpectedly.".to_owned())?
}

#[tauri::command]
async fn build_semantic_index(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<SemanticStatus, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let index = index(&state)?;
        let mut guard = state
            .semantic_model
            .lock()
            .map_err(|_| "The local meaning model is unavailable.".to_owned())?;
        if guard.is_none() {
            *guard = Some(
                SemanticModel::load(&state.semantic_cache_path)
                    .map_err(|error| error.to_string())?,
            );
        }
        let progress_app = app.clone();
        index
            .rebuild_semantic_index(
                guard
                    .as_mut()
                    .ok_or_else(|| "The local meaning model is unavailable.".to_owned())?,
                &state.semantic_cache_path,
                move |completed, total| {
                    let _ = progress_app.emit(
                        "semantic-progress",
                        SemanticProgress {
                            phase: "index",
                            completed,
                            total,
                        },
                    );
                },
            )
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "Meaning search setup stopped unexpectedly.".to_owned())?
}

#[tauri::command]
async fn remove_folder(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        index(&state)?
            .remove_folder(Path::new(&path))
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The indexing process stopped unexpectedly.".to_owned())?
}

#[tauri::command]
async fn delete_index(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        index(&state)?.clear().map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The indexing process stopped unexpectedly.".to_owned())?
}

#[tauri::command]
async fn open_result(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    open_indexed_path(state.inner().clone(), path, false).await
}

#[tauri::command]
async fn reveal_result(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    open_indexed_path(state.inner().clone(), path, true).await
}

async fn open_indexed_path(state: AppState, path: String, reveal: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        index(&state)?
            .open_source(Path::new(&path), reveal)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The file operation stopped unexpectedly.".to_owned())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            app.manage(AppState {
                database_path: data_dir.join("seeklocal.db"),
                semantic_cache_path: data_dir.join("models"),
                semantic_model: Arc::new(Mutex::new(None)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_stats,
            get_folders,
            index_folder,
            search_documents,
            get_semantic_status,
            prepare_semantic_search,
            build_semantic_index,
            remove_folder,
            delete_index,
            open_result,
            reveal_result,
        ])
        .run(tauri::generate_context!())
}
