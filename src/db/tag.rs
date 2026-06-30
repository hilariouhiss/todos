use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rusqlite::{Connection, Result, params};

pub trait TagRepository {
    /// Load tag names for a batch of task IDs.
    /// Returns a map from task_id to its tag name list.
    fn load_for_tasks(&self, task_ids: &[i64]) -> Result<HashMap<i64, Vec<String>>>;
    fn insert(&self, name: &str, color: Option<&str>) -> Result<i64>;
}

pub struct SqliteTagRepository {
    conn: Rc<RefCell<Connection>>,
}

impl SqliteTagRepository {
    pub fn new(conn: Rc<RefCell<Connection>>) -> Self {
        Self { conn }
    }
}

impl TagRepository for SqliteTagRepository {
    fn load_for_tasks(&self, task_ids: &[i64]) -> Result<HashMap<i64, Vec<String>>> {
        if task_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let conn = self.conn.borrow();
        let placeholders: Vec<String> = task_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT tt.task_id, t.name
             FROM task_tags tt
             JOIN tags t ON t.id = tt.tag_id
             WHERE tt.task_id IN ({})
             ORDER BY tt.task_id, t.name",
            placeholders.join(",")
        );

        let params: Vec<&dyn rusqlite::types::ToSql> = task_ids
            .iter()
            .map(|id| id as &dyn rusqlite::types::ToSql)
            .collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params.as_slice(), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut result: HashMap<i64, Vec<String>> = HashMap::new();
        for row in rows {
            let (task_id, tag_name) = row?;
            result.entry(task_id).or_default().push(tag_name);
        }
        Ok(result)
    }

    fn insert(&self, name: &str, color: Option<&str>) -> Result<i64> {
        let conn = self.conn.borrow();
        conn.execute(
            "INSERT INTO tags (name, color) VALUES (?1, ?2)",
            params![name, color],
        )?;
        Ok(conn.last_insert_rowid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    use rusqlite::Connection;

    fn setup_in_memory() -> (SqliteTagRepository, Rc<RefCell<Connection>>) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!("../../sql/schema.sql"))
            .unwrap();

        // Insert sample tags and task_tags
        conn.execute_batch(
            "INSERT INTO tags (id, name) VALUES (1, 'ui'), (2, 'bug'), (3, 'docs');
             INSERT INTO tasks (id, title, status, sort_key, created_at, updated_at)
             VALUES (1, 't1', 0, '80', datetime('now'), datetime('now'));
             INSERT INTO tasks (id, title, status, sort_key, created_at, updated_at)
             VALUES (2, 't2', 0, '8180', datetime('now'), datetime('now'));
             INSERT INTO tasks (id, title, status, sort_key, created_at, updated_at)
             VALUES (3, 't3', 0, '8280', datetime('now'), datetime('now'));
             INSERT INTO task_tags (tag_id, task_id) VALUES (1, 1), (2, 1), (2, 2);",
        )
        .unwrap();

        let conn_rc = Rc::new(RefCell::new(conn));
        let repo = SqliteTagRepository::new(conn_rc.clone());
        (repo, conn_rc)
    }

    #[test]
    fn load_for_tasks_empty_input() {
        let (repo, _conn) = setup_in_memory();
        let result = repo.load_for_tasks(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn load_for_tasks_single_task_with_tags() {
        let (repo, _conn) = setup_in_memory();
        let result = repo.load_for_tasks(&[1]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get(&1).unwrap(),
            &vec!["bug".to_string(), "ui".to_string()]
        );
    }

    #[test]
    fn load_for_tasks_single_task_with_one_tag() {
        let (repo, _conn) = setup_in_memory();
        let result = repo.load_for_tasks(&[2]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.get(&2).unwrap(), &vec!["bug".to_string()]);
    }

    #[test]
    fn load_for_tasks_task_with_no_tags() {
        let (repo, _conn) = setup_in_memory();
        let result = repo.load_for_tasks(&[3]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn load_for_tasks_multiple_tasks() {
        let (repo, _conn) = setup_in_memory();
        let result = repo.load_for_tasks(&[1, 2, 3]).unwrap();
        assert_eq!(result.len(), 2); // Only task 1 and 2 have tags
        assert_eq!(result.get(&1).unwrap().len(), 2);
        assert_eq!(result.get(&2).unwrap().len(), 1);
    }

    #[test]
    fn load_for_tasks_orders_tags_alphabetically() {
        let (repo, _conn) = setup_in_memory();
        let result = repo.load_for_tasks(&[1]).unwrap();
        let tags = result.get(&1).unwrap();
        assert_eq!(tags[0], "bug");
        assert_eq!(tags[1], "ui");
    }

    #[test]
    fn load_for_tasks_nonexistent_ids() {
        let (repo, _conn) = setup_in_memory();
        let result = repo.load_for_tasks(&[999]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn insert_creates_tag() {
        let (repo, _conn) = setup_in_memory();
        let id = repo.insert("new-tag", Some("#ff0000")).unwrap();
        assert!(id > 0);
    }
}
