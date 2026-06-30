/// Full domain entity matching the `projects` table row.
/// Fields are stored but only read when future CRUD UIs are implemented.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub manager: String,
    pub color: Option<String>,
    pub created_by: Option<i64>,
    pub created_at: String,
    pub updated_by: Option<i64>,
    pub updated_at: String,
    pub deleted_by: Option<i64>,
    pub deleted_at: Option<String>,
}
