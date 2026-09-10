import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { IndexReport, IndexStats, SearchResult, SemanticStatus } from "./types";

export async function chooseFolder(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false, title: "Choose a folder to index" });
  return typeof selected === "string" ? selected : null;
}

export const getStats = () => invoke<IndexStats>("get_stats");
export const getFolders = () => invoke<string[]>("get_folders");
export const indexFolder = (path: string) => invoke<IndexReport>("index_folder", { path });
export const searchDocuments = (query: string, semantic = false) =>
  invoke<SearchResult[]>("search_documents", { query, limit: 50, semantic });
export const getSemanticStatus = () => invoke<SemanticStatus>("get_semantic_status");
export const prepareSemanticSearch = () => invoke<SemanticStatus>("prepare_semantic_search");
export const buildSemanticIndex = () => invoke<SemanticStatus>("build_semantic_index");
export const openResult = (path: string) => invoke<void>("open_result", { path });
export const revealResult = (path: string) => invoke<void>("reveal_result", { path });
export const removeFolder = (path: string) => invoke<void>("remove_folder", { path });
export const deleteIndex = () => invoke<void>("delete_index");
