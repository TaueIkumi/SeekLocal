import { FormEvent, useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  buildSemanticIndex, chooseFolder, deleteIndex, getFolders, getSemanticStatus, getStats, indexFolder, openResult,
  prepareSemanticSearch, removeFolder, revealResult, searchDocuments,
} from "./api";
import type { IndexReport, IndexStats, SearchResult, SemanticProgress, SemanticStatus } from "./types";

const EMPTY_STATS: IndexStats = { folders: 0, documents: 0, failures: 0 };
type SearchFilter = "all" | "filename" | "markdown" | "semantic";

const SEARCH_FILTERS: { id: SearchFilter; label: string }[] = [
  { id: "all", label: "All files" },
  { id: "filename", label: "Filename" },
  { id: "markdown", label: "Markdown" },
  { id: "semantic", label: "Meaning search" },
];

function basename(path: string) {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

function reasonFor(result: SearchResult) {
  if (result.matchType === "semantic") return "The meaning of this passage is close to your search, even without the same words.";
  if (result.matchType === "hybrid") return "Both the wording and the overall meaning are strongly related to your search.";
  if (result.matchType === "filename") return "Your search phrase appears in the filename.";
  return "Your search phrase appears in the document body.";
}

function SearchIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m15.5 15.5 5 5" /></svg>;
}

function FolderIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7.5h7l2-2h3.5A2.5 2.5 0 0 1 18 8v.5h1A2 2 0 0 1 21 10.5v7A2.5 2.5 0 0 1 18.5 20h-13A2.5 2.5 0 0 1 3 17.5v-10Z" /></svg>;
}

function HighlightedSnippet({ value }: { value: string }) {
  const parts = value.split(/(<mark>|<\/mark>)/);
  let highlighted = false;
  return parts.map((part, index) => {
    if (part === "<mark>") { highlighted = true; return null; }
    if (part === "</mark>") { highlighted = false; return null; }
    return highlighted ? <mark key={index}>{part}</mark> : <span key={index}>{part}</span>;
  });
}

