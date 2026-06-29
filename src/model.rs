use chrono::NaiveDate;

/// Mirrors the `status` column in SQLite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TaskStatus {
    Todo = 0,
    InProgress = 1,
    Done = 2,
    Archived = 3,
}

impl TaskStatus {
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            0 => Some(Self::Todo),
            1 => Some(Self::InProgress),
            2 => Some(Self::Done),
            3 => Some(Self::Archived),
            _ => None,
        }
    }
}

/// Full domain entity matching the `tasks` table row.
/// Many fields are stored but only read when future CRUD UIs are implemented.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: i32,
    pub sort_order: f64,
    pub due_at: Option<NaiveDate>,
    pub reminder_at: Option<String>,
    pub parent_task_id: Option<i64>,
    pub project_id: Option<i64>,
    pub assignee_id: Option<i64>,
    pub completed_at: Option<String>,
    pub creator_id: Option<i64>,
    pub created_at: String,
    pub updater_id: Option<i64>,
    pub updated_at: String,
    pub deleter_id: Option<i64>,
    pub deleted_at: Option<String>,
    pub archiver_id: Option<i64>,
    pub archived_at: Option<String>,
}

impl Task {
    /// True when `due_at` is before today and the task isn't Done/Archived.
    pub fn is_overdue(&self) -> bool {
        if let Some(due) = self.due_at
            && self.status != TaskStatus::Done
            && self.status != TaskStatus::Archived
        {
            let today = chrono::Local::now().date_naive();
            return due < today;
        }
        false
    }

    /// Convert to the lightweight UI card consumed by Slint conversion.
    pub fn to_card_data(&self, tags: Vec<String>) -> TaskCardData {
        let due_text = self
            .due_at
            .map(|d| d.format("%-m/%-d").to_string())
            .unwrap_or_default();
        TaskCardData {
            id: self.id.to_string(),
            title: self.title.clone(),
            priority: self.priority,
            due_text,
            is_overdue: self.is_overdue(),
            tags,
        }
    }
}

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
