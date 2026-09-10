# SeekLocal Agent Guide

## 1. Product mission

SeekLocal is a privacy-first desktop search application for finding information inside local documents.

The core promise is:

> Select a folder, search the contents of documents in natural language, and jump directly to the matching source—all without uploading files or creating an account.

SeekLocal is not a document-management system and must not reorganize, replace, or silently copy a user's source documents. It builds a local, disposable search index over the user's existing folder structure.

## 2. Product principles

All implementation decisions must follow these principles, in priority order:

1. **Local by default and by design**
   - Document contents, extracted text, embeddings, thumbnails, queries, and search history stay on the device.
   - Core functionality must not depend on a cloud API, account, hosted database, or remote inference service.
   - Do not add telemetry, analytics, advertising SDKs, or automatic network requests.
   - Any future network-enabled feature must be optional, visibly disclosed, and disabled by default.

2. **Read-only toward source files**
   - Never modify, rename, move, delete, or overwrite indexed source files.
   - Store derived data only in SeekLocal's application-data directory.
   - Opening a result may launch the original file, but SeekLocal must not claim ownership of it.

3. **Search before chat**
   - The primary experience is fast, trustworthy retrieval—not a generic chatbot.
   - Every result must show why it matched: filename, path, excerpt, match type, and page or section when available.
   - AI-generated answers must not be added until source-linked search is reliable.

4. **Simple installation**
   - The initial target is Windows 10/11 x64.
   - End users should install and run SeekLocal without Python, Docker, Ollama, Node.js, or developer tooling.
   - Development dependencies may be complex; the released application must not expose that complexity.

5. **Useful without a GPU**
   - Keyword search must work on ordinary CPU-only machines.
   - Semantic search must have a CPU-compatible path and clear resource controls.
   - Expensive indexing must run in the background without making the computer unusable.

## 3. MVP scope

The first usable release should include only the following:

- Select one or more folders to index.
- Recursively discover supported documents while respecting exclusions.
- Extract text from PDF, DOCX, TXT, and Markdown files.
- Index filename, path, metadata, and extracted text locally.
- Provide fast full-text search with typo-tolerant or prefix-friendly behavior where practical.
- Provide optional multilingual semantic search that works with Japanese and English.
- Display ranked results with highlighted excerpts.
- Show the matching PDF page when page information is available.
- Open the original file and reveal it in File Explorer.
- Detect created, changed, moved, and deleted files and update the index incrementally.
- Let the user pause indexing, remove an indexed folder, and delete all derived index data.
- Clearly display indexing status, failures, and unsupported files.

The following are explicitly post-MVP:

- AI chat or document question answering.
- OCR for scanned PDFs and images.
- XLSX, PPTX, email, archive, audio, and video indexing.
- File tagging, renaming, moving, deduplication, or document management.
- Cloud synchronization or shared/team indexes.
- macOS and Linux packaging.
- Browser extensions and MCP integration.

Do not expand the MVP merely because a post-MVP feature is interesting. Prefer a small, dependable search product over a broad demo.

## 4. Proposed technical direction

Unless the repository later records a different decision in an architecture decision record, use:

- **Desktop shell:** Tauri 2
- **Frontend:** React and TypeScript
- **Core/indexer:** Rust
- **Local database:** SQLite
- **Lexical search:** SQLite FTS5
- **Semantic embeddings:** a bundled or explicitly downloaded ONNX model with multilingual Japanese/English support
- **PDF rendering/extraction:** PDFium or another redistributable local engine after license review
- **DOCX extraction:** direct Open XML parsing where sufficient
- **File watching:** native filesystem notifications with a periodic reconciliation pass

Keep document extraction, indexing, ranking, and UI behind explicit interfaces so individual engines can be replaced without rewriting the product.

Suggested logical modules:

```text
apps/desktop        Tauri application and React UI
crates/core         Domain types and application services
crates/discovery    Folder traversal, exclusions, and file watching
crates/extractors   Format-specific text and metadata extraction
crates/index        SQLite schema, FTS, migrations, and index lifecycle
crates/search       Query parsing, ranking, snippets, and result fusion
crates/semantic     Optional local embeddings and vector retrieval
```

This layout is a direction, not a requirement before the project is scaffolded. Prefer the smallest structure that preserves these boundaries.

## 5. Search behavior

- Exact and keyword matches should normally outrank weaker semantic matches.
- Rank documents, not isolated chunks; use the best matching passages to explain each document result.
- Avoid allowing a long document with many chunks to dominate the results.
- Preserve page numbers, headings, paragraph positions, and character offsets whenever extractors can provide them.
- Search results must be deterministic for identical index and settings.
- Empty, malformed, encrypted, and unsupported documents must fail individually rather than aborting an indexing job.
- Never present generated text as a verbatim source excerpt.

## 6. Index and data handling

- The index is derived, disposable data. Users must be able to rebuild it at any time.
- Use schema migrations for persistent database changes.
- Store a stable file identity where possible, plus path, size, modification time, and a content fingerprint sufficient to avoid unnecessary reprocessing.
- Do not store full duplicate copies of source documents.
- Minimize extracted sensitive data outside the index database.
- Keep temporary files inside the application-data or OS temporary directory and remove them after use.
- Exclude common dependency, build, VCS, and system directories by default, while allowing user overrides.
- Handle symlinks and junctions safely; prevent traversal loops.

