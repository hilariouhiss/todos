#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod model;

use std::error::Error;
use std::rc::Rc;

use slint::language::DragAction;
use slint::{CloseRequestResponse, ComponentHandle, DataTransfer, ModelRc, SharedString, VecModel};

use db::project::ProjectRepository;
use db::tag::TagRepository;
use db::task::{self, TaskRepository};
use model::sort::sort_tasks;
use model::{ColumnSortSettings, Settings, SortConfig, SortField, TaskCardData, TaskStatus};

// include_modules!() only picks up the last-compiled file via SLINT_INCLUDE_GENERATED.
// We compile two entry points (tray.slint and main-window.slint), so include both manually.
include!(concat!(env!("OUT_DIR"), "/tray.rs"));
include!(concat!(env!("OUT_DIR"), "/main-window.rs"));

// ---- Drag payload ----

/// Data attached to each `DataTransfer` via `set_user_data`.
#[derive(Clone)]
struct DragPayload {
    task_id: i64,
    source_column: i32,
    source_index: i32,
}

// ---- Column model handles ----

/// Persistent `VecModel` handles for the three kanban columns.
/// Updating these in-place via `set_vec()` avoids creating new `ModelRc`
/// allocations on every change.
struct ColumnModels {
    todo: Rc<VecModel<TaskCardUi>>,
    doing: Rc<VecModel<TaskCardUi>>,
    done: Rc<VecModel<TaskCardUi>>,
}

impl ColumnModels {
    fn update(&self, status: TaskStatus, cards: Vec<TaskCardUi>) {
        match status {
            TaskStatus::Todo => self.todo.set_vec(cards),
            TaskStatus::InProgress => self.doing.set_vec(cards),
            TaskStatus::Done => self.done.set_vec(cards),
            TaskStatus::Archived => {}
        }
    }
}

// ---- Entry point ----

