use std::cell::RefCell;
use std::rc::Rc;

use rusqlite::{params, Connection, Result};

use crate::model::Project;

pub trait ProjectRepository {
    fn insert(&self, name: &str, description: &str, manager: &str, color: Option<&str>) -> Result<i64>;
    fn load_all(&self) -> Result<Vec<Project>>;
}

pub struct SqliteProjectRepository {
    conn: Rc<RefCell<Connection>>,
}

impl SqliteProjectRepository {
    pub fn new(conn: Rc<RefCell<Connection>>) -> Self {
        Self { conn }
    }
}

impl ProjectRepository for SqliteProjectRepository {
    fn insert(&self, name: &str, description: &str, manager: &str, color: Option<&str>) -> Result<i64> {
        let conn = self.conn.borrow();
        conn.execute(
            "INSERT INTO projects (name, description, manager, color, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'), datetime('now'))",
            params![name, description, manager, color],
        )?;
        Ok(conn.last_insert_rowid())
    }

    fn load_all(&self) -> Result<Vec<Project>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, manager, color, created_by, created_at, updated_by, updated_at, deleted_by, deleted_at
             FROM projects
             WHERE deleted_at IS NULL
             ORDER BY name ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                manager: row.get(3)?,
                color: row.get(4)?,
                created_by: row.get(5)?,
                created_at: row.get(6)?,
                updated_by: row.get(7)?,
                updated_at: row.get(8)?,
                deleted_by: row.get(9)?,
                deleted_at: row.get(10)?,
            })
        })?;

        rows.collect()
    }
}
