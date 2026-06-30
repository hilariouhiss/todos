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

This is a **Slint** (slint.dev) GUI application — a declarative, reactive UI toolkit for Rust — implementing a kanban-style task manager.

### Build pipeline

1. `build.rs` calls `slint_build::compile("ui/main-window.slint")`, which compiles the entire Slint UI tree (main-window.slint imports all files under `ui/widgets/` and `ui/dialogs/`) into Rust code at build time.
2. `src/main.rs` imports the generated code via `slint::include_modules!()` and instantiates `MainWindow` (the root Slint component).

### UI layer (`ui/`)

The UI is organized into three directories:

- **`ui/` root** — `main-window.slint` (root window + `Api` callback bridge), `theme.slint` (dark/light color singleton), `common.slint` (shared types: `TaskCardUi` struct, `ThemeSettings` global, `Layout`/`PriorityColors`/`ColorPresets` globals), `archived-tasks.slint` (archive overlay).
- **`ui/widgets/`** — Reusable components: `KanbanColumn` (drag-and-drop column), `Card` (draggable task card), `Sidebar` (left nav bar), `DialogShell`/`DialogHeader`/`DialogFooter`/`DialogButtons`/`CloseButton` (dialog building blocks), `StyledTextInput` (themed input wrapper), `ColorPresetsBar` (color swatch row).
- **`ui/dialogs/`** — Modal dialogs: `AddTagDialog`, `AddProjectDialog`, `AddTaskDialog`, `SettingsDialog`.

**Component hierarchy:**

```
MainWindow (Window)
  HorizontalLayout
    Sidebar → SidebarButton (gear: settings) + SidebarButton (package: archive)
    VerticalLayout
      Header bar (title + buttons: +tag, +project, +task)
      Three KanbanColumn instances (todo / doing / done)
  // Conditional overlays:
  AddTagDialog | AddProjectDialog | AddTaskDialog | SettingsDialog | ArchivedTasksOverlay
```

**Key Slint patterns:**

- **`ThemeSettings` global** (`common.slint`): `in-out` properties for `theme-mode`, `auto-archive-days`, `auto-archive-enabled`, and six per-column sort configs (`todo/doing/done-sort-field` and `todo/doing/done-sort-ascending`). These are `<=>` two-way bound to `MainWindow` properties, enabling bidirectional sync between Rust and UI.
- **`Theme` global** (`theme.slint`): Derives all color properties from `ThemeSettings.theme-mode`, updating reactively when the user toggles themes.
- **Pure callbacks**: `make-data` and `can-drop` are `pure` callbacks (no side effects), allowing Slint to call them speculatively during drag operations.
- **Modal pattern**: Dialogs render as full-screen semi-transparent overlays with centered `DialogCard`, built by composing `DialogHeader`, content body, and `DialogFooter`.
- **Conditional rendering**: All dialogs use `if root.show-*-dialog:` — they only exist in the component tree when visible.

### Rust layer (`src/main.rs`)

Entry point that orchestrates the entire application:

1. **Loads settings** via `Settings::load()` (reads `settings.toml` or returns defaults).
2. **Initializes database** via `db::init()` — opens SQLite, applies schema, runs auto-archive, seeds sample data if empty.
3. **Creates UI** via `MainWindow::new()`, pushes settings into UI properties.
4. **Loads columns** by calling `rebuild_and_set_column()` for each status.
5. **Wires callbacks** on `Api`:
   - `on_make_data` / `on_can_drop` / `on_dropped` — drag-and-drop across kanban columns (same-column reorder only allowed in Manual sort mode).
   - `on_add_tag` / `on_add_project` / `on_add_task` — create entities, reload affected columns, close dialog.
   - `on_save_settings` — builds `Settings` from UI state, persists to `settings.toml`, reloads all columns.
   - `on_open_archived` — loads archived tasks (status=3), shows the archived overlay.
6. **Runs event loop** via `ui.run()`.

Uses `ui.as_weak()` for safe callback capture (prevents the UI from being kept alive by closures).

Key helper functions: `rebuild_and_set_column()` (loads DB → runs `sort_tasks()` → builds `TaskCardUi` model → sets on UI), `get_sort_config()` (reads per-column sort from UI), `build_settings()` (snapshots UI state to `Settings`).

### Model layer (`src/model/`)

Re-exported via `src/model.rs`. All domain entities:

| File | Key types | Purpose |
| --- | --- | --- |
| `task.rs` | `Task` | Full task row (20 fields including audit columns). Methods: `is_overdue()`, `to_card_data()`. |
| `task_status.rs` | `TaskStatus` | Enum: `Todo=0`, `InProgress=1`, `Done=2`, `Archived=3`. Has `from_i32` conversion. |
| `task_card_data.rs` | `TaskCardData` | Platform-independent intermediate between `Task` and Slint's `TaskCardUi`. Contains: `id`, `title`, `priority`, `due_text`, `is_overdue`, `tags`. |
| `project.rs` | `Project` | Project entity (11 fields, soft-delete pattern). |
| `sort.rs` | `SortField`, `SortConfig`, `sort_tasks()` | Sort system (see Sort section below). |
| `settings.rs` | `Settings`, `ColumnSortSettings` | User settings + TOML persistence (see Settings section below). |

**Important:** `TaskCardData` decouples the model from the Slint framework. Domain `Task` → `to_card_data()` → `TaskCardData` → `card_data_to_slint()` → Slint `TaskCardUi`. This keeps the model layer framework-agnostic.

### Database layer (`src/db/`)