fn main() -> Result<(), Box<dyn Error>> {
    // --- Load settings ---
    let settings = Settings::load();

    // --- Database init ---
    let (task_repo, tag_repo, project_repo) = db::init(
        "todos.db",
        settings.auto_archive_enabled,
        settings.auto_archive_days,
    )?;

    // --- Create UI ---
    let ui = MainWindow::new()?;

    // Apply persisted settings to ThemeSettings globals (<=>-bound to MainWindow).
    push_settings_to_ui(&ui, &settings);

    // --- Load initial column data ---
    let models = Rc::new(ColumnModels {
        todo: Rc::new(VecModel::from(load_and_build_cards(
            &task_repo,
            &tag_repo,
            TaskStatus::Todo,
            get_sort_config(&ui, TaskStatus::Todo),
        ))),
        doing: Rc::new(VecModel::from(load_and_build_cards(
            &task_repo,
            &tag_repo,
            TaskStatus::InProgress,
            get_sort_config(&ui, TaskStatus::InProgress),
        ))),
        done: Rc::new(VecModel::from(load_and_build_cards(
            &task_repo,
            &tag_repo,
            TaskStatus::Done,
            get_sort_config(&ui, TaskStatus::Done),
        ))),
    });

    ui.set_todo(ModelRc::from(models.todo.clone()));
    ui.set_doing(ModelRc::from(models.doing.clone()));
    ui.set_done(ModelRc::from(models.done.clone()));

    // --- Wire Api callbacks ---
    let task_repo_rc = Rc::new(task_repo);
    let tag_repo_rc = Rc::new(tag_repo);
    let project_repo_rc = Rc::new(project_repo);
    let ui_weak = ui.as_weak();

    let api = ui.global::<Api>();

    // make-data: construct DataTransfer with DragPayload user data
    api.on_make_data(move |task_id_str, source_column, source_index| {
        let task_id: i64 = task_id_str.parse().unwrap_or(0);
        let payload = DragPayload {
            task_id,
            source_column,
            source_index,
        };
        let mut dt = DataTransfer::default();
        dt.set_user_data(Rc::new(payload));
        dt
    });

    // can-drop: accept move if internal payload present
    api.on_can_drop(move |event, _target_column, _target_index| {
        if event.data.user_data().is_some() {
            return event.proposed_action;
        }
        DragAction::None
    });

    // dropped: execute the move in the database
    {
        let task_repo = task_repo_rc.clone();
        let tag_repo = tag_repo_rc.clone();
        let ui_weak = ui_weak.clone();
        let models = models.clone();

        api.on_dropped(move |event, target_column, target_index| {
            let payload = match extract_payload(event.data.user_data()) {
                Some(p) => p,
                None => return DragAction::None,
            };

            let task_id = payload.task_id;
            let source_column = payload.source_column;
            let source_index = payload.source_index;

            let Some(target_status) = TaskStatus::from_i32(target_column) else {
                return DragAction::None;
            };
            let Some(source_status) = TaskStatus::from_i32(source_column) else {
                return DragAction::None;
            };

            // Same-column index correction
            let effective_index = if source_column == target_column && source_index < target_index {
                target_index - 1
            } else {
                target_index
            };

            // No-op guard: same-index, or same-column drag in non-manual mode
            let is_same_column = source_column == target_column;
            if is_same_column {
                if source_index == effective_index {
                    return DragAction::None;
                }
                // Block same-column reordering when sort is not manual
                if !can_reorder_in_column(&ui_weak, target_status) {
                    return DragAction::None;
                }
            }

            // Load target column for sort_key computation
            let target_tasks = match task_repo.load_by_status(target_status) {
                Ok(t) => t,
                Err(_) => return DragAction::None,
            };

            // Compute prev/next sort_keys from neighbors (source-filtered if same column)
            let (prev_key, next_key) =
                task::sort_neighbors(&target_tasks, is_same_column, source_index, effective_index);

            if task::sort_key_needs_rebalance(prev_key.as_deref(), next_key.as_deref()) {
                let _ = task_repo.renumber_column(target_status);
            }

            // Reload (may have changed from renumber), re-filter, compute final sort_key
            let reloaded = task_repo
                .load_by_status(target_status)
                .unwrap_or(target_tasks);
            let (prev_key, next_key) =
                task::sort_neighbors(&reloaded, is_same_column, source_index, effective_index);
            let Some(new_key) =
                task::new_sort_key_between(prev_key.as_deref(), next_key.as_deref())
            else {
                return DragAction::None;
            };

            if task_repo
                .move_task(task_id, target_status, &new_key)
                .is_err()
            {
                return DragAction::None;
            }

            // Rebuild and update UI models
            if let Some(ui) = ui_weak.upgrade() {
                rebuild_and_set_column(&*task_repo, &*tag_repo, target_status, &ui, &models);

                if !is_same_column {
                    rebuild_and_set_column(&*task_repo, &*tag_repo, source_status, &ui, &models);
                }
            }

            DragAction::Move
        });
    }

    // add-tag: insert tag, reload all columns, close dialog
    {
        let tag_repo = tag_repo_rc.clone();
        let task_repo = task_repo_rc.clone();
        let tag_repo_2 = tag_repo_rc.clone();
        let ui_weak = ui_weak.clone();
        let models = models.clone();

        api.on_add_tag(move |name, color| {
            let _ = tag_repo.insert(&name, if color.is_empty() { None } else { Some(&color) });
            if let Some(ui) = ui_weak.upgrade() {
                reload_all_columns(&*task_repo, &*tag_repo_2, &ui, &models);
                ui.set_show_add_tag_dialog(false);
            }
        });
    }

    // add-project: insert project, reload all columns, close dialog
    {
        let project_repo = project_repo_rc.clone();
        let task_repo = task_repo_rc.clone();
        let tag_repo = tag_repo_rc.clone();
        let ui_weak = ui_weak.clone();
        let models = models.clone();

        api.on_add_project(move |name, description, manager, color| {
            let _ = project_repo.insert(
                &name,
                &description,
                &manager,
                if color.is_empty() { None } else { Some(&color) },
            );
            if let Some(ui) = ui_weak.upgrade() {
                reload_all_columns(&*task_repo, &*tag_repo, &ui, &models);
                ui.set_show_add_project_dialog(false);
            }
        });
    }

    // add-task: insert task (goes to Todo), reload Todo column, close dialog
    {
        let task_repo = task_repo_rc.clone();
        let tag_repo = tag_repo_rc.clone();
        let ui_weak = ui_weak.clone();
        let models = models.clone();

        api.on_add_task(
            move |title, description, due_at, priority, parent_task_id_str, project_id_str| {
                let due = if due_at.is_empty() {
                    None
                } else {
                    Some(due_at.as_str())
                };
                let parent: Option<i64> = parent_task_id_str.parse().ok();
                let project: Option<i64> = project_id_str.parse().ok();
                let _ = task_repo.insert(&title, &description, due, priority, parent, project);
                if let Some(ui) = ui_weak.upgrade() {
                    rebuild_and_set_column(&*task_repo, &*tag_repo, TaskStatus::Todo, &ui, &models);
                    ui.set_show_add_task_dialog(false);
                }
            },
        );
    }

    // save-settings: persist all settings (including sort config) to settings.toml
    {
        let task_repo = task_repo_rc.clone();
        let tag_repo = tag_repo_rc.clone();
        let ui_weak = ui_weak.clone();
        let models = models.clone();

        api.on_save_settings(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let _ = pull_settings_from_ui(&ui).save();
            reload_all_columns(&*task_repo, &*tag_repo, &ui, &models);
            ui.set_show_settings_dialog(false);
        });
    }

    // save-close-behavior: persist close behavior and act on it
    {
        let ui_weak = ui_weak.clone();
        api.on_save_close_behavior(move |behavior_str, dont_ask_again| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let behavior = if dont_ask_again {
                behavior_str.to_string()
            } else {
                // If "don't ask again" not checked, act but don't persist
                // (we don't persist anything, keeping close_behavior = "" so dialog shows next time)
                String::new()
            };

            if !behavior.is_empty() {
                ui.set_close_behavior(behavior.clone().into());
                let _ = pull_settings_from_ui(&ui).save();
            }

            execute_close_behavior(&ui, &behavior_str);
        });
    }

    // open-archived: load archived tasks and show overlay
    {
        let task_repo = task_repo_rc.clone();
        let tag_repo = tag_repo_rc.clone();
        let ui_weak = ui_weak.clone();

        api.on_open_archived(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let tasks = match task_repo.load_by_status(TaskStatus::Archived) {
                Ok(t) => t,
                Err(_) => return,
            };
            let ids: Vec<i64> = tasks.iter().map(|t| t.id).collect();
            let tags_map = tag_repo.load_for_tasks(&ids).unwrap_or_default();

            let rows: Vec<ArchivedTaskUi> = tasks
                .iter()
                .map(|task| {
                    let tags = tags_map.get(&task.id).cloned().unwrap_or_default();
                    ArchivedTaskUi {
                        title: task.title.clone().into(),
                        completed_at: task.completed_at.clone().unwrap_or_default().into(),
                        archived_at: task.archived_at.clone().unwrap_or_default().into(),
                        priority: task.priority,
                        tags: tags.join(", ").into(),
                    }
                })
                .collect();

            let model: ModelRc<ArchivedTaskUi> = ModelRc::from(Rc::new(VecModel::from(rows)));
            ui.set_archived_tasks(model);
            ui.set_show_archived(true);
        });
    }

    // --- System tray ---
    let tray = AppTrayIcon::new()?;
    {
        let ui_weak = ui_weak.clone();
        tray.on_show_window(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.window().show().ok();
            }
        });
    }
    {
        let ui_weak = ui_weak.clone();
        tray.on_open_settings(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.window().show().ok();
                ui.set_show_settings_dialog(true);
            }
        });
    }
    tray.on_quit_application(|| {
        slint::quit_event_loop().ok();
    });

    // --- Window close interception ---
    {
        let ui_weak = ui_weak.clone();
        ui.window().on_close_requested(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return CloseRequestResponse::HideWindow;
            };
            let behavior = ui.get_close_behavior();
            execute_close_behavior(&ui, &behavior)
        });
    }

    // --- Run ---
    // Show the window explicitly (ui.run() usually does this, but we use the
    // free function to keep running when the window is hidden to tray).
    ui.show()?;
    // Use run_event_loop_until_quit to keep running even when all windows
    // are hidden (the tray icon keeps the event loop alive).
    slint::run_event_loop_until_quit()?;

    Ok(())
}

