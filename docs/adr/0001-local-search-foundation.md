# ADR 0001: Local search foundation

- Status: Accepted
- Date: 2026-09-09

## Context

The repository initially contained only `AGENT.md`. The first implementation needs to establish a useful end-to-end path without committing the application to a particular PDF, DOCX, or semantic-search engine.

## Decision

Use a Tauri 2 shell with a React/TypeScript interface and keep discovery, extraction, indexing, and search inside a separate Rust crate. Store derived text in an application-data SQLite database and use bundled SQLite FTS5 with Unicode trigram tokenization for deterministic Japanese and English retrieval. Queries shorter than three characters use a bounded SQLite substring search because trigram indexes cannot represent them.

The first vertical slice indexes UTF-8 TXT and Markdown files. It does not make network requests, follow symbolic links, or modify source files. Additional extractors will implement the same boundary in later increments.

## Consequences

- Search remains usable without a GPU or network connection.
- The bundled SQLite build increases binary size but avoids requiring a system SQLite installation and provides FTS5 consistently.
- PDF, DOCX, background watching, cancellation, and semantic search remain explicit MVP work.
- Opening a file is allowed only after its canonical path is found in the local index.
