export type IndexStats = {
  folders: number;
  documents: number;
  failures: number;
};

export type IndexReport = {
  root: string;
  discovered: number;
  indexed: number;
  unchanged: number;
  removed: number;
  failures: number;
};

export type SemanticStatus = {
  modelAvailable: boolean;
  ready: boolean;
  indexedDocuments: number;
  totalDocuments: number;
  modelName: string;
  downloadBytes: number;
};

export type SemanticProgress = {
  phase: "download" | "index";
  completed: number;
  total: number;
};

export type SearchResult = {
  path: string;
  fileName: string;
  extension: string;
  snippet: string;
  matchType: "filename" | "content" | "semantic" | "hybrid";
  score: number;
};
