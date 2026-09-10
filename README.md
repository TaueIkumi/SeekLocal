# SeekLocal

### Search your files by meaning — without sending them anywhere.

[![Local-first](https://img.shields.io/badge/privacy-local--first-31b679)](#privacy-by-design)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/core-Rust-000000?logo=rust)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/UI-React-61DAFB?logo=react&logoColor=111111)](https://react.dev/)

SeekLocal is an open-source desktop search app for the documents already on your computer. Choose a folder, search in Japanese or English, and jump straight to the original file.

No account. No document uploads. No cloud database. No chat interface standing between you and your sources.

> SeekLocal is in early development. If local-first search is something you want to exist, **star the repository** and follow the journey.

## See it in action

[![SeekLocal demo showing keyword search and local meaning search](docs/assets/seeklocal-demo.gif)](SeekLocal.mp4)

The same query returns no keyword match, then finds the related document by meaning. [Watch the full 1080p video](SeekLocal.mp4).

## Download the Windows preview

[**Download SeekLocal v0.1.0-alpha.1 for Windows →**](https://github.com/TaueIkumi/SeekLocal/releases/download/v0.1.0-alpha.1/SeekLocal-0.1.0-windows-x64-setup.exe)

This unsigned early preview may trigger a Windows SmartScreen warning. You can verify the download with the published [`SHA256SUMS.txt`](https://github.com/TaueIkumi/SeekLocal/releases/download/v0.1.0-alpha.1/SHA256SUMS.txt).

## Why SeekLocal?

Most search tools make you choose between basic filename matching and uploading private documents to a hosted AI service. SeekLocal is exploring a third option: useful meaning-based retrieval that runs on your own machine.

- **Search by words or meaning** — find a passage even when it uses different wording from your query.
- **Japanese and English** — multilingual retrieval is built into the current semantic-search path.
- **Source-first results** — every result includes its filename, path, excerpt, and match type.
- **Your files stay yours** — source documents are never renamed, moved, edited, or deleted.
- **Works without a GPU** — keyword search and local CPU inference are first-class paths.
- **Disposable index** — remove one folder or erase the complete local index whenever you want.

## A search that keywords miss

The repository includes a small test folder that demonstrates meaning search.

Search for:

```text
jogging
```

The target document never uses the word `jogging`. It describes running beside a river in the morning instead. Regular keyword search returns no match; Meaning search retrieves [`01-morning-routine.md`](meaning-search-test/01-morning-routine.md).

Try it with the files in [`meaning-search-test/`](meaning-search-test/).

## How it works

```text
Your folders
     │ read-only scan
     ▼
Text and Markdown extraction
     │
     ├──► SQLite FTS5 ──────► exact and prefix matches
     │
     └──► local MiniLM ─────► meaning matches
                    │
                    ▼
        ranked, source-linked results
```

Keyword search is immediately available and runs entirely offline. Meaning search is optional: you explicitly download the reusable quantized multilingual model (about 267 MB), after which indexing and inference run locally.

## Current capabilities

- Index one or more folders recursively.
- Read UTF-8 `.txt`, `.md`, and `.markdown` files.
- Exclude common dependency, build, VCS, and system directories.
- Avoid symlinks and traversal loops.
- Search filenames and contents with local SQLite FTS5.
- Combine lexical and semantic ranking deterministically.
- Search completed documents while managing multiple indexed folders.
- Detect changed and deleted files when a folder is indexed again.
- Open a result or reveal it in the system file manager.
- Delete derived index data without touching source files.

## Try it locally

SeekLocal currently targets contributors and early testers. You will need Node.js, npm, Rust, and the [Tauri 2 platform prerequisites](https://v2.tauri.app/start/prerequisites/).

Windows preview installers are published as [GitHub pre-releases](https://github.com/TaueIkumi/SeekLocal/releases). They are currently unsigned and may trigger a Windows SmartScreen warning.

```bash
git clone https://github.com/TaueIkumi/SeekLocal.git
cd SeekLocal
npm install
npm run tauri:dev
```

To test meaning search:

1. Add the `meaning-search-test` folder.
2. Select **Meaning search**.
3. Download the model and build the local meaning index.
4. Search for `jogging`.

`npm run dev` launches the web interface only. Native folder selection, indexing, and file actions require `npm run tauri:dev`.

## Privacy by design

SeekLocal follows a few non-negotiable rules:

- Normal indexing and search make no network requests.
- Document contents, extracted text, embeddings, and queries remain on the device.
- Source files are treated as read-only.
- The search index lives in the application-data directory and can be rebuilt.
- Model download happens only after an explicit user action.
- There is no telemetry, analytics SDK, advertising, or account system.

See [`AGENTs.md`](AGENTs.md) for the complete product and engineering principles.

## Roadmap

- [x] Local folder discovery and incremental reconciliation
- [x] SQLite full-text search for Japanese and English
- [x] Optional local multilingual meaning search
- [x] Source excerpts and open/reveal actions
- [ ] PDF extraction with page-aware results
- [ ] DOCX extraction
- [ ] Background file watching and indexing controls
- [ ] Windows installer and clean-machine release validation
- [ ] Accessibility, keyboard navigation, and theme hardening

The focus is intentionally narrow: dependable local retrieval comes before chat, document management, or cloud synchronization.

## Development

```bash
npm run build
npm run lint
cargo fmt --all --check
cargo clippy -p seeklocal-core --all-targets -- -D warnings
cargo test -p seeklocal-core
```

Workspace-wide Rust checks and desktop builds may additionally require host-specific Tauri development packages. Dependency and license notes are recorded in [`docs/dependencies.md`](docs/dependencies.md), and the initial architecture decision is in [`docs/adr/0001-local-search-foundation.md`](docs/adr/0001-local-search-foundation.md).

## Contributing

Bug reports, focused pull requests, and feedback from people with real document collections are welcome. Good first contributions include extraction fixtures, Unicode edge cases, ranking tests, accessibility fixes, and Windows validation.

Please keep the core promise intact: local by default, read-only toward source files, and search results grounded in the original document.

---

If you want private, local meaning search to become a polished desktop app, **give SeekLocal a star**. It is the simplest way to signal that this project should keep going.

<details>
<summary>日本語で読む</summary>

SeekLocalは、PC内の文書をアップロードせずに検索するためのデスクトップアプリです。フォルダを選ぶだけで、キーワード検索と日本語・英語対応のローカル意味検索を利用できます。元ファイルは変更せず、検索結果から該当ファイルへ直接移動できます。

まだ開発初期段階です。ローカルファーストな文書検索に可能性を感じたら、Starで応援してもらえるとうれしいです。

</details>