## 7. Security and privacy requirements

- Treat every indexed document and filename as untrusted input.
- Do not execute macros, scripts, embedded files, links, or active PDF content.
- Run parsers with the least privilege practical and put resource limits around untrusted inputs.
- Defend against path traversal, archive bombs when archive support is added, oversized documents, malformed parsers, and uncontrolled memory use.
- Do not bind a local HTTP server to non-loopback interfaces. Prefer Tauri IPC when possible.
- Never log document contents, search queries, tokens, personal paths, or extracted text in production logs.
- Logs should contain safe operational identifiers and actionable error categories.
- Document every network request. A default installation should make none during normal indexing and searching.
- Model download and update checks, if introduced, require an explicit user action and visible destination/size information.
- Review the license and redistribution terms of every native library, model, tokenizer, and binary before bundling it.

## 8. UX requirements

- A new user should reach their first search result within a few minutes of installation.
- Make indexing progress understandable: discovered files, completed files, failures, and remaining work.
- Allow searching completed portions while indexing continues.
- Never hide a parser failure; summarize it without overwhelming the user.
- Always show the original path and a source excerpt for a result.
- Clearly distinguish keyword, filename, metadata, and semantic matches.
- Provide a prominent pause control and a clear way to delete the local index.
- Avoid technical terms such as embeddings, vectors, FTS, and inference in the default UI.
- Accessibility, keyboard navigation, light/dark themes, and Japanese text rendering are baseline quality requirements.

## 9. Engineering rules

- Keep source files focused and avoid large modules with mixed responsibilities.
- Prefer explicit types and error variants over stringly typed state.
- Do not use `unwrap`, `expect`, unchecked indexing, or panics on user-controlled data in production Rust paths.
- Keep blocking filesystem, parser, database, and inference work off the UI thread.
- Bound concurrency and queues; indexing an enormous folder must not create unbounded tasks.
- Make cancellation cooperative and test it.
- Use parameterized SQL and transactions for index updates.
- Validate paths at the boundary and use canonical paths carefully without breaking removable-drive workflows.
- Add dependencies only when they materially reduce risk or implementation cost.
- Update this guide and relevant documentation when a product invariant changes.

## 10. Testing expectations

Changes should be verified at the lowest practical level and, when relevant, end to end.

At minimum, cover:

- Extraction from representative PDF, DOCX, TXT, and Markdown fixtures.
- Japanese, English, mixed-language, Unicode, emoji, and unusual filename handling.
- Empty, encrypted, corrupted, very large, and permission-denied files.
- Ranking behavior for filename, exact phrase, keyword, and semantic matches.
- Incremental create/update/move/delete behavior.
- Symlink or junction loops and excluded directories.
- Index cancellation, restart, rebuild, and migration.
- Deleting an indexed folder without touching the source folder.
- Offline operation with network access disabled.

Test fixtures must be synthetic or redistributable. Never commit private user documents.

Before calling a release ready, verify:

- The packaged app runs on a clean Windows machine without developer runtimes.
- Core search works with the network disabled.
- Uninstalling the app does not delete or alter source documents.
- Index-data deletion removes derived content and leaves originals untouched.
- Installer and release artifacts have checksums; code signing should be added before broad distribution.

## 11. Development workflow

- Inspect the repository and existing decisions before editing.
- Preserve unrelated user changes.
- Keep changes small enough to review and test.
- For meaningful architectural choices, add a short ADR under `docs/adr/`.
- Every feature change should include tests or an explanation of why automated coverage is impractical.
- Every user-facing change should include a screenshot or short recording when visual verification is useful.
- Do not claim a feature is complete until its primary path and important failure paths have been exercised.

Use the following commands from the repository root:

```text
npm install                                      Install frontend and Tauri CLI dependencies
npm run tauri:dev                                Run the desktop app in development
npm run build                                    Type-check and build the frontend
npm run lint                                     Lint the frontend
cargo fmt --all --check                          Check Rust formatting
cargo clippy -p seeklocal-core --all-targets -- -D warnings
                                                 Lint the platform-independent search core
cargo test -p seeklocal-core                     Test the platform-independent search core
npm run tauri:build                              Build the desktop application on a configured host
```

`cargo test --workspace` and workspace-wide Clippy additionally require the host-specific [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). Release packaging is currently disabled until Windows icons, checksums, and installer validation are added.

## 12. Definition of done

A task is done when:

- It satisfies the requested behavior without violating product principles.
- Source documents remain untouched.
- Error, cancellation, and offline behavior have been considered.
- Relevant tests pass.
- Formatting and lint checks pass.
- User-visible behavior is documented.
- New dependencies and licenses have been reviewed.
- No sensitive document content is exposed through logs, fixtures, analytics, or network traffic.

When requirements conflict, protect user files and privacy first, preserve search correctness second, and optimize performance third.
