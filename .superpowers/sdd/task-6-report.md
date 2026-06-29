# Task 6 Report: Slint UI — Header Buttons, Dialog Overlays, and Api Callbacks

**Status:** Complete
**Date:** 2026-06-30

## Build Result

```
cargo build 2>&1
```

**Slint compilation:** SUCCESS (no errors)
**Rust compilation:** SUCCESS (4 warnings for unused code that Task 7 will wire up)

## Changes Made

### File modified: `ui/main-window.slint`

### 1. Import update
- Removed `TextInput` from `std-widgets.slint` import (it is a Slint builtin, not exported from std-widgets)
- Added `Button` and `ComboBox` to the import from `std-widgets.slint`
- Final import: `import { Palette, Button, ComboBox } from "std-widgets.slint";`

### 2. Dialog visibility properties (MainWindow)
Added four `in-out property <bool>` fields:
- `show-add-tag-dialog: false`
- `show-add-project-dialog: false`
- `show-add-task-dialog: false`
- `task-dialog-expanded: false`

### 3. Api global callbacks
Added three new callbacks:
- `add-tag(name: string, color: string)`
- `add-project(name: string, description: string, manager: string, color: string)`
- `add-task(title: string, description: string, due-at: string, priority: int, parent-task-id: string, project-id: string)`

### 4. ColorPresets global
Added `ColorPreset` struct and `ColorPresets` global with 8 color entries (blue, green, yellow, red, purple, orange, gray, white).

### 5. Header button bar
Replaced the standalone header `Text` with a `HorizontalLayout` containing:
- Hint text
- Spacer (`Rectangle { horizontal-stretch: 1; }`)
- "+标签", "+项目", "+任务" `Button` elements that toggle the respective dialog visibility

### 6. Three dialog overlays
Each dialog is a conditional `Rectangle` overlay positioned absolutely as a direct child of `MainWindow`:

- **Add Tag** (320x260): name TextInput, color preset grid, Cancel/Confirm buttons
- **Add Project** (360x380): name, description, manager TextInputs, color grid, Cancel/Confirm buttons
- **Add Task** (400x320 compact / 400x480 expanded): title, description, due date TextInputs, expand/collapse toggle, conditional ComboBox for priority and TextInputs for parent-task-id and project-id

## Slint-specific Fixes Made

**Fix 1: Dialog overlays must be outside layouts.** The initial implementation placed the dialog overlays inside the `VerticalLayout` with `x: 0; y: 0;`. Slint 1.17 rejects setting `y` on elements inside a layout because the layout manages positioning. The fix was to close the `VerticalLayout` after the kanban columns and place the three dialog overlays as direct children of `MainWindow`, where absolute positioning via `x`/`y` is permitted.

**Fix 2: TextInput is a builtin, not from std-widgets.slint.** The initial import included `TextInput` from `std-widgets.slint`, which is not an exported type. `TextInput` is a Slint builtin element and requires no import. Removed it from the import statement.

## Existing Code Preserved

- `TaskCardUi` struct, `Card` component, `KanbanColumn` component: unchanged
- `Layout`, `Theme`, `PriorityColors` globals: unchanged
- Column structure and drag-and-drop logic: unchanged
