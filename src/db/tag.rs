use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rusqlite::{Connection, Result};

pub trait TagRepository {
    /// Load tag names for a batch of task IDs.
    /// Returns a map from task_id to its tag name list.
    fn load_for_tasks(&self, task_ids: &[i64]) -> Result<HashMap<i64, Vec<String>>>;
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
}
