use perch_core::models::{SourceResult, TaskSnapshot, TodoSnapshot};
use perch_core::query::build_bootstrap;
use perch_core::store::CacheStore;

fn make_task(source: &str, id: &str, workspace: &str, updated_at: f64) -> TaskSnapshot {
    let mut task = TaskSnapshot::new(source, id);
    task.title = format!("任务 {id}");
    task.title_source = "source".to_owned();
    task.project_path = Some(format!("/Users/x/work/{workspace}"));
    task.project_name = Some(workspace.to_owned());
    task.created_at = Some(updated_at - 3600.0);
    task.updated_at = Some(updated_at);
    task.status_source = Some("idle".to_owned());
    task.refresh_fingerprint();
    task
}

#[test]
fn missing_detection_only_marks_absent_tasks_after_complete_scan() {
    let store = CacheStore::open_in_memory().unwrap();
    let first_scan = store.begin_scan("qoder", "full", "source", "test").unwrap();
    let tasks = vec![
        make_task("qoder", "a", "alpha", 1_000.0),
        make_task("qoder", "b", "alpha", 1_000.0),
        make_task("qoder", "c", "beta", 1_000.0),
    ];
    store
        .save_result(&SourceResult::ok("qoder", tasks, Vec::new()), first_scan)
        .unwrap();
    assert_eq!(store.mark_missing("qoder", first_scan).unwrap(), 0);

    let second_scan = store.begin_scan("qoder", "full", "source", "test").unwrap();
    store
        .save_result(
            &SourceResult::ok(
                "qoder",
                vec![make_task("qoder", "a", "alpha", 2_000.0)],
                Vec::new(),
            ),
            second_scan,
        )
        .unwrap();
    assert_eq!(store.mark_missing("qoder", second_scan).unwrap(), 2);

    let loaded = store.load_tasks().unwrap();
    let a = loaded.iter().find(|task| task.source_id == "a").unwrap();
    let b = loaded.iter().find(|task| task.source_id == "b").unwrap();
    assert!(!a.source_missing);
    assert!(b.source_missing);
    assert_eq!(a.updated_at, Some(2_000.0));

    let third_scan = store.begin_scan("qoder", "full", "source", "test").unwrap();
    store
        .save_result(
            &SourceResult::ok(
                "qoder",
                vec![
                    make_task("qoder", "a", "alpha", 3_000.0),
                    make_task("qoder", "b", "alpha", 3_000.0),
                ],
                Vec::new(),
            ),
            third_scan,
        )
        .unwrap();
    assert_eq!(store.mark_missing("qoder", third_scan).unwrap(), 0);

    let loaded = store.load_tasks().unwrap();
    let b = loaded.iter().find(|task| task.source_id == "b").unwrap();
    let c = loaded.iter().find(|task| task.source_id == "c").unwrap();
    assert!(!b.source_missing, "seen task should be restored");
    assert!(c.source_missing, "absent task stays missing");
}

#[test]
fn tasks_without_project_share_a_no_project_group() {
    let store = CacheStore::open_in_memory().unwrap();
    let now = 1_800_000_000.0;
    let scan = store.begin_scan("qoder", "full", "source", "test").unwrap();

    let mut orphan = make_task("qoder", "orphan", "alpha", now - 10.0);
    orphan.project_path = None;
    orphan.project_name = None;
    let normal = make_task("qoder", "normal", "alpha", now - 20.0);
    store
        .save_result(
            &SourceResult::ok("qoder", vec![orphan, normal], Vec::new()),
            scan,
        )
        .unwrap();

    let payload = build_bootstrap(&store, now).unwrap();
    let group = payload
        .workspaces
        .iter()
        .find(|group| group.key == "__no_project__")
        .unwrap();
    assert_eq!(group.name, "未关联项目");
    assert_eq!(group.project_path, None);
    assert_eq!(group.tasks.len(), 1);
    assert_eq!(group.tasks[0].source_id, "orphan");
    assert_eq!(group.tasks[0].workspace_name, "未关联项目");
}

#[test]
fn bootstrap_groups_by_workspace_and_splits_recent_older() {
    let store = CacheStore::open_in_memory().unwrap();
    let now = 1_800_000_000.0;
    let scan = store.begin_scan("qoder", "full", "source", "test").unwrap();

    let mut recent = make_task("qoder", "recent", "alpha", now - 100.0);
    recent.source_unread = true;
    let older = make_task("qoder", "older", "alpha", now - 10.0 * 86400.0);
    let mut archived = make_task("qoder", "archived", "beta", now - 200.0);
    archived.source_archived = true;
    archived.status_source = Some("archived".to_owned());
    let mut child = make_task("qoder", "child", "alpha", now - 50.0);
    child.parent_key = Some("qoder:recent".to_owned());
    let mut automation = make_task("claude", "automation", "alpha", now - 30.0);
    automation.agent_kind = Some("automation".to_owned());

    let todo_done = TodoSnapshot {
        task_key: "qoder:recent".to_owned(),
        item_id: "t1".to_owned(),
        subject: "已完成".to_owned(),
        status: "completed".to_owned(),
        position: 0,
    };
    let mut todo_pending = todo_done.clone();
    todo_pending.item_id = "t2".to_owned();
    todo_pending.subject = "待办".to_owned();
    todo_pending.status = "pending".to_owned();
    todo_pending.position = 1;

    store
        .save_result(
            &SourceResult::ok(
                "qoder",
                vec![recent, older, archived, child, automation],
                vec![todo_done, todo_pending],
            ),
            scan,
        )
        .unwrap();

    let payload = build_bootstrap(&store, now).unwrap();

    assert_eq!(
        payload.counts.total, 2,
        "children and automation are excluded"
    );
    assert_eq!(payload.counts.recent, 1);
    assert_eq!(payload.counts.older, 1);
    assert_eq!(payload.counts.archived, 1);
    assert_eq!(payload.counts.attention, 1);

    let alpha = payload
        .workspaces
        .iter()
        .find(|group| group.name == "alpha")
        .unwrap();
    assert_eq!(alpha.project_path.as_deref(), Some("/Users/x/work/alpha"));
    assert_eq!(alpha.tasks.len(), 4);
    let recent = alpha
        .tasks
        .iter()
        .find(|task| task.source_id == "recent")
        .unwrap();
    assert!(recent.is_recent);
    assert_eq!(recent.todo_total, 2);
    assert_eq!(recent.todo_done, 1);
    assert_eq!(recent.todos.len(), 2);
    assert!(recent.source_unread);

    let older = alpha
        .tasks
        .iter()
        .find(|task| task.source_id == "older")
        .unwrap();
    assert!(!older.is_recent);
    let child = alpha
        .tasks
        .iter()
        .find(|task| task.source_id == "child")
        .unwrap();
    assert!(child.is_child);

    let beta = payload
        .workspaces
        .iter()
        .find(|group| group.name == "beta")
        .unwrap();
    assert_eq!(beta.tasks.len(), 1);
    assert!(beta.tasks[0].source_archived);

    let qoder = payload
        .sources
        .iter()
        .find(|source| source.source == "qoder")
        .unwrap();
    assert_eq!(qoder.task_count, 4);
}