// ---- Helpers ----

/// Read the current sort configuration for a column from the MainWindow properties
/// (which are `<=>`-bound to `ThemeSettings` globals).
fn get_sort_config(ui: &MainWindow, status: TaskStatus) -> SortConfig {
    let (field, ascending) = match status {
        TaskStatus::Todo => (ui.get_todo_sort_field(), ui.get_todo_sort_ascending()),
        TaskStatus::InProgress => (ui.get_doing_sort_field(), ui.get_doing_sort_ascending()),
        TaskStatus::Done => (ui.get_done_sort_field(), ui.get_done_sort_ascending()),
        _ => return SortConfig::default(),
    };
    SortConfig {
        field: SortField::from_i32(field),
        direction: ascending,
    }
}

/// Check whether same-column drag reordering is allowed for a column.
/// Returns `false` when the column's sort mode is not Manual (reordering
/// would be immediately undone by the sort), or when the UI is gone.
fn can_reorder_in_column(ui_weak: &slint::Weak<MainWindow>, status: TaskStatus) -> bool {
    let Some(ui) = ui_weak.upgrade() else {
        return false;
    };
    get_sort_config(&ui, status).field == SortField::Manual
}

/// Execute the action for a given close-behavior string.
/// Returns the appropriate response for `on_close_requested`.
fn execute_close_behavior(ui: &MainWindow, behavior: &str) -> CloseRequestResponse {
    match behavior {
        "quit" => {
            slint::quit_event_loop().ok();
            CloseRequestResponse::HideWindow
        }
        "minimize_to_tray" => {
            ui.window().hide().ok();
            CloseRequestResponse::KeepWindowShown
        }
        _ => {
            // Unset — show the close behavior dialog
            ui.set_show_close_behavior_dialog(true);
            CloseRequestResponse::KeepWindowShown
        }
    }
}

