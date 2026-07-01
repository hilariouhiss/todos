use std::cell::RefCell;
use std::rc::Rc;

use fractional_index::FractionalIndex;
use rusqlite::{Connection, Result, params};

use crate::model::{Task, TaskStatus};

pub trait TaskRepository {
    fn load_by_status(&self, status: TaskStatus) -> Result<Vec<Task>>;
    fn move_task(&self, task_id: i64, new_status: TaskStatus, sort_key: &str) -> Result<()>;
    fn renumber_column(&self, status: TaskStatus) -> Result<()>;
    fn insert(
        &self,
        title: &str,
        description: &str,
        due_at: Option<&str>,
        priority: i32,
        parent_task_id: Option<i64>,
        project_id: Option<i64>,
    ) -> Result<i64>;
    /// Run auto-archive: move eligible Done tasks to Archived.
    /// No-op when `enabled` is false.
    fn run_auto_archive(&self, enabled: bool, days: u32) -> Result<()>;
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
            "SELECT id, title, description, status, priority, sort_key,
                    due_at, reminder_at, parent_task_id, project_id,
                    assignee, completed_at, created_by, created_at,
                    updated_by, updated_at, deleted_by, deleted_at,
                    archived_by, archived_at
             FROM tasks
             WHERE deleted_at IS NULL AND status = ?1
             ORDER BY sort_key ASC",
        )?;

        let rows = stmt.query_map(params![status as i32], |row| {
            Ok(Task {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                status: TaskStatus::from_i32(row.get::<_, i32>(3)?).unwrap_or(TaskStatus::Todo),
                priority: row.get(4)?,
                sort_key: row.get(5)?,
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

    fn move_task(&self, task_id: i64, new_status: TaskStatus, sort_key: &str) -> Result<()> {
        let conn = self.conn.borrow();

        let completed_col = match new_status {
            TaskStatus::Done => "completed_at = COALESCE(completed_at, datetime('now')),",
            _ => "completed_at = NULL,",
        };

        let sql = format!(
            "UPDATE tasks SET status = ?1, sort_key = ?2, {completed_col} updated_at = datetime('now') WHERE id = ?3"
        );

        conn.execute(&sql, params![new_status as i32, sort_key, task_id])?;
        Ok(())
    }

    fn renumber_column(&self, status: TaskStatus) -> Result<()> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT id FROM tasks
             WHERE deleted_at IS NULL AND status = ?1
             ORDER BY sort_key ASC",
        )?;

        let ids: Vec<i64> = stmt
            .query_map(params![status as i32], |row| row.get(0))?
            .collect::<Result<Vec<i64>>>()?;

        drop(stmt);

        conn.execute_batch("BEGIN IMMEDIATE")?;
        let mut key = FractionalIndex::default();
        for id in &ids {
            let key_str = key.to_string();
            conn.execute(
                "UPDATE tasks SET sort_key = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![key_str, id],
            )?;
            key = FractionalIndex::new_after(&key);
        }
        conn.execute_batch("COMMIT")?;
        Ok(())
    }

    fn run_auto_archive(&self, enabled: bool, days: u32) -> Result<()> {
        let conn = self.conn.borrow();
        crate::db::run_auto_archive(&conn, enabled, days)
    }

    fn insert(
        &self,
        title: &str,
        description: &str,
        due_at: Option<&str>,
        priority: i32,
        parent_task_id: Option<i64>,
        project_id: Option<i64>,
    ) -> Result<i64> {
        let conn = self.conn.borrow();

        // Get the last sort_key in the Todo column, generate the next key after it
        let last_key: Option<String> = conn
            .query_row(
                "SELECT sort_key FROM tasks
                 WHERE deleted_at IS NULL AND status = 0
                 ORDER BY sort_key DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        let new_key = match last_key {
            Some(ref k) => {
                let last =
                    FractionalIndex::from_string(k).unwrap_or_else(|_| FractionalIndex::default());
                FractionalIndex::new_after(&last)
            }
            None => FractionalIndex::default(),
        };

        conn.execute(
            "INSERT INTO tasks (title, description, status, priority, sort_key, due_at, parent_task_id, project_id, created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7, datetime('now'), datetime('now'))",
            params![
                title,
                description,
                priority,
                new_key.to_string(),
                due_at,
                parent_task_id,
                project_id,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }
}

/// Compute prev and next sort_keys for insertion at `effective_index`.
/// When `filter_source` is true, the task at `source_index` is excluded so its
/// own sort_key doesn't contaminate the computation (same-column moves).
/// Uses direct indexing — no allocation.
pub fn sort_neighbors(
    tasks: &[Task],
    filter_source: bool,
    source_index: i32,
    effective_index: i32,
) -> (Option<String>, Option<String>) {
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
            .map(|t| t.sort_key.clone())
    } else {
        None
    };
    let next = tasks
        .get(index_of(effective_index))
        .map(|t| t.sort_key.clone());
    (prev, next)
}

/// Generate a new sort_key between `prev` and `next` using string fractional indexing.
/// Returns None if the keys cannot be parsed (should not happen with valid DB data).
pub fn new_sort_key_between(prev: Option<&str>, next: Option<&str>) -> Option<String> {
    let prev_fi = prev
        .map(FractionalIndex::from_string)
        .transpose()
        .ok()
        .flatten();
    let next_fi = next
        .map(FractionalIndex::from_string)
        .transpose()
        .ok()
        .flatten();
    Some(FractionalIndex::new(prev_fi.as_ref(), next_fi.as_ref())?.to_string())
}

/// True when the sort_key has grown unreasonably long and the column should be rebalanced.
pub fn sort_key_needs_rebalance(prev: Option<&str>, next: Option<&str>) -> bool {
    let threshold = 100;
    prev.is_some_and(|k| k.len() >= threshold) || next.is_some_and(|k| k.len() >= threshold)
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
        sort_key: &str,
    ) {
        conn.execute(
            "INSERT INTO tasks (id, title, status, priority, sort_key, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), datetime('now'))",
            rusqlite::params![id, title, status, priority, sort_key],
        )
        .unwrap();
    }

    fn insert_task_with_due(
        conn: &Connection,
        id: i64,
        title: &str,
        status: i32,
        sort_key: &str,
        completed_at: Option<&str>,
    ) {
        conn.execute(
            "INSERT INTO tasks (id, title, status, sort_key, due_at, completed_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), datetime('now'))",
            rusqlite::params![
                id,
                title,
                status,
                sort_key,
                completed_at.unwrap_or("2026-12-01"),
                completed_at,
            ],
        )
        .unwrap();
    }

    // ---- new_sort_key_between ----

    #[test]
    fn new_key_empty_column() {
        let key = new_sort_key_between(None, None).unwrap();
        assert!(!key.is_empty());
        // Default key for empty column
        assert_eq!(key, FractionalIndex::default().to_string());
    }

    #[test]
    fn new_key_after_last() {
        let first = FractionalIndex::default().to_string();
        let key = new_sort_key_between(Some(&first), None).unwrap();
        let fi = FractionalIndex::from_string(&key).unwrap();
        let fi_first = FractionalIndex::from_string(&first).unwrap();
        assert!(fi > fi_first);
    }

    #[test]
    fn new_key_before_first() {
        let first = FractionalIndex::default().to_string();
        let key = new_sort_key_between(None, Some(&first)).unwrap();
        let fi = FractionalIndex::from_string(&key).unwrap();
        let fi_first = FractionalIndex::from_string(&first).unwrap();
        assert!(fi < fi_first);
    }

    #[test]
    fn new_key_between() {
        let first = FractionalIndex::default(); // "80"
        let second = FractionalIndex::new_after(&first); // after first
        let mid =
            new_sort_key_between(Some(&first.to_string()), Some(&second.to_string())).unwrap();
        let fi_mid = FractionalIndex::from_string(&mid).unwrap();
        assert!(fi_mid > first);
        assert!(fi_mid < second);
    }

    // ---- sort_key_needs_rebalance ----

    #[test]
    fn no_rebalance_for_normal_keys() {
        assert!(!sort_key_needs_rebalance(Some("80"), Some("8180")));
    }

    #[test]
    fn no_rebalance_when_both_none() {
        assert!(!sort_key_needs_rebalance(None, None));
    }

    #[test]
    fn rebalance_when_prev_too_long() {
        let long_key = "8".repeat(100);
        assert!(sort_key_needs_rebalance(Some(&long_key), None));
    }

    #[test]
    fn rebalance_when_next_too_long() {
        let long_key = "8".repeat(100);
        assert!(sort_key_needs_rebalance(None, Some(&long_key)));
    }

    #[test]
    fn no_rebalance_just_below_threshold() {
        let key = "8".repeat(99);
        assert!(!sort_key_needs_rebalance(Some(&key), None));
    }

    // ---- sort_neighbors ----

    fn make_task(id: i64, sort_key: String) -> Task {
        Task {
            id,
            title: format!("task-{id}"),
            description: String::new(),
            status: TaskStatus::Todo,
            priority: 0,
            sort_key,
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
        let tasks = vec![
            make_task(1, "a".into()),
            make_task(2, "b".into()),
            make_task(3, "c".into()),
        ];
        let (prev, next) = sort_neighbors(&tasks, false, 0, 0);
        assert_eq!(prev, None);
        assert_eq!(next, Some("a".into()));
    }

    #[test]
    fn sort_neighbors_cross_column_insert_at_end() {
        let tasks = vec![
            make_task(1, "a".into()),
            make_task(2, "b".into()),
            make_task(3, "c".into()),
        ];
        let (prev, next) = sort_neighbors(&tasks, false, 0, 3);
        assert_eq!(prev, Some("c".into()));
        assert_eq!(next, None);
    }

    #[test]
    fn sort_neighbors_cross_column_insert_between() {
        let tasks = vec![
            make_task(1, "a".into()),
            make_task(2, "b".into()),
            make_task(3, "c".into()),
        ];
        let (prev, next) = sort_neighbors(&tasks, false, 0, 1);
        assert_eq!(prev, Some("a".into()));
        assert_eq!(next, Some("b".into()));
    }

    #[test]
    fn sort_neighbors_same_column_excludes_source() {
        let tasks = vec![
            make_task(1, "a".into()),
            make_task(2, "b".into()),
            make_task(3, "c".into()),
        ];
        let (prev, next) = sort_neighbors(&tasks, true, 1, 2);
        assert_eq!(prev, Some("c".into())); // C
        assert_eq!(next, None); // end of list
    }

    #[test]
    fn sort_neighbors_same_column_excludes_source_insert_between() {
        let tasks = vec![
            make_task(1, "a".into()),
            make_task(2, "b".into()),
            make_task(3, "c".into()),
        ];
        let (prev, next) = sort_neighbors(&tasks, true, 2, 0);
        assert_eq!(prev, None);
        assert_eq!(next, Some("a".into()));
    }

    #[test]
    fn sort_neighbors_same_column_move_down_one() {
        let tasks = vec![
            make_task(1, "a".into()),
            make_task(2, "b".into()),
            make_task(3, "c".into()),
        ];
        let (prev, next) = sort_neighbors(&tasks, true, 0, 0);
        assert_eq!(prev, None);
        assert_eq!(next, Some("b".into()));
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
            insert_task(&c, 1, "todo-1", 0, 1, "80");
            insert_task(&c, 2, "doing-1", 1, 0, "80");
            insert_task(&c, 3, "todo-2", 0, 2, "8180");
        }
        let todos = repo.load_by_status(TaskStatus::Todo).unwrap();
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].title, "todo-1");
        assert_eq!(todos[1].title, "todo-2");
    }

    #[test]
    fn load_by_status_orders_by_sort_key() {
        let (repo, conn) = setup_in_memory();
        {
            let c = conn.borrow();
            insert_task(&c, 1, "second", 0, 0, "8280");
            insert_task(&c, 2, "first", 0, 0, "80");
            insert_task(&c, 3, "third", 0, 0, "8380");
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
            insert_task(&c, 1, "alive", 0, 0, "80");
            c.execute(
                "INSERT INTO tasks (id, title, status, sort_key, deleted_at, created_at, updated_at)
                 VALUES (2, 'dead', 0, '8180', datetime('now'), datetime('now'), datetime('now'))",
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
            insert_task(&c, 1, "task", 0, 0, "80");
        }
        repo.move_task(1, TaskStatus::Done, "8180").unwrap();
        let tasks = repo.load_by_status(TaskStatus::Done).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].completed_at.is_some());
    }

    #[test]
    fn move_task_from_done_to_todo_clears_completed_at() {
        let (repo, conn) = setup_in_memory();
        {
            let c = conn.borrow();
            insert_task_with_due(&c, 1, "done-task", 2, "80", Some("2026-06-01 10:00:00"));
        }
        let tasks = repo.load_by_status(TaskStatus::Done).unwrap();
        assert!(tasks[0].completed_at.is_some());

        repo.move_task(1, TaskStatus::Todo, "80").unwrap();
        let tasks = repo.load_by_status(TaskStatus::Todo).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].completed_at.is_none());
    }

    #[test]
    fn move_task_updates_sort_key() {
        let (repo, conn) = setup_in_memory();
        {
            let c = conn.borrow();
            insert_task(&c, 1, "task", 0, 0, "80");
        }
        repo.move_task(1, TaskStatus::InProgress, "abc123").unwrap();
        let tasks = repo.load_by_status(TaskStatus::InProgress).unwrap();
        assert_eq!(tasks[0].sort_key, "abc123");
    }

    #[test]
    fn renumber_column_renumbers_in_order() {
        let (repo, conn) = setup_in_memory();
        {
            let c = conn.borrow();
            insert_task(&c, 1, "a", 0, 0, "99");
            insert_task(&c, 2, "b", 0, 0, "17");
            insert_task(&c, 3, "c", 0, 0, "42");
        }
        repo.renumber_column(TaskStatus::Todo).unwrap();
        let tasks = repo.load_by_status(TaskStatus::Todo).unwrap();
        // After renumber, sorted by old sort_key order: b(17), c(42), a(99)
        // New keys: default="80", new_after="8180", new_after="8280"
        assert_eq!(tasks[0].title, "b");
        assert_eq!(tasks[1].title, "c");
        assert_eq!(tasks[2].title, "a");
        // Verify keys are lexicographically ordered
        assert!(tasks[0].sort_key < tasks[1].sort_key);
        assert!(tasks[1].sort_key < tasks[2].sort_key);
    }

    #[test]
    fn renumber_column_empty_does_not_panic() {
        let (repo, _conn) = setup_in_memory();
        repo.renumber_column(TaskStatus::Todo).unwrap();
    }

    #[test]
    fn insert_creates_task_in_todo_column() {
        let (repo, _conn) = setup_in_memory();
        let id = repo
            .insert("new task", "desc", None, 2, None, None)
            .unwrap();
        assert!(id > 0);
        let todos = repo.load_by_status(TaskStatus::Todo).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].title, "new task");
        assert_eq!(todos[0].priority, 2);
    }

    #[test]
    fn insert_sets_sort_key_after_last() {
        let (repo, conn) = setup_in_memory();
        {
            let c = conn.borrow();
            insert_task(&c, 1, "existing", 0, 0, "80");
        }
        repo.insert("new", "", None, 0, None, None).unwrap();
        let todos = repo.load_by_status(TaskStatus::Todo).unwrap();
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].sort_key, "80"); // existing
        // New key should be lexicographically after the old one
        assert!(todos[0].sort_key < todos[1].sort_key);
    }
}
