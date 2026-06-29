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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    // ---- TaskStatus::from_i32 ----

    #[test]
    fn from_i32_valid_values() {
        assert_eq!(TaskStatus::from_i32(0), Some(TaskStatus::Todo));
        assert_eq!(TaskStatus::from_i32(1), Some(TaskStatus::InProgress));
        assert_eq!(TaskStatus::from_i32(2), Some(TaskStatus::Done));
        assert_eq!(TaskStatus::from_i32(3), Some(TaskStatus::Archived));
    }

    #[test]
    fn from_i32_invalid_values() {
        assert_eq!(TaskStatus::from_i32(-1), None);
        assert_eq!(TaskStatus::from_i32(4), None);
        assert_eq!(TaskStatus::from_i32(100), None);
    }

    // ---- Task::is_overdue ----

    fn task_with_due(due_days_from_today: i64) -> Task {
        let today = Local::now().date_naive();
        let due = today + chrono::Duration::days(due_days_from_today);
        Task {
            id: 1,
            title: "test".into(),
            description: String::new(),
            status: TaskStatus::Todo,
            priority: 1,
            sort_order: 1000.0,
            due_at: Some(due),
            reminder_at: None,
            parent_task_id: None,
            project_id: None,
            assignee_id: None,
            completed_at: None,
            creator_id: None,
            created_at: String::new(),
            updater_id: None,
            updated_at: String::new(),
            deleter_id: None,
            deleted_at: None,
            archiver_id: None,
            archived_at: None,
        }
    }

    #[test]
    fn is_overdue_when_due_yesterday_and_todo() {
        let task = task_with_due(-1);
        assert!(task.is_overdue());
    }

    #[test]
    fn is_not_overdue_when_due_tomorrow() {
        let task = task_with_due(1);
        assert!(!task.is_overdue());
    }

    #[test]
    fn is_not_overdue_when_due_today() {
        let task = task_with_due(0);
        assert!(!task.is_overdue());
    }

    #[test]
    fn is_not_overdue_when_no_due_date() {
        let mut task = task_with_due(0);
        task.due_at = None;
        assert!(!task.is_overdue());
    }

    #[test]
    fn done_task_never_overdue_even_if_due_passed() {
        let mut task = task_with_due(-5);
        task.status = TaskStatus::Done;
        assert!(!task.is_overdue());
    }

    #[test]
    fn archived_task_never_overdue() {
        let mut task = task_with_due(-5);
        task.status = TaskStatus::Archived;
        assert!(!task.is_overdue());
    }

    #[test]
    fn in_progress_task_is_overdue_if_due_passed() {
        let mut task = task_with_due(-1);
        task.status = TaskStatus::InProgress;
        assert!(task.is_overdue());
    }

    // ---- Task::to_card_data ----

    #[test]
    fn to_card_data_converts_all_fields() {
        let today = Local::now().date_naive();
        let due = today + chrono::Duration::days(5);
        let task = Task {
            id: 42,
            title: "测试任务".into(),
            description: String::new(),
            status: TaskStatus::Todo,
            priority: 2,
            sort_order: 2000.0,
            due_at: Some(due),
            reminder_at: None,
            parent_task_id: None,
            project_id: None,
            assignee_id: None,
            completed_at: None,
            creator_id: None,
            created_at: String::new(),
            updater_id: None,
            updated_at: String::new(),
            deleter_id: None,
            deleted_at: None,
            archiver_id: None,
            archived_at: None,
        };

        let card = task.to_card_data(vec!["ui".into(), "bug".into()]);

        assert_eq!(card.id, "42");
        assert_eq!(card.title, "测试任务");
        assert_eq!(card.priority, 2);
        assert!(!card.due_text.is_empty()); // formatted date
        assert!(!card.is_overdue); // due is in the future
        assert_eq!(card.tags, vec!["ui", "bug"]);
    }

    #[test]
    fn to_card_data_without_due_date() {
        let task = Task {
            id: 1,
            title: "no due".into(),
            description: String::new(),
            status: TaskStatus::Todo,
            priority: 0,
            sort_order: 1000.0,
            due_at: None,
            reminder_at: None,
            parent_task_id: None,
            project_id: None,
            assignee_id: None,
            completed_at: None,
            creator_id: None,
            created_at: String::new(),
            updater_id: None,
            updated_at: String::new(),
            deleter_id: None,
            deleted_at: None,
            archiver_id: None,
            archived_at: None,
        };

        let card = task.to_card_data(vec![]);
        assert_eq!(card.due_text, "");
        assert!(!card.is_overdue);
    }

    #[test]
    fn to_card_data_with_overdue_due_date() {
        let yesterday = Local::now().date_naive() - chrono::Duration::days(1);
        let task = Task {
            id: 1,
            title: "overdue task".into(),
            description: String::new(),
            status: TaskStatus::Todo,
            priority: 0,
            sort_order: 1000.0,
            due_at: Some(yesterday),
            reminder_at: None,
            parent_task_id: None,
            project_id: None,
            assignee_id: None,
            completed_at: None,
            creator_id: None,
            created_at: String::new(),
            updater_id: None,
            updated_at: String::new(),
            deleter_id: None,
            deleted_at: None,
            archiver_id: None,
            archived_at: None,
        };

        let card = task.to_card_data(vec![]);
        assert!(card.is_overdue);
        assert!(!card.due_text.is_empty());
    }
}
