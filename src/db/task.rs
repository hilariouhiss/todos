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
                    assignee, completed_at, created_by, created_at,
                    updated_by, updated_at, deleted_by, deleted_at,
                    archived_by, archived_at
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
                assignee: row.get(10)?,
                completed_at: row.get(11)?,
                created_by: row.get(12)?,
                created_at: row.get(13)?,
                updated_by: row.get(14)?,
                updated_at: row.get(15)?,
                deleted_by: row.get(16)?,
                deleted_at: row.get(17)?,
                archived_by: row.get(18)?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    use rusqlite::Connection;

    // ---- Test helpers ----

    fn setup_in_memory() -> (SqliteTaskRepository, Rc<RefCell<Connection>>) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!("../../sql/schema.sql"))
            .unwrap();
        let conn_rc = Rc::new(RefCell::new(conn));
        let repo = SqliteTaskRepository::new(conn_rc.clone());
        (repo, conn_rc)
    }

    fn insert_task(
        conn: &Connection,
        id: i64,
        title: &str,
        status: i32,
        priority: i32,
        sort_order: f64,
    ) {
        conn.execute(
            "INSERT INTO tasks (id, title, status, priority, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), datetime('now'))",
            rusqlite::params![id, title, status, priority, sort_order],
        )
        .unwrap();
    }

    fn insert_task_with_due(
        conn: &Connection,
        id: i64,
        title: &str,
        status: i32,
        sort_order: f64,
        completed_at: Option<&str>,
    ) {
        conn.execute(
            "INSERT INTO tasks (id, title, status, sort_order, due_at, completed_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), datetime('now'))",
            rusqlite::params![id, title, status, sort_order, completed_at.unwrap_or("2026-12-01"), completed_at],
        )
        .unwrap();
    }

    // ---- compute_sort_order ----

    #[test]
    fn compute_sort_order_empty_column() {
        assert_eq!(compute_sort_order(None, None), 1000.0);
    }

    #[test]
    fn compute_sort_order_before_first() {
        assert_eq!(compute_sort_order(None, Some(1000.0)), 0.0);
    }

    #[test]
    fn compute_sort_order_after_last() {
        assert_eq!(compute_sort_order(Some(2000.0), None), 3000.0);
    }

    #[test]
    fn compute_sort_order_between() {
        assert_eq!(compute_sort_order(Some(1000.0), Some(2000.0)), 1500.0);
    }

    #[test]
    fn compute_sort_order_between_fractional() {
        assert_eq!(compute_sort_order(Some(1.0), Some(2.0)), 1.5);
    }

    // ---- sort_order_gap_too_small ----

    #[test]
    fn gap_ok_when_wide() {
        assert!(!sort_order_gap_too_small(Some(1.0), Some(2.0)));
    }

    #[test]
    fn gap_too_small_when_tiny() {
        assert!(sort_order_gap_too_small(Some(1.0), Some(1.0000000001)));
    }

    #[test]
    fn gap_ok_when_missing_prev() {
        assert!(!sort_order_gap_too_small(None, Some(1000.0)));
    }

    #[test]
    fn gap_ok_when_missing_next() {
        assert!(!sort_order_gap_too_small(Some(1000.0), None));
    }

    #[test]
    fn gap_ok_when_both_none() {
        assert!(!sort_order_gap_too_small(None, None));
    }

    // ---- sort_neighbors ----

    fn make_task(id: i64, sort_order: f64) -> Task {
        Task {
            id,
            title: format!("task-{id}"),
            description: String::new(),
            status: TaskStatus::Todo,
            priority: 0,
            sort_order,
            due_at: None,
            reminder_at: None,
            parent_task_id: None,
            project_id: None,
            assignee: None,
            completed_at: None,
            created_by: None,
            created_at: String::new(),
            updated_by: None,
            updated_at: String::new(),
            deleted_by: None,
            deleted_at: None,
            archived_by: None,
            archived_at: None,
        }
    }

    #[test]
    fn sort_neighbors_cross_column_insert_at_start() {
        // [A:100, B:200, C:300], insert at index 0 → between None and A
        let tasks = vec![
            make_task(1, 100.0),
            make_task(2, 200.0),
            make_task(3, 300.0),
        ];
        let (prev, next) = sort_neighbors(&tasks, false, 0, 0);
        assert_eq!(prev, None);
        assert_eq!(next, Some(100.0));
    }

    #[test]
    fn sort_neighbors_cross_column_insert_at_end() {
        let tasks = vec![
            make_task(1, 100.0),
            make_task(2, 200.0),
            make_task(3, 300.0),
        ];
        let (prev, next) = sort_neighbors(&tasks, false, 0, 3);
        assert_eq!(prev, Some(300.0));
        assert_eq!(next, None);
    }

    #[test]
    fn sort_neighbors_cross_column_insert_between() {
        let tasks = vec![
            make_task(1, 100.0),
            make_task(2, 200.0),
            make_task(3, 300.0),
        ];
        let (prev, next) = sort_neighbors(&tasks, false, 0, 1);
        assert_eq!(prev, Some(100.0));
        assert_eq!(next, Some(200.0));
    }

    #[test]
    fn sort_neighbors_same_column_excludes_source() {
        // [A:100, B:200, C:300], move B (index 1) to index 2
        // effective_index=2 (corrected from raw 3 where it was dropped)
        // Neighbors should be C and None (B excluded, D doesn't exist)
        // Wait: effective_index is the CORRECTED index for 3-element list after removal
        // source_index=1, raw drop at index 2
        // effective_index = 2 (same column, source_index=1 < 2, so 2-1=1? No...)
        // Actually: source_index=1, target_index (raw)=3
        // effective_index = 3-1 = 2 (since source(1) < target(3))
        // After removing B: [A:100, C:300]
        // effective_index=2 means after C (= end)
        // prev = C(300), next = None
        let tasks = vec![
            make_task(1, 100.0),
            make_task(2, 200.0),
            make_task(3, 300.0),
        ];
        let (prev, next) = sort_neighbors(&tasks, true, 1, 2);
        assert_eq!(prev, Some(300.0)); // C
        assert_eq!(next, None); // end of list
    }

    #[test]
    fn sort_neighbors_same_column_excludes_source_insert_between() {
        // [A:100, B:200, C:300], move C (index 2) to index 0
        // effective_index=0 (source_index=2, not < target=0, so 0)
        // After removing C: [A:100, B:200]
        // effective_index=0 → between None and A
        let tasks = vec![
            make_task(1, 100.0),
            make_task(2, 200.0),
            make_task(3, 300.0),
        ];
        let (prev, next) = sort_neighbors(&tasks, true, 2, 0);
        assert_eq!(prev, None);
        assert_eq!(next, Some(100.0)); // A (after filtering, index 0 is A)
    }

    #[test]
    fn sort_neighbors_same_column_move_down_one() {
        // [A:100, B:200, C:300], swap A and B
        // source_index=0, target_index=1
        // effective_index=1 (source=0 < target=1, so 1-1=0? No: target_index=1, not < source=0)
        // Actually: source_index=0, target_index=1, source<target → effective=1-1=0
        // Hmm no. source_index=0 < target_index=1 → effective = 1-1 = 0
        // After removing A: [B:200, C:300], effective_index=0 → between None and B
        // That's wrong-ish... wait. If you drag A from index 0 to after index 0 (between A and B),
        // the raw target is 1. After removing A, effective is 0.
        // But the user wanted to put A after B? No, dropping at "index 1" means after position 1,
        // which is after the first card (A). After removing A, B is at 0, C at 1.
        // So target_index=1 means after B, which is index 1 in [B, C].
        // effective=0? That doesn't match.
        //
        // Actually the Slint drop-index formula:
        // drop-index = clamp(floor((hover-y - card-height/2) / card-stride) + 1, 0, tasks.length)
        //
        // Dropping between A and B (hover over that gap): hover-y is roughly card-height
        // card-stride = card-height + card-spacing
        // (card-height - card-height/2) / card-stride + 1 = card-height/2 / card-stride + 1 ≈ 0.5 + 1 = 1 (if card-spacing small)
        // So drop-index=1 means "after card index 0" = between A and B
        // After effective correction: source=0 < target=1 → effective = 0
        // After removal: [B:200, C:300], effective=0 → between None and B = BEFORE B
        // That's correct! Moving A from index 0 to between A and B, A is removed,
        // so the gap between None and B is the right spot.

        let tasks = vec![
            make_task(1, 100.0),
            make_task(2, 200.0),
            make_task(3, 300.0),
        ];
        // source_index=0, effective=0 (insert at start after removal)
        let (prev, next) = sort_neighbors(&tasks, true, 0, 0);
        assert_eq!(prev, None);
        assert_eq!(next, Some(200.0)); // B, since A is filtered out
    }

    // ---- SqliteTaskRepository integration tests ----

    #[test]
    fn load_by_status_empty() {
        let (repo, _conn) = setup_in_memory();
        let tasks = repo.load_by_status(TaskStatus::Todo).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn load_by_status_filters_by_status() {
        let (repo, conn) = setup_in_memory();
        {
            let c = conn.borrow();
            insert_task(&c, 1, "todo-1", 0, 1, 1000.0);
            insert_task(&c, 2, "doing-1", 1, 0, 2000.0);
            insert_task(&c, 3, "todo-2", 0, 2, 3000.0);
        }
        let todos = repo.load_by_status(TaskStatus::Todo).unwrap();
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].title, "todo-1");
        assert_eq!(todos[1].title, "todo-2");
    }

    #[test]
    fn load_by_status_orders_by_sort_order() {
        let (repo, conn) = setup_in_memory();
        {
            let c = conn.borrow();
            insert_task(&c, 1, "second", 0, 0, 5000.0);
            insert_task(&c, 2, "first", 0, 0, 1000.0);
            insert_task(&c, 3, "third", 0, 0, 9000.0);
        }
        let todos = repo.load_by_status(TaskStatus::Todo).unwrap();
        assert_eq!(todos[0].title, "first");
        assert_eq!(todos[1].title, "second");
        assert_eq!(todos[2].title, "third");
    }

    #[test]
    fn load_by_status_excludes_soft_deleted() {
        let (repo, conn) = setup_in_memory();
        {
            let c = conn.borrow();
            insert_task(&c, 1, "alive", 0, 0, 1000.0);
            c.execute(
                "INSERT INTO tasks (id, title, status, sort_order, deleted_at, created_at, updated_at)
                 VALUES (2, 'dead', 0, 2000.0, datetime('now'), datetime('now'), datetime('now'))",
                [],
            )
            .unwrap();
        }
        let todos = repo.load_by_status(TaskStatus::Todo).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].title, "alive");
    }

    #[test]
    fn move_task_cross_column_to_done_sets_completed_at() {
        let (repo, conn) = setup_in_memory();
        {
            let c = conn.borrow();
            insert_task(&c, 1, "task", 0, 0, 1000.0);
        }
        repo.move_task(1, TaskStatus::Done, 500.0).unwrap();
        let tasks = repo.load_by_status(TaskStatus::Done).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].completed_at.is_some());
    }

    #[test]
    fn move_task_from_done_to_todo_clears_completed_at() {
        let (repo, conn) = setup_in_memory();
        {
            let c = conn.borrow();
            insert_task_with_due(&c, 1, "done-task", 2, 1000.0, Some("2026-06-01 10:00:00"));
        }
        // Verify completed_at is set
        let tasks = repo.load_by_status(TaskStatus::Done).unwrap();
        assert!(tasks[0].completed_at.is_some());

        repo.move_task(1, TaskStatus::Todo, 500.0).unwrap();
        let tasks = repo.load_by_status(TaskStatus::Todo).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].completed_at.is_none());
    }

    #[test]
    fn move_task_updates_sort_order() {
        let (repo, conn) = setup_in_memory();
        {
            let c = conn.borrow();
            insert_task(&c, 1, "task", 0, 0, 1000.0);
        }
        repo.move_task(1, TaskStatus::InProgress, 7777.0).unwrap();
        let tasks = repo.load_by_status(TaskStatus::InProgress).unwrap();
        assert_eq!(tasks[0].sort_order, 7777.0);
    }

    #[test]
    fn renumber_column_renumbers_in_order() {
        let (repo, conn) = setup_in_memory();
        {
            let c = conn.borrow();
            // Disordered sort_orders
            insert_task(&c, 1, "a", 0, 0, 42.0);
            insert_task(&c, 2, "b", 0, 0, 17.0);
            insert_task(&c, 3, "c", 0, 0, 99.0);
        }
        repo.renumber_column(TaskStatus::Todo).unwrap();
        let tasks = repo.load_by_status(TaskStatus::Todo).unwrap();
        // After renumber, should be 1000, 2000, 3000 in the original sort_order
        // But wait — the SELECT in renumber orders by sort_order ASC,
        // so ids are processed in order: b(17), a(42), c(99)
        // Result: b=1000.0, a=2000.0, c=3000.0
        assert_eq!(tasks[0].sort_order, 1000.0);
        assert_eq!(tasks[1].sort_order, 2000.0);
        assert_eq!(tasks[2].sort_order, 3000.0);
        assert_eq!(tasks[0].title, "b");
        assert_eq!(tasks[1].title, "a");
        assert_eq!(tasks[2].title, "c");
    }

    #[test]
    fn renumber_column_empty_does_not_panic() {
        let (repo, _conn) = setup_in_memory();
        repo.renumber_column(TaskStatus::Todo).unwrap();
    }
}