function App() {
  const composingQuery = useRef(false);
  const suppressCompositionSubmit = useRef(false);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [folders, setFolders] = useState<string[]>([]);
  const [stats, setStats] = useState<IndexStats>(EMPTY_STATS);
  const [busy, setBusy] = useState(false);
  const [searched, setSearched] = useState(false);
  const [activeFilter, setActiveFilter] = useState<SearchFilter>("all");
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [semanticStatus, setSemanticStatus] = useState<SemanticStatus | null>(null);
  const [semanticBusy, setSemanticBusy] = useState(false);
  const [semanticProgress, setSemanticProgress] = useState<SemanticProgress | null>(null);
  const [notice, setNotice] = useState("Everything stays on this device.");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [nextStats, nextFolders, nextSemanticStatus] = await Promise.all([getStats(), getFolders(), getSemanticStatus()]);
    setStats(nextStats);
    setFolders(nextFolders);
    setSemanticStatus(nextSemanticStatus);
  }, []);

  useEffect(() => {
    let cancelled = false;
    Promise.all([getStats(), getFolders(), getSemanticStatus()])
      .then(([nextStats, nextFolders, nextSemanticStatus]) => {
        if (!cancelled) { setStats(nextStats); setFolders(nextFolders); setSemanticStatus(nextSemanticStatus); }
      })
      .catch((reason: unknown) => { if (!cancelled) setError(String(reason)); });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    let disposed = false;
    let stopListening: (() => void) | undefined;
    listen<SemanticProgress>("semantic-progress", (event) => {
      if (!disposed) setSemanticProgress(event.payload);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stopListening = unlisten;
    }).catch((reason: unknown) => {
      if (!disposed) setError(String(reason));
    });
    return () => {
      disposed = true;
      stopListening?.();
    };
  }, []);

  async function addFolder() {
    const path = await chooseFolder();
    if (!path) return;
    setBusy(true); setError(null); setNotice(`Indexing ${basename(path)}...`);
    try {
      const report: IndexReport = await indexFolder(path);
      setNotice(`Indexed ${report.indexed}; ${report.unchanged} unchanged; ${report.failures} skipped.`);
      await refresh();
    } catch (reason) { setError(String(reason)); } finally { setBusy(false); }
  }

  async function runSearch(normalized: string, filter: SearchFilter) {
    if (!normalized) return;
    setBusy(true); setError(null);
    try {
      const useSemanticSearch = filter === "semantic" && Boolean(semanticStatus?.ready);
      let nextResults = await searchDocuments(normalized, useSemanticSearch);
      if (filter === "filename") nextResults = nextResults.filter((result) => result.matchType === "filename");
      if (filter === "markdown") nextResults = nextResults.filter((result) => ["md", "markdown"].includes(result.extension.toLocaleLowerCase()));
      setResults(nextResults);
      setSelectedPath(nextResults[0]?.path ?? null);
      setSearched(true);
      if (filter === "semantic" && !useSemanticSearch) {
        setNotice("Meaning index is not ready yet, so these are regular text matches.");
      }
    }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  async function submitSearch(event: FormEvent) {
    event.preventDefault();
    if (composingQuery.current || suppressCompositionSubmit.current) {
      suppressCompositionSubmit.current = false;
      return;
    }
    await runSearch(query.trim(), activeFilter);
  }

  async function chooseFilter(filter: SearchFilter) {
    setActiveFilter(filter);
    if (searched && query.trim()) await runSearch(query.trim(), filter);
  }

  async function downloadSemanticModel() {
    setSemanticBusy(true); setError(null);
    setSemanticProgress({ phase: "download", completed: 0, total: 0 });
    setNotice("Downloading the meaning model once...");
    try {
      const status = await prepareSemanticSearch();
      setSemanticStatus(status);
      setNotice("Meaning model downloaded. You can now choose folders, then build their meaning index.");
    } catch (reason) { setError(String(reason)); }
    finally { setSemanticBusy(false); setSemanticProgress(null); }
  }

  async function updateSemanticIndex() {
    setSemanticBusy(true); setError(null);
    setSemanticProgress({ phase: "index", completed: 0, total: 0 });
    setNotice("Building the meaning index for the current folders...");
    try {
      const status = await buildSemanticIndex();
      setSemanticStatus(status);
      setNotice(`Meaning search is ready for ${status.indexedDocuments} documents.`);
      if (query.trim()) {
        const nextResults = await searchDocuments(query.trim(), true);
        setResults(nextResults);
        setSelectedPath(nextResults[0]?.path ?? null);
        setSearched(true);
      }
    } catch (reason) { setError(String(reason)); }
    finally { setSemanticBusy(false); setSemanticProgress(null); }
  }

  async function forgetFolder(path: string) {
    if (!window.confirm(`${path}\n\nRemove this folder from the index? Your source files will not be changed.`)) return;
    setBusy(true); setError(null);
    try {
      await removeFolder(path); setResults([]); setSearched(false); setSelectedPath(null);
      setNotice("Folder removed from the index. Source files were not changed.");
      await refresh();
    } catch (reason) { setError(String(reason)); } finally { setBusy(false); }
  }

  async function clearEverything() {
    if (!window.confirm("Delete all index data? Your source files will not be changed.")) return;
    setBusy(true); setError(null);
    try {
      await deleteIndex(); setResults([]); setSearched(false); setSelectedPath(null);
      setNotice("The local index was deleted. Source files were not changed.");
      await refresh();
    } catch (reason) { setError(String(reason)); } finally { setBusy(false); }
  }

  async function resultAction(action: (path: string) => Promise<void>, path: string) {
    setError(null);
    try { await action(path); } catch (reason) { setError(String(reason)); }
  }

  return (
    <main className="shell" aria-busy={busy || semanticBusy}>
      <header className="topbar">
        <a className="brand" href="#top" aria-label="SeekLocal home">
          <span className="brand-mark" aria-hidden="true"><span>S</span></span><span>SeekLocal</span>
        </a>
        <div className="topbar-meta">
          <span className="document-count">{stats.documents} documents</span>
          <span className="privacy-pill"><span className="status-dot" aria-hidden="true" /> Private &amp; offline</span>
        </div>
      </header>

      <section className="hero" id="top">
        <p className="eyebrow">PRIVATE SEARCH, RIGHT ON YOUR COMPUTER</p>
        <h1>Find anything in<br /><em>your local files.</em></h1>
        <p className="lede">Search the documents you already have. No uploads, no account, and every result links back to the original file.</p>
        <form className="search-box" onSubmit={submitSearch}>
          <span className="search-icon"><SearchIcon /></span>
          <label className="sr-only" htmlFor="search">Search documents</label>
          <input id="search" autoFocus lang="ja" inputMode="text" autoCapitalize="none" autoCorrect="off" spellCheck={false}
            value={query} onChange={(event) => setQuery(event.target.value)}
            onCompositionStart={() => { composingQuery.current = true; }}
            onCompositionEnd={(event) => {
              composingQuery.current = false;
              setQuery(event.currentTarget.value);
              window.setTimeout(() => { suppressCompositionSubmit.current = false; }, 0);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" && (composingQuery.current || event.nativeEvent.isComposing)) {
                suppressCompositionSubmit.current = true;
              }
            }}
            placeholder={stats.documents === 0 ? "Add a folder to start searching" : "Try a phrase, filename, or keyword"}
            disabled={stats.documents === 0} />
          <button type="submit" disabled={busy || semanticBusy || !query.trim()}>Search <span aria-hidden="true">→</span></button>
        </form>
        <div className="filter-chips" aria-label="Search filters">
          {SEARCH_FILTERS.map((filter) => <button key={filter.id} type="button" className={activeFilter === filter.id ? "active" : ""} aria-pressed={activeFilter === filter.id} onClick={() => chooseFilter(filter.id)} disabled={busy || semanticBusy}>{filter.label}</button>)}
        </div>
        {activeFilter === "semantic" && stats.documents > 0 && semanticStatus && !semanticStatus.ready && <div className="semantic-setup">
          <div><strong>{semanticStatus.modelAvailable ? "Build meaning index" : "Download meaning model"}</strong><span>{semanticBusy && semanticProgress?.phase === "download" ? "Downloading the multilingual model (about 267 MB)..." : semanticBusy && semanticProgress?.phase === "index" ? `Indexing files locally${semanticProgress.total > 0 ? ` · ${semanticProgress.completed} / ${semanticProgress.total}` : "..."}` : semanticStatus.modelAvailable ? `The model is stored locally. ${semanticStatus.indexedDocuments} of ${semanticStatus.totalDocuments} documents are indexed.` : "Downloads the reusable model once (about 267 MB). Changing folders will not download it again."}</span>{semanticBusy && semanticProgress?.phase === "index" && semanticProgress.total > 0 && <progress value={semanticProgress.completed} max={semanticProgress.total} aria-label="Meaning index progress" />}</div>
          <button type="button" onClick={semanticStatus.modelAvailable ? updateSemanticIndex : downloadSemanticModel} disabled={semanticBusy}>{semanticBusy ? (semanticProgress?.phase === "download" ? "Downloading..." : "Indexing...") : semanticStatus.modelAvailable ? "Build index" : "Download model"}</button>
        </div>}
        {stats.documents === 0 && <button className="hero-add" type="button" onClick={addFolder} disabled={busy}>Choose a folder <span aria-hidden="true">→</span></button>}
        <div className="notice-row"><span className={busy ? "pulse-dot" : "check-dot"} aria-hidden="true" /><p className="status-line" role="status">{busy ? "Working locally..." : notice}</p></div>
        {error && <p className="error" role="alert">{error}</p>}
      </section>

      <section className="workspace" aria-label="Local search workspace">
        <div className="metrics" aria-label="Index overview">
          <div className="metric metric-mint"><strong>{stats.documents}</strong><span>Searchable documents</span></div>
          <div className="metric metric-sky"><strong>{stats.folders}</strong><span>Indexed folders</span></div>
          <div className="metric metric-lavender"><strong>{stats.failures}</strong><span>Files skipped</span></div>
        </div>

        {searched && <section className="results" aria-label="Search results">
          <div className="section-heading results-heading"><div><p className="eyebrow">SEARCH RESULTS</p><h2>{results.length} {results.length === 1 ? "document" : "documents"} found</h2></div><span>Every match includes its source</span></div>
          {results.length === 0 ? <div className="empty-card"><span className="empty-icon"><SearchIcon /></span><h3>No matching documents</h3><p>Try a shorter phrase or a different keyword.</p></div> :
            <div className="results-layout"><div className="result-list" role="list" aria-label="Files matching your search" tabIndex={0}>{results.map((result) => <article className={`result-card ${selectedPath === result.path ? "selected" : ""}`} role="listitem" key={result.path} onClick={() => setSelectedPath(result.path)}>
              <div className={`file-badge ${result.extension}`} aria-hidden="true"><span>{result.extension.toUpperCase()}</span></div>
              <div className="result-body">
                <div className="result-title-row"><h3>{result.fileName}</h3><span className={`match-badge match-${result.matchType}`}>{result.matchType === "filename" ? "Filename match" : result.matchType === "semantic" ? "Meaning match" : result.matchType === "hybrid" ? "Strong match" : "Content match"}</span></div>
                <p className="path">{result.path}</p><p className="snippet"><HighlightedSnippet value={result.snippet} /></p>
                <div className="result-actions"><button type="button" onClick={() => resultAction(openResult, result.path)}>Open file <span aria-hidden="true">→</span></button><button type="button" className="quiet" onClick={() => resultAction(revealResult, result.path)}>Show in folder</button></div>
              </div>
            </article>)}</div>{selectedPath && (() => {
              const selected = results.find((result) => result.path === selectedPath);
              if (!selected) return null;
              return <aside className="evidence-panel" aria-label="Why this result matched">
                <div className="evidence-head"><p className="eyebrow">WHY THIS MATCHED</p><span className={`match-badge match-${selected.matchType}`}>{selected.matchType === "filename" ? "Filename" : selected.matchType === "semantic" ? "Meaning" : selected.matchType === "hybrid" ? "Words + meaning" : "Document body"}</span></div>
                <h3>{selected.fileName}</h3><p className="evidence-reason">{reasonFor(selected)}</p>
                <div className="source-preview"><span>Source excerpt</span><p><HighlightedSnippet value={selected.snippet} /></p></div>
                <dl><div><dt>Type</dt><dd>{selected.extension.toUpperCase()}</dd></div><div><dt>Location</dt><dd>{selected.path}</dd></div></dl>
                <div className="evidence-actions"><button type="button" onClick={() => resultAction(openResult, selected.path)}>Open original <span aria-hidden="true">→</span></button><button type="button" className="quiet" onClick={() => resultAction(revealResult, selected.path)}>Show in folder</button></div>
              </aside>;
            })()}</div>}
        </section>}

        <section className="library" aria-label="Indexed folders">
          <div className="section-heading"><div><p className="eyebrow">LOCAL LIBRARY</p><h2>Your searchable folders</h2></div><button className="primary" type="button" onClick={addFolder} disabled={busy}>Add folder <span aria-hidden="true">＋</span></button></div>
          {folders.length === 0 ? <button className="dropzone" type="button" onClick={addFolder} disabled={busy}>
            <span className="folder-icon"><FolderIcon /></span><span className="dropzone-copy"><strong>Choose your first folder</strong><small>TXT and Markdown files are indexed locally and left untouched.</small></span><span className="dropzone-arrow" aria-hidden="true">→</span>
          </button> : <div className="folder-list">{folders.map((folder) => <div className="folder-row" key={folder}>
            <span className="folder-icon"><FolderIcon /></span><div><strong>{basename(folder)}</strong><small>{folder}</small></div><span className="indexed-badge">Indexed</span><button type="button" className="quiet danger" onClick={() => forgetFolder(folder)}>Remove</button>
          </div>)}</div>}
          {stats.documents > 0 && <button className="delete-link" type="button" onClick={clearEverything}>Delete all local index data</button>}
        </section>
      </section>

      <footer><span><span className="footer-dot" aria-hidden="true" /> Your files never leave this device.</span><span>SeekLocal 0.1.0</span></footer>
    </main>
  );
}

export default App;
