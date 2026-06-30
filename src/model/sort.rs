use std::cmp::Ordering;

use pinyin::ToPinyin;
use serde::{Deserialize, Serialize};

use super::Task;

/// Sort field options for a kanban column.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    /// Preserve the DB sort_key order (drag-and-drop manual ordering).
    Manual = 0,
    /// Sort by priority (P0 highest → P3 lowest).
    #[default]
    Priority = 1,
    /// Sort by due date.
    DueDate = 2,
    /// Sort by title, case-insensitive and Chinese-pinyin-aware.
    Title = 3,
}

impl SortField {
    /// Convert from the i32 stored in Slint ThemeSettings globals.
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Manual,
            1 => Self::Priority,
            2 => Self::DueDate,
            3 => Self::Title,
            _ => Self::default(),
        }
    }

    /// Convert to i32 for the Slint ThemeSettings global.
    pub fn to_i32(self) -> i32 {
        self as i32
    }
}

/// Sort configuration for one kanban column.
///
/// `direction` is `true` for ascending, `false` for descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortConfig {
    pub field: SortField,
    /// `true` = ascending, `false` = descending.
    #[serde(default = "default_direction")]
    pub direction: bool,
}

fn default_direction() -> bool {
    true
}

impl Default for SortConfig {
    fn default() -> Self {
        Self {
            field: SortField::Priority,
            direction: true,
        }
    }
}

// ---- Sort logic ----

/// Sort tasks in-place according to `config`.
///
/// For `Manual` mode, tasks are assumed to already be in DB `sort_key` order
/// (the DB query appends `ORDER BY sort_key ASC`). Descending simply reverses.
pub fn sort_tasks(tasks: &mut [Task], config: SortConfig) {
    // No-op: Manual + Ascending preserves DB sort_key order.
    if config.field == SortField::Manual && config.direction {
        return;
    }
    let ascending = config.direction;

    match config.field {
        SortField::Manual => {
            tasks.reverse();
        }
        SortField::Priority => {
            tasks.sort_by(|a, b| with_dir(a.priority.cmp(&b.priority), ascending));
        }
        SortField::DueDate => {
            tasks.sort_by(|a, b| {
                // None always sorts last regardless of direction
                match (a.due_at, b.due_at) {
                    (None, None) => Ordering::Equal,
                    (None, Some(_)) => Ordering::Greater,
                    (Some(_), None) => Ordering::Less,
                    (Some(da), Some(db)) => with_dir(da.cmp(&db), ascending),
                }
            });
        }
        SortField::Title => {
            if ascending {
                tasks.sort_by_cached_key(|t| pinyin_sort_key(&t.title));
            } else {
                tasks.sort_by_cached_key(|t| std::cmp::Reverse(pinyin_sort_key(&t.title)));
            }
        }
    }
}

/// Flip comparison result when descending.
fn with_dir(ord: Ordering, ascending: bool) -> Ordering {
    if ascending { ord } else { ord.reverse() }
}

