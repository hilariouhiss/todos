#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod model;

use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;

use slint::language::DragAction;
use slint::{ComponentHandle, DataTransfer, ModelRc, SharedString, VecModel};

use db::project::ProjectRepository;
use db::tag::TagRepository;
use db::task::{self, TaskRepository};
use model::sort::sort_tasks;
use model::{ColumnSortSettings, Settings, SortConfig, SortField, TaskCardData, TaskStatus};

slint::include_modules!();

// ---- Drag payload ----

/// Data attached to each `DataTransfer` via `set_user_data`.
#[derive(Clone)]
struct DragPayload {
    task_id: i64,
    source_column: i32,
    source_index: i32,
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

    // Apply persisted settings to MainWindow properties
    ui.set_theme_mode(settings.theme_mode.clone().into());
    ui.set_auto_archive_days(settings.auto_archive_days as i32);
    ui.set_auto_archive_enabled(settings.auto_archive_enabled);

    // Apply sort settings
    ui.set_todo_sort_field(settings.sort_field_index(TaskStatus::Todo));
    ui.set_todo_sort_ascending(settings.sort_ascending(TaskStatus::Todo));
    ui.set_doing_sort_field(settings.sort_field_index(TaskStatus::InProgress));
    ui.set_doing_sort_ascending(settings.sort_ascending(TaskStatus::InProgress));
    ui.set_done_sort_field(settings.sort_field_index(TaskStatus::Done));
    ui.set_done_sort_ascending(settings.sort_ascending(TaskStatus::Done));

    // --- Load initial column data ---
    rebuild_and_set_column(&task_repo, &tag_repo, TaskStatus::Todo, &ui);
    rebuild_and_set_column(&task_repo, &tag_repo, TaskStatus::InProgress, &ui);
    rebuild_and_set_column(&task_repo, &tag_repo, TaskStatus::Done, &ui);

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
                rebuild_and_set_column(&*task_repo, &*tag_repo, target_status, &ui);

                if !is_same_column {
                    rebuild_and_set_column(&*task_repo, &*tag_repo, source_status, &ui);
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

        api.on_add_tag(move |name, color| {
            let _ = tag_repo.insert(&name, if color.is_empty() { None } else { Some(&color) });
            if let Some(ui) = ui_weak.upgrade() {
                reload_all_columns(&*task_repo, &*tag_repo_2, &ui);
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

        api.on_add_project(move |name, description, manager, color| {
            let _ = project_repo.insert(
                &name,
                &description,
                &manager,
                if color.is_empty() { None } else { Some(&color) },
            );
            if let Some(ui) = ui_weak.upgrade() {
                reload_all_columns(&*task_repo, &*tag_repo, &ui);
                ui.set_show_add_project_dialog(false);
            }
        });
    }

    // add-task: insert task (goes to Todo), reload Todo column, close dialog
    {
        let task_repo = task_repo_rc.clone();
        let tag_repo = tag_repo_rc.clone();
        let ui_weak = ui_weak.clone();

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
                    rebuild_and_set_column(&*task_repo, &*tag_repo, TaskStatus::Todo, &ui);
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

        api.on_save_settings(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let _ = build_settings(&ui).save();
            reload_all_columns(&*task_repo, &*tag_repo, &ui);
            ui.set_show_settings_dialog(false);
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

    // --- Run ---
    ui.run()?;

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

/// Build a `Settings` snapshot from the current MainWindow property values.
fn build_settings(ui: &MainWindow) -> Settings {
    Settings {
        theme_mode: ui.get_theme_mode().to_string(),
        auto_archive_days: ui.get_auto_archive_days() as u32,
        auto_archive_enabled: ui.get_auto_archive_enabled(),
        column_sort: ColumnSortSettings {
            todo: get_sort_config(ui, TaskStatus::Todo),
            in_progress: get_sort_config(ui, TaskStatus::InProgress),
            done: get_sort_config(ui, TaskStatus::Done),
        },
    }
}

fn extract_payload(data: Option<Rc<dyn std::any::Any>>) -> Option<DragPayload> {
    data.and_then(|rc| rc.downcast::<DragPayload>().ok())
        .map(|rc| (*rc).clone())
}

/// Reload all three columns from the database and set them on the UI.
fn reload_all_columns(
    task_repo: &dyn TaskRepository,
    tag_repo: &dyn TagRepository,
    ui: &MainWindow,
) {
    rebuild_and_set_column(task_repo, tag_repo, TaskStatus::Todo, ui);
    rebuild_and_set_column(task_repo, tag_repo, TaskStatus::InProgress, ui);
    rebuild_and_set_column(task_repo, tag_repo, TaskStatus::Done, ui);
}

/// Reload tasks for `status` from the database, build a fresh `ModelRc`,
/// and set it on the correct `MainWindow` property.
fn rebuild_and_set_column(
    task_repo: &dyn TaskRepository,
    tag_repo: &dyn TagRepository,
    status: TaskStatus,
    ui: &MainWindow,
) {
    let mut tasks = match task_repo.load_by_status(status) {
        Ok(t) => t,
        Err(_) => return,
    };
    // Apply client-side sort according to the column's sort config
    let sort_config = get_sort_config(ui, status);
    sort_tasks(&mut tasks, sort_config);

    let ids: Vec<i64> = tasks.iter().map(|t| t.id).collect();
    let tags = tag_repo.load_for_tasks(&ids).unwrap_or_default();
    let model = build_card_model(&tasks, &tags);

    match status {
        TaskStatus::Todo => ui.set_todo(model),
        TaskStatus::InProgress => ui.set_doing(model),
        TaskStatus::Done => ui.set_done(model),
        _ => {}
    }
}

/// Convert domain task list + tag map into a Slint `ModelRc<TaskCardUi>`.
fn build_card_model(
    tasks: &[model::Task],
    tags_map: &HashMap<i64, Vec<String>>,
) -> ModelRc<TaskCardUi> {
    let cards: Vec<TaskCardUi> = tasks
        .iter()
        .map(|task| {
            let tags = tags_map.get(&task.id).cloned().unwrap_or_default();
            card_data_to_slint(task.to_card_data(tags))
        })
        .collect();
    vec_to_model_rc(cards)
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
