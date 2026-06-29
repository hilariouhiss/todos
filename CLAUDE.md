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

1. `build.rs` compiles the Slint UI file (`ui/app-window.slint`) into Rust code at build time
2. `src/main.rs` imports the generated code via `slint::include_modules!()` and instantiates `AppWindow`

**UI layer** (`ui/app-window.slint`): Declarative UI defined in Slint's DSL. Components expose **properties** (reactive data) and **callbacks** (events the Rust side handles). The template uses `in-out property` for two-way data binding and `callback` for event dispatch.

**Rust layer** (`src/main.rs`): Creates the UI instance, connects callback handlers (closures that manipulate properties), and runs the event loop via `ui.run()`. Uses `ui.as_weak()` for safe callback capture (prevents the UI from being kept alive by closures).

**Data flow:** UI events → callback → Rust handler updates properties → Slint reactively re-renders affected UI elements. There is no separate model or state management layer — state lives in Slint properties manipulated by Rust callbacks.

**Platform note:** `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` suppresses the console window on Windows release builds so only the Slint window appears when launched from the file manager.

## Tool Usage

- **Context7 MCP** — Use when looking up library/framework/API documentation (Slint, Rust stdlib, crates, etc.)
- **Codegraph** — Use FIRST for searching/navigating code within this repository (symbol search, call traces, impact analysis)
