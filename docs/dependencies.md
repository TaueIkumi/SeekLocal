# Direct dependency review

Reviewed on 2026-09-09. SeekLocal's normal indexing and search path does not require network access.

| Dependency | Purpose | License |
| --- | --- | --- |
| Tauri / Tauri build / dialog plugin | Desktop shell and native folder picker | Apache-2.0 OR MIT |
| React / React DOM | User interface | MIT |
| SQLite (bundled through rusqlite) | Local database and FTS5 | Public domain (SQLite); rusqlite is MIT |
| BLAKE3 | Content fingerprinting | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception |
| fastembed / ONNX Runtime | Optional local CPU embedding inference | Apache-2.0 / MIT |
| walkdir | Bounded filesystem traversal | Unlicense OR MIT |
| serde / thiserror | Typed serialization and errors | MIT OR Apache-2.0 |

Vite, TypeScript, ESLint, and their plugins are development-only dependencies and are not shipped as application runtime services. Their direct licenses are MIT or Apache-2.0.

The optional meaning-search setup downloads `intfloat/multilingual-e5-small` (MIT) into SeekLocal's application-data directory only after an explicit user action that displays the approximate download size. Once cached, inference and search run locally.

Before enabling application bundling, generate and review a complete transitive third-party notice for the Windows artifact. PDF/DOCX parsers require separate redistribution reviews before they are added.
