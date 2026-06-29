UPDATE tasks
SET status = 3,
    archived_at = COALESCE(archived_at, datetime('now')),
    updated_at = datetime('now')
WHERE status = 2
  AND deleted_at IS NULL
  AND completed_at IS NOT NULL
  AND completed_at <= datetime('now', '-7 days');
