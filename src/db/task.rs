use std::cell::RefCell;
use std::rc::Rc;

use rusqlite::{Connection, Result, params};

use crate::model::{Task, TaskStatus};

pub trait TaskRepository {
    fn load_by_status(&self, status: TaskStatus) -> Result<Vec<Task>>;
    fn move_task(&self, task_id: i64, new_status: TaskStatus, sort_order: f64) -> Result<()>;
    fn renumber_column(&self, status: TaskStatus) -> Result<()>;
}

pub struct SqliteTaskRepository {
    conn: Rc<RefCell<Connection>>,
}

impl SqliteTaskRepository {
    pub fn new(conn: Rc<RefCell<Connection>>) -> Self {
        Self { conn }
    }
}

impl TaskRepository for SqliteTaskRepository {
    fn load_by_status(&self, status: TaskStatus) -> Result<Vec<Task>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT id, title, description, status, priority, sort_order,
                    due_at, reminder_at, parent_task_id, project_id,
                    assignee_id, completed_at, creator_id, created_at,
                    updater_id, updated_at, deleter_id, deleted_at,
                    archiver_id, archived_at
             FROM tasks
             WHERE deleted_at IS NULL AND status = ?1
             ORDER BY sort_order ASC",
        )?;

        let rows = stmt.query_map(params![status as i32], |row| {
            Ok(Task {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                status: TaskStatus::from_i32(row.get::<_, i32>(3)?).unwrap_or(TaskStatus::Todo),
                priority: row.get(4)?,
                sort_order: row.get(5)?,
                due_at: row
                    .get::<_, Option<String>>(6)?
                    .and_then(|s| chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()),
                reminder_at: row.get(7)?,
                parent_task_id: row.get(8)?,
                project_id: row.get(9)?,
                assignee_id: row.get(10)?,
                completed_at: row.get(11)?,
                creator_id: row.get(12)?,
                created_at: row.get(13)?,
                updater_id: row.get(14)?,
                updated_at: row.get(15)?,
                deleter_id: row.get(16)?,
                deleted_at: row.get(17)?,
                archiver_id: row.get(18)?,
                archived_at: row.get(19)?,
            })
        })?;

        rows.collect()
    }

    fn move_task(&self, task_id: i64, new_status: TaskStatus, sort_order: f64) -> Result<()> {
        let conn = self.conn.borrow();

        let completed_col = match new_status {
            TaskStatus::Done => "completed_at = COALESCE(completed_at, datetime('now')),",
            _ => "completed_at = NULL,",
        };

        let sql = format!(
            "UPDATE tasks SET status = ?1, sort_order = ?2, {completed_col} updated_at = datetime('now') WHERE id = ?3"
        );

        conn.execute(&sql, params![new_status as i32, sort_order, task_id])?;
        Ok(())
    }

    fn renumber_column(&self, status: TaskStatus) -> Result<()> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT id FROM tasks
             WHERE deleted_at IS NULL AND status = ?1
             ORDER BY sort_order ASC",
        )?;

        let ids: Vec<i64> = stmt
            .query_map(params![status as i32], |row| row.get(0))?
            .collect::<Result<Vec<i64>>>()?;

        // Drop the statement borrow before starting the transaction
        drop(stmt);

        conn.execute_batch("BEGIN IMMEDIATE")?;
        for (i, id) in ids.iter().enumerate() {
            let new_order = (i as f64 + 1.0) * 1000.0;
            conn.execute(
                "UPDATE tasks SET sort_order = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![new_order, id],
            )?;
        }
        conn.execute_batch("COMMIT")?;
        Ok(())
    }
}

/// Compute `sort_order` for a task inserted between `prev` and `next`.
pub fn compute_sort_order(prev: Option<f64>, next: Option<f64>) -> f64 {
    match (prev, next) {
        (None, None) => 1000.0,
        (None, Some(next)) => next - 1000.0,
        (Some(prev), None) => prev + 1000.0,
        (Some(prev), Some(next)) => (prev + next) / 2.0,
    }
}

/// True when the gap between adjacent sort_orders is too narrow for safe insertion.
pub fn sort_order_gap_too_small(prev: Option<f64>, next: Option<f64>) -> bool {
    match (prev, next) {
        (Some(p), Some(n)) => (n - p).abs() < 0.000001,
        _ => false,
    }
}

/// Compute prev and next sort_orders for insertion at `effective_index`.
/// When `filter_source` is true, the task at `source_index` is excluded so its
/// own sort_order doesn't contaminate the computation (same-column moves).
/// Uses direct indexing — no allocation.
pub fn sort_neighbors(
    tasks: &[Task],
    filter_source: bool,
    source_index: i32,
    effective_index: i32,
) -> (Option<f64>, Option<f64>) {
    let index_of = |i: i32| -> usize {
        let raw = i as usize;
        if filter_source && raw >= source_index as usize {
            raw + 1
        } else {
            raw
        }
    };

    let prev = if effective_index > 0 {
        tasks
            .get(index_of(effective_index - 1))
            .map(|t| t.sort_order)
    } else {
        None
    };
    let next = tasks.get(index_of(effective_index)).map(|t| t.sort_order);
    (prev, next)
}
