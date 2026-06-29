pub mod project;
pub mod tag;
pub mod task;

use std::cell::RefCell;
use std::rc::Rc;

use rusqlite::{Connection, Result};

use self::project::SqliteProjectRepository;
use self::tag::SqliteTagRepository;
use self::task::SqliteTaskRepository;

/// Initialise the database: enable foreign keys, apply schema, auto-archive,
/// and seed sample data if the task table is empty.
/// Returns repository handles that share a single `Rc<RefCell<Connection>>`.
pub fn init(
    path: &str,
) -> Result<(
    SqliteTaskRepository,
    SqliteTagRepository,
    SqliteProjectRepository,
)> {
    let conn = Connection::open(path)?;

    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(include_str!("../sql/schema.sql"))?;
    conn.execute_batch(include_str!("../sql/auto_archive.sql"))?;

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE deleted_at IS NULL",
        [],
        |row| row.get(0),
    )?;
    if count == 0 {
        conn.execute_batch(include_str!("../sql/seed.sql"))?;
    }

    let conn_rc = Rc::new(RefCell::new(conn));
    let task_repo = SqliteTaskRepository::new(conn_rc.clone());
    let tag_repo = SqliteTagRepository::new(conn_rc.clone());
    let project_repo = SqliteProjectRepository::new(conn_rc);

    Ok((task_repo, tag_repo, project_repo))
}
