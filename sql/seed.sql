-- Sample tags
INSERT INTO tags (id, name, color) VALUES
    (1, 'ui',     '#58a6ff'),
    (2, 'bug',    '#f85149'),
    (3, 'docs',   '#3fb950'),
    (4, 'test',   '#d29922'),
    (5, 'chore',  '#8b949e'),
    (6, 'auth',   '#bc8cff');

-- Todo column (status = 0)
INSERT INTO tasks (id, title, description, status, priority, sort_order, due_at, created_at, updated_at) VALUES
    (1, '重构登录页面',       '使用 Slint 新组件重写登录页面',              0, 2, 1000.0, '2026-07-05', datetime('now'), datetime('now')),
    (2, '编写单元测试',       '为核心模块补充测试覆盖',                      0, 0, 2000.0, '2026-07-10', datetime('now'), datetime('now')),
    (3, '设计系统颜色方案',   '定义明暗主题色板',                            0, 1, 3000.0, NULL,         datetime('now'), datetime('now')),
    (4, '修复按钮样式错位',   'Firefox 下提交按钮偏移 3px',                  0, 1, 4000.0, '2026-06-30', datetime('now'), datetime('now'));

-- In Progress column (status = 1)
INSERT INTO tasks (id, title, description, status, priority, sort_order, due_at, created_at, updated_at) VALUES
    (5, '修复内存泄漏',       '长时间运行后 RSS 持续增长',                   1, 0, 1000.0, '2026-06-28', datetime('now'), datetime('now')),
    (6, '实现拖拽上传',       '支持拖拽文件到上传区域',                      1, 2, 2000.0, '2026-07-03', datetime('now'), datetime('now'));

-- Done column (status = 2)
INSERT INTO tasks (id, title, description, status, priority, sort_order, due_at, completed_at, created_at, updated_at) VALUES
    (7, '升级到 Slint 1.17',  '完成 Slint 版本升级及 API 迁移',              2, 2, 1000.0, '2026-06-20', datetime('now', '-2 days'),  datetime('now'), datetime('now')),
    (8, '更新 API 文档',      '补充新增接口的文档和示例',                    2, 1, 2000.0, '2026-06-15', datetime('now', '-5 days'),  datetime('now'), datetime('now')),
    (9, '发布 v1.0 版本',     '完成发布前检查和打包',                        2, 0, 3000.0, '2026-06-10', datetime('now', '-10 days'), datetime('now'), datetime('now'));

-- Tag assignments
INSERT INTO task_tags (tag_id, task_id) VALUES
    (1, 1), (6, 1),
    (4, 2),
    (1, 3),
    (1, 4), (2, 4),
    (2, 5),
    (1, 6),
    (5, 7),
    (3, 8),
    (5, 9);
