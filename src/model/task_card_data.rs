/// Platform-independent card data. Converted to Slint's `TaskCardUi` in main.rs.
#[derive(Debug, Clone)]
pub struct TaskCardData {
    pub id: String,
    pub title: String,
    pub priority: i32,
    pub due_text: String,
    pub is_overdue: bool,
    pub tags: Vec<String>,
}