/// Push all `Settings` fields to MainWindow properties on startup
/// (which are `<=>`-bound to `ThemeSettings` globals).
fn push_settings_to_ui(ui: &MainWindow, settings: &Settings) {
    ui.set_theme_mode(settings.theme_mode.clone().into());
    ui.set_auto_archive_days(settings.auto_archive_days as i32);
    ui.set_auto_archive_enabled(settings.auto_archive_enabled);
    ui.set_close_behavior(settings.close_behavior.clone().into());
    ui.set_todo_sort_field(settings.column_sort.todo.field.to_i32());
    ui.set_todo_sort_ascending(settings.column_sort.todo.direction);
    ui.set_doing_sort_field(settings.column_sort.in_progress.field.to_i32());
    ui.set_doing_sort_ascending(settings.column_sort.in_progress.direction);
    ui.set_done_sort_field(settings.column_sort.done.field.to_i32());
    ui.set_done_sort_ascending(settings.column_sort.done.direction);
}

/// Snapshot MainWindow properties into a `Settings` struct for persisting.
fn pull_settings_from_ui(ui: &MainWindow) -> Settings {
    Settings {
        theme_mode: ui.get_theme_mode().to_string(),
        auto_archive_days: ui.get_auto_archive_days() as u32,
        auto_archive_enabled: ui.get_auto_archive_enabled(),
        close_behavior: ui.get_close_behavior().to_string(),
        column_sort: ColumnSortSettings {
            todo: SortConfig {
                field: SortField::from_i32(ui.get_todo_sort_field()),
                direction: ui.get_todo_sort_ascending(),
            },
            in_progress: SortConfig {
                field: SortField::from_i32(ui.get_doing_sort_field()),
                direction: ui.get_doing_sort_ascending(),
            },
            done: SortConfig {
                field: SortField::from_i32(ui.get_done_sort_field()),
                direction: ui.get_done_sort_ascending(),
            },
        },
    }
}

fn extract_payload(data: Option<Rc<dyn std::any::Any>>) -> Option<DragPayload> {
    data.and_then(|rc| rc.downcast::<DragPayload>().ok())
        .map(|rc| (*rc).clone())
}

/// Reload all three columns from the database and update the models in-place.
fn reload_all_columns(
    task_repo: &dyn TaskRepository,
    tag_repo: &dyn TagRepository,
    ui: &MainWindow,
    models: &ColumnModels,
) {
    rebuild_and_set_column(task_repo, tag_repo, TaskStatus::Todo, ui, models);
    rebuild_and_set_column(task_repo, tag_repo, TaskStatus::InProgress, ui, models);
    rebuild_and_set_column(task_repo, tag_repo, TaskStatus::Done, ui, models);
}

/// Reload tasks for `status` from the database and update the `ColumnModels` in-place
/// via `VecModel::set_vec()`, avoiding new `ModelRc` allocations.
fn rebuild_and_set_column(
    task_repo: &dyn TaskRepository,
    tag_repo: &dyn TagRepository,
    status: TaskStatus,
    ui: &MainWindow,
    models: &ColumnModels,
) {
    let sort_config = get_sort_config(ui, status);
    let cards = load_and_build_cards(task_repo, tag_repo, status, sort_config);
    models.update(status, cards);
}

/// Load tasks from DB, sort, and convert to `Vec<TaskCardUi>` for the UI model.
fn load_and_build_cards(
    task_repo: &dyn TaskRepository,
    tag_repo: &dyn TagRepository,
    status: TaskStatus,
    sort_config: SortConfig,
) -> Vec<TaskCardUi> {
    let mut tasks = match task_repo.load_by_status(status) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    sort_tasks(&mut tasks, sort_config);

    let ids: Vec<i64> = tasks.iter().map(|t| t.id).collect();
    let tags = tag_repo.load_for_tasks(&ids).unwrap_or_default();

    tasks
        .iter()
        .map(|task| {
            let task_tags = tags.get(&task.id).cloned().unwrap_or_default();
            card_data_to_slint(task.to_card_data(task_tags))
        })
        .collect()
}

/// Convert platform-independent `TaskCardData` to the Slint-generated `TaskCardUi`.
fn card_data_to_slint(data: TaskCardData) -> TaskCardUi {
    TaskCardUi {
        id: data.id.as_str().into(),
        title: data.title.as_str().into(),
        priority: data.priority,
        due_text: data.due_text.as_str().into(),
        is_overdue: data.is_overdue,
        tags: vec_to_model_rc(data.tags.into_iter().map(SharedString::from).collect()),
    }
}

fn vec_to_model_rc<T: 'static + Clone>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}
