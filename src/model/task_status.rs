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

#[cfg(test)]
mod tests {
    use super::*;

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
}