Repository pattern — each entity has a trait and a SQLite implementation sharing `Rc<RefCell<Connection>>`:

| Repository trait | Implementation | Key methods |
| --- | --- | --- |
| `TaskRepository` | `SqliteTaskRepository` | `load_by_status()`, `move_task()`, `renumber_column()`, `insert()` |
| `TagRepository` | `SqliteTagRepository` | `load_for_tasks()` (batch), `insert()` |
| `ProjectRepository` | `SqliteProjectRepository` | `load_all()`, `insert()` |

`src/db.rs::init()` opens the SQLite database at a given path, executes `schema.sql` (all tables/indexes via `IF NOT EXISTS`), runs auto-archive inline (parameterized by days), seeds sample data if the tasks table is empty, and returns all three repo handles.

**Soft-delete pattern:** Rows are never physically deleted; `deleted_at` is set instead. All queries filter `WHERE deleted_at IS NULL`.

### Sort system (`src/model/sort.rs`)

**`SortField` enum:** `Manual`, `Priority`, `DueDate`, `Title` (serialized as `"manual"`, `"priority"`, `"due_date"`, `"title"`). Default: `Priority`.

**`SortConfig` struct:** `{ field: SortField, direction: bool }` where `true` = ascending, `false` = descending.

**`sort_tasks(tasks: &mut [Task], config: SortConfig)`** sorts in-place. DB always returns tasks `ORDER BY sort_key ASC`. For `Manual+Ascending`: no-op (DB order preserved). For `Manual+Descending`: reverses. For other fields: uses `sort_by` with the appropriate comparator.

**Pinyin-aware title sorting:** `pinyin_sort_key()` converts Chinese characters to tone-less pinyin via the `pinyin` crate, lowercases everything else, then compares the resulting flat ASCII strings. Uses `sort_by_cached_key` to compute each key once (O(n log n) computations, not O(n²)).

**Per-column sort config:** Each of the three kanban columns has its own independent `SortConfig`, persisted in `settings.toml` and synced to the UI via `ThemeSettings` globals. When sort is not Manual, same-column drag-and-drop reordering is blocked (`can_reorder_in_column()`), since the next column rebuild would undo the manual order.

**Drag-and-drop reordering:** Uses string fractional indexing (Figma's algorithm via the `fractional_index` crate). `new_sort_key_between` generates a key between two neighbors; `sort_neighbors` computes prev/next keys (filtering the source task on same-column moves); `renumber_column` rebalances a column with evenly-spaced keys when keys grow too long (>100 chars). New tasks are always inserted into the Todo column, generating a sort_key after the last existing one.

### Settings system (`src/model/settings.rs`)

**`Settings` struct:**

```rust
{
    theme_mode: String,            // "system" (default), "light", or "dark"
    auto_archive_enabled: bool,    // default true
    auto_archive_days: u32,        // default 7; 0 = archive immediately
    column_sort: ColumnSortSettings,  // per-column SortConfig
}
```

Persisted to/from `settings.toml` (next to the executable) via `toml::to_string_pretty` / `toml::from_str`. `Settings::load()` reads on startup; `Settings::save()` writes when the user confirms in the settings dialog.

Settings are NOT auto-saved on any other action — only when the user clicks "Confirm" in the settings dialog.

### Data flow

```
UI events → callback → Rust handler updates DB via repos → rebuild column models from DB
→ apply client-side sort via sort_tasks() → set on MainWindow properties → Slint reactively re-renders
```

State is re-derived from the database on every change; there is no in-memory cache beyond what Slint properties hold. Settings flow differently: changes propagate through `ThemeSettings` globals with `<=>` bindings, enabling the theme and sort config to update reactively without a full column rebuild.

### Naming conventions

- **Slint `MainWindow` properties** use `doing_` prefix for the InProgress column (e.g., `doing_sort_field`, `doing_sort_ascending`).
- **Rust/Settings struct** uses `in_progress` for the same column (`ColumnSortSettings.in_progress`).
- **Task status enum** is `TaskStatus::InProgress`, rendered in the UI as "Doing".

### Platform note

`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` suppresses the console window on Windows release builds so only the Slint window appears when launched from the file manager.

### Key dependencies

- `slint` 1.17 — Declarative UI framework
- `rusqlite` 0.40 (bundled feature) — SQLite, no system dep needed
- `chrono` 0.4 — Date handling (`NaiveDate`, duration arithmetic)
- `fractional_index` 2.0 — String fractional indexing for sort keys
- `serde` 1.0 (derive feature) — Serialization for settings
- `toml` 1.1 — TOML parsing/generation for `settings.toml`
- `pinyin` 0.10 — Chinese pinyin conversion for title sorting

### Testing

In-memory SQLite databases (`Connection::open_in_memory()`) with `setup_in_memory()` helpers in each `#[cfg(test)]` module. Database tests insert their own rows via helper functions rather than relying on seed data.

Test coverage by area:

- `model/task.rs` — Overdue logic and card data conversion
- `model/sort.rs` — All sort combinations (Manual/Priority/DueDate/Title × asc/desc), empty/single lists, pinyin ordering, Chinese+ASCII mixing
- `db/task.rs` — Repository methods and fractional-index helpers
- `db/tag.rs` — Batch tag loading, empty input, ordering
- `db/project.rs` — Insert and load

## Tool Usage

- **Context7 MCP** — Use when looking up library/framework/API documentation (Slint, Rust stdlib, crates, etc.)
- **Codegraph** — Use FIRST for searching/navigating code within this repository (symbol search, call traces, impact analysis)
