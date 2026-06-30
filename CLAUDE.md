# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo fmt            # Format files
cargo clippy         # Check Code
cargo build          # Debug build
cargo run            # Build and run
cargo build --release  # Release build (LTO, single codegen unit)
cargo test           # Run tests
```

## Architecture

This is a **Slint** (slint.dev) GUI application — a declarative, reactive UI toolkit for Rust.

**Build pipeline:**

1. `build.rs` compiles the Slint UI file (`ui/main-window.slint`) into Rust code at build time
2. `src/main.rs` imports the generated code via `slint::include_modules!()` and instantiates `AppWindow`

**UI layer** (`ui/main-window.slint`): Declarative UI defined in Slint's DSL. Components expose **properties** (reactive data) and **callbacks** (events the Rust side handles). The template uses `in-out property` for two-way data binding and `callback` for event dispatch.

**Rust layer** (`src/main.rs`): Creates the UI instance, connects callback handlers (closures that manipulate properties), and runs the event loop via `ui.run()`. Uses `ui.as_weak()` for safe callback capture (prevents the UI from being kept alive by closures).

**Model layer** (`src/model.rs`): Domain entities — `Task` (full task row), `Project`, `TaskCardData` (lightweight UI card), and `TaskStatus` enum (Todo=0, InProgress=1, Done=2, Archived=3) with `from_i32` conversion.

**Database layer** (`src/db/`): Repository pattern — each entity has a trait (`TaskRepository`, `TagRepository`, `ProjectRepository`) and a `Sqlite*Repository` implementation that holds `Rc<RefCell<Connection>>` to share one SQLite connection across repos. `src/db.rs::init()` opens the DB, applies schema/auto-archive/seed SQL, and returns all three repo handles.

**SQL files** (`sql/`): `schema.sql` (table definitions), `seed.sql` (sample data), `auto_archive.sql` (triggers for archiving completed tasks).

**Sort ordering:** Tasks use string fractional indexing (Figma's algorithm via the `fractional_index` crate) with `sort_key: TEXT` columns. `new_sort_key_between` generates a key between two neighbors; `sort_neighbors` computes prev/next keys (filtering the source task on same-column moves); `renumber_column` rebalances a column with evenly-spaced keys when keys grow too long (>100 chars).

**Data flow:** UI events → callback → Rust handler updates DB via repos → rebuild column models from DB → Slint reactively re-renders. State is re-derived from the database on every change; there is no in-memory cache beyond what Slint properties hold.

**Platform note:** `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` suppresses the console window on Windows release builds so only the Slint window appears when launched from the file manager.

**Key dependencies:** `rusqlite` with `bundled` feature (bundles SQLite, no system dep needed). `chrono` for date handling (`NaiveDate`). `fractional_index` for string fractional indexing (sort keys). Rust edition 2024.

**Testing:** In-memory SQLite databases (`Connection::open_in_memory()`) with `setup_in_memory()` helpers in each `#[cfg(test)]` module. Database tests insert their own rows via helper functions rather than relying on seed data.

## Tool Usage

- **Context7 MCP** — Use when looking up library/framework/API documentation (Slint, Rust stdlib, crates, etc.)
- **Codegraph** — Use FIRST for searching/navigating code within this repository (symbol search, call traces, impact analysis)
