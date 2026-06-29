#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod model;

use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;

use slint::language::DragAction;
use slint::{DataTransfer, ModelRc, SharedString, VecModel};

use db::tag::TagRepository;
use db::task::{self, TaskRepository};
use model::{TaskCardData, TaskStatus};

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
    // --- Database init ---
    let (task_repo, tag_repo) = db::init("todos.db")?;

    // --- Load initial column data ---
    let todo_tasks = task_repo.load_by_status(TaskStatus::Todo)?;
    let doing_tasks = task_repo.load_by_status(TaskStatus::InProgress)?;
    let done_tasks = task_repo.load_by_status(TaskStatus::Done)?;

    let all_ids: Vec<i64> = todo_tasks
        .iter()
        .chain(doing_tasks.iter())
        .chain(done_tasks.iter())
        .map(|t| t.id)
        .collect();
    let tags_map = tag_repo.load_for_tasks(&all_ids)?;

    let todo_model = build_card_model(&todo_tasks, &tags_map);
    let doing_model = build_card_model(&doing_tasks, &tags_map);
    let done_model = build_card_model(&done_tasks, &tags_map);

    // --- Create UI ---
    let ui = MainWindow::new()?;
    ui.set_todo(todo_model);
    ui.set_doing(doing_model);
    ui.set_done(done_model);

    // --- Wire Api callbacks ---
    let task_repo_rc = Rc::new(task_repo);
    let tag_repo_rc = Rc::new(tag_repo);
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
            let payload: DragPayload = match event.data.user_data() {
                Some(rc) => match rc.downcast::<DragPayload>() {
                    Ok(rc) => (*rc).clone(),
                    Err(_) => return DragAction::None,
                },
                None => return DragAction::None,
            };

            let task_id = payload.task_id;
            let source_column = payload.source_column;
            let source_index = payload.source_index;

            let target_status = column_to_status(target_column);
            let source_status = column_to_status(source_column);
            let (Some(target_status), Some(source_status)) = (target_status, source_status) else {
                return DragAction::None;
            };

            // Same-column index correction
            let effective_index = if source_column == target_column && source_index < target_index {
                target_index - 1
            } else {
                target_index
            };

            // No-op guard
            if source_column == target_column && source_index == effective_index {
                return DragAction::None;
            }

            // Load target column for sort_order computation
            let target_tasks = match task_repo.load_by_status(target_status) {
                Ok(t) => t,
                Err(_) => return DragAction::None,
            };

            // Check precision and renumber if needed
            let prev_order = if effective_index > 0 {
                target_tasks
                    .get(effective_index as usize - 1)
                    .map(|t| t.sort_order)
            } else {
                None
            };
            let next_order = target_tasks
                .get(effective_index as usize)
                .map(|t| t.sort_order);

            if task::sort_order_gap_too_small(prev_order, next_order) {
                let _ = task_repo.renumber_column(target_status);
            }

            // Reload to get fresh sort_orders (may have changed from renumber)
            let reloaded = task_repo
                .load_by_status(target_status)
                .unwrap_or(target_tasks);
            let prev_order = if effective_index > 0 {
                reloaded
                    .get(effective_index as usize - 1)
                    .map(|t| t.sort_order)
            } else {
                None
            };
            let next_order = reloaded.get(effective_index as usize).map(|t| t.sort_order);
            let new_order = task::compute_sort_order(prev_order, next_order);

            if task_repo
                .move_task(task_id, target_status, new_order)
                .is_err()
            {
                return DragAction::None;
            }

            // Rebuild and update UI models
            if let Some(ui) = ui_weak.upgrade() {
                rebuild_and_set_column(&*task_repo, &*tag_repo, target_status, &ui);

                if source_column != target_column {
                    rebuild_and_set_column(&*task_repo, &*tag_repo, source_status, &ui);
                }
            }

            DragAction::Move
        });
    }

    // --- Run ---
    ui.run()?;

    Ok(())
}

// ---- Helpers ----

fn column_to_status(col: i32) -> Option<TaskStatus> {
    match col {
        0 => Some(TaskStatus::Todo),
        1 => Some(TaskStatus::InProgress),
        2 => Some(TaskStatus::Done),
        _ => None,
    }
}

/// Reload tasks for `status` from the database, build a fresh `ModelRc`,
/// and set it on the correct `MainWindow` property.
fn rebuild_and_set_column(
    task_repo: &dyn TaskRepository,
    tag_repo: &dyn TagRepository,
    status: TaskStatus,
    ui: &MainWindow,
) {
    let tasks = match task_repo.load_by_status(status) {
        Ok(t) => t,
        Err(_) => return,
    };
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
    ModelRc::from(Rc::new(VecModel::from(cards)))
}

/// Convert platform-independent `TaskCardData` to the Slint-generated `TaskCardUi`.
fn card_data_to_slint(data: TaskCardData) -> TaskCardUi {
    let tags_model: ModelRc<SharedString> = ModelRc::from(Rc::new(VecModel::from(
        data.tags
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));

    TaskCardUi {
        id: data.id.into(),
        title: data.title.into(),
        priority: data.priority,
        due_text: data.due_text.into(),
        is_overdue: data.is_overdue,
        tags: tags_model,
    }
}