/// Build a case-insensitive, pinyin-aware sort key from a title string.
///
/// Chinese characters are converted to their tone-less pinyin (e.g. "张" → "zhang").
/// Non-Chinese characters are lowercased. This produces a flat ASCII key suitable
/// for `str::cmp`-based sorting.
fn pinyin_sort_key(s: &str) -> String {
    let mut result = String::new();
    for (ch, py) in s.chars().zip(s.to_pinyin()) {
        if let Some(py) = py {
            result.push_str(py.plain());
        } else {
            result.extend(ch.to_lowercase());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TaskStatus;

    fn make_task(id: i64, title: &str, priority: i32, sort_key: &str) -> Task {
        Task {
            id,
            title: title.into(),
            description: String::new(),
            status: TaskStatus::Todo,
            priority,
            sort_key: sort_key.into(),
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

    fn make_task_with_due(id: i64, title: &str, priority: i32, due_days_from_today: i64) -> Task {
        let today = chrono::Local::now().date_naive();
        let due = today + chrono::Duration::days(due_days_from_today);
        Task {
            due_at: Some(due),
            ..make_task(id, title, priority, "80")
        }
    }

    fn make_task_no_due(id: i64, title: &str, priority: i32) -> Task {
        Task {
            due_at: None,
            ..make_task(id, title, priority, "80")
        }
    }

    // ---- SortField ----

    #[test]
    fn sort_field_from_i32_valid() {
        assert_eq!(SortField::from_i32(0), SortField::Manual);
        assert_eq!(SortField::from_i32(1), SortField::Priority);
        assert_eq!(SortField::from_i32(2), SortField::DueDate);
        assert_eq!(SortField::from_i32(3), SortField::Title);
    }

    #[test]
    fn sort_field_from_i32_invalid_defaults_to_priority() {
        assert_eq!(SortField::from_i32(99), SortField::Priority);
        assert_eq!(SortField::from_i32(-1), SortField::Priority);
    }

    #[test]
    fn sort_field_to_i32_roundtrip() {
        for f in [
            SortField::Manual,
            SortField::Priority,
            SortField::DueDate,
            SortField::Title,
        ] {
            assert_eq!(SortField::from_i32(f.to_i32()), f);
        }
    }

    // ---- sort_tasks: empty / single ----

    #[test]
    fn sort_empty_does_not_panic() {
        let mut tasks: Vec<Task> = vec![];
        sort_tasks(&mut tasks, SortConfig::default());
        assert!(tasks.is_empty());
    }

    #[test]
    fn sort_single_unchanged() {
        let mut tasks = vec![make_task(1, "test", 2, "80")];
        sort_tasks(&mut tasks, SortConfig::default());
        assert_eq!(tasks[0].id, 1);
    }

    // ---- sort_tasks: Manual ----

    #[test]
    fn manual_asc_preserves_order() {
        let mut tasks = vec![
            make_task(1, "a", 0, "80"),
            make_task(2, "b", 0, "8180"),
            make_task(3, "c", 0, "8280"),
        ];
        sort_tasks(
            &mut tasks,
            SortConfig {
                field: SortField::Manual,
                direction: true,
            },
        );
        assert_eq!(tasks[0].id, 1);
        assert_eq!(tasks[1].id, 2);
        assert_eq!(tasks[2].id, 3);
    }

    #[test]
    fn manual_desc_reverses_order() {
        let mut tasks = vec![
            make_task(1, "a", 0, "80"),
            make_task(2, "b", 0, "8180"),
            make_task(3, "c", 0, "8280"),
        ];
        sort_tasks(
            &mut tasks,
            SortConfig {
                field: SortField::Manual,
                direction: false,
            },
        );
        assert_eq!(tasks[0].id, 3);
        assert_eq!(tasks[1].id, 2);
        assert_eq!(tasks[2].id, 1);
    }

    // ---- sort_tasks: Priority ----

    #[test]
    fn priority_asc_highest_first() {
        let mut tasks = vec![
            make_task(1, "low", 3, "80"),
            make_task(2, "high", 0, "8180"),
            make_task(3, "mid", 1, "8280"),
        ];
        sort_tasks(
            &mut tasks,
            SortConfig {
                field: SortField::Priority,
                direction: true,
            },
        );
        // P0 before P1 before P3
        assert_eq!(tasks[0].priority, 0);
        assert_eq!(tasks[1].priority, 1);
        assert_eq!(tasks[2].priority, 3);
    }

    #[test]
    fn priority_desc_lowest_first() {
        let mut tasks = vec![
            make_task(1, "low", 3, "80"),
            make_task(2, "high", 0, "8180"),
            make_task(3, "mid", 1, "8280"),
        ];
        sort_tasks(
            &mut tasks,
            SortConfig {
                field: SortField::Priority,
                direction: false,
            },
        );
        assert_eq!(tasks[0].priority, 3);
        assert_eq!(tasks[1].priority, 1);
        assert_eq!(tasks[2].priority, 0);
    }

    // ---- sort_tasks: DueDate ----

    #[test]
    fn due_date_asc_earliest_first_none_last() {
        let mut tasks = vec![
            make_task_no_due(1, "no-due", 0),
            make_task_with_due(2, "later", 0, 5),
            make_task_with_due(3, "sooner", 0, -2),
        ];
        sort_tasks(
            &mut tasks,
            SortConfig {
                field: SortField::DueDate,
                direction: true,
            },
        );
        // Sooner (-2 days) first, then later (+5), then no-due (None) last
        assert_eq!(tasks[0].id, 3); // soonest
        assert_eq!(tasks[1].id, 2); // later
        assert_eq!(tasks[2].id, 1); // no due — always last
    }

    #[test]
    fn due_date_desc_latest_first_none_still_last() {
        let mut tasks = vec![
            make_task_no_due(1, "no-due", 0),
            make_task_with_due(2, "later", 0, 5),
            make_task_with_due(3, "sooner", 0, -2),
        ];
        sort_tasks(
            &mut tasks,
            SortConfig {
                field: SortField::DueDate,
                direction: false,
            },
        );
        // Latest first: later (+5), then sooner (-2), then no-due (None) last
        assert_eq!(tasks[0].id, 2); // latest
        assert_eq!(tasks[1].id, 3); // sooner
        assert_eq!(tasks[2].id, 1); // no due — always last
    }

    #[test]
    fn due_date_all_none_preserves_order() {
        let mut tasks = vec![make_task_no_due(1, "a", 0), make_task_no_due(2, "b", 0)];
        sort_tasks(
            &mut tasks,
            SortConfig {
                field: SortField::DueDate,
                direction: true,
            },
        );
        assert_eq!(tasks[0].id, 1);
        assert_eq!(tasks[1].id, 2);
    }

    // ---- sort_tasks: Title ----

    #[test]
    fn title_asc_case_insensitive() {
        let mut tasks = vec![
            make_task(1, "Banana", 0, "80"),
            make_task(2, "apple", 0, "8180"),
            make_task(3, "Cherry", 0, "8280"),
        ];
        sort_tasks(
            &mut tasks,
            SortConfig {
                field: SortField::Title,
                direction: true,
            },
        );
        assert_eq!(tasks[0].title, "apple");
        assert_eq!(tasks[1].title, "Banana");
        assert_eq!(tasks[2].title, "Cherry");
    }

    #[test]
    fn title_desc_case_insensitive() {
        let mut tasks = vec![
            make_task(1, "Banana", 0, "80"),
            make_task(2, "apple", 0, "8180"),
            make_task(3, "Cherry", 0, "8280"),
        ];
        sort_tasks(
            &mut tasks,
            SortConfig {
                field: SortField::Title,
                direction: false,
            },
        );
        assert_eq!(tasks[0].title, "Cherry");
        assert_eq!(tasks[1].title, "Banana");
        assert_eq!(tasks[2].title, "apple");
    }

    #[test]
    fn title_chinese_pinyin_order() {
        // 张 (zhang) < 李 (li) < 王 (wang)
        let mut tasks = vec![
            make_task(1, "王五", 0, "80"),
            make_task(2, "张三", 0, "8180"),
            make_task(3, "李四", 0, "8280"),
        ];
        sort_tasks(
            &mut tasks,
            SortConfig {
                field: SortField::Title,
                direction: true,
            },
        );
        // li < wang < zhang → 李四 < 王五 < 张三
        assert_eq!(tasks[0].title, "李四");
        assert_eq!(tasks[1].title, "王五");
        assert_eq!(tasks[2].title, "张三");
    }

    #[test]
    fn title_mixed_chinese_and_ascii() {
        let mut tasks = vec![
            make_task(1, "张三", 0, "80"),
            make_task(2, "alice", 0, "8180"),
            make_task(3, "Bob", 0, "8280"),
        ];
        sort_tasks(
            &mut tasks,
            SortConfig {
                field: SortField::Title,
                direction: true,
            },
        );
        // alice < bob < zhangsan
        assert_eq!(tasks[0].title, "alice");
        assert_eq!(tasks[1].title, "Bob");
        assert_eq!(tasks[2].title, "张三");
    }

    // ---- pinyin_sort_key ----

    #[test]
    fn pinyin_sort_key_ascii_lowercase() {
        let key = pinyin_sort_key("HelloWorld");
        assert_eq!(key, "helloworld");
    }

    #[test]
    fn pinyin_sort_key_chinese() {
        let key = pinyin_sort_key("张三");
        assert_eq!(key, "zhangsan");
    }

    #[test]
    fn pinyin_sort_key_mixed() {
        let key = pinyin_sort_key("Hello张三");
        assert!(key.starts_with("hello"));
        assert!(key.ends_with("zhangsan"));
    }

    #[test]
    fn pinyin_sort_key_empty() {
        let key = pinyin_sort_key("");
        assert!(key.is_empty());
    }
}
