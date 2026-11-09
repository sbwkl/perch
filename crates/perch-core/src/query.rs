use std::collections::HashMap;

use crate::error::Result;
use crate::models::{
    source_label, BootstrapPayload, Counts, SourceSyncState, SourceView, TaskSnapshot, TaskView,
    TodoSnapshot, TodoView, WorkspaceGroup, ALL_SOURCES, RECENT_WINDOW_SECONDS,
};
use crate::store::CacheStore;
use crate::usage::{UsageOverview, UsageSnapshot};

pub fn build_bootstrap(store: &CacheStore, now: f64) -> Result<BootstrapPayload> {
    build_bootstrap_with_usage(store, now, None)
}

pub fn build_bootstrap_with_usage(
    store: &CacheStore,
    now: f64,
    usage: Option<&UsageSnapshot>,
) -> Result<BootstrapPayload> {
    let tasks = store.load_tasks()?;
    let todos = store.load_todos()?;
    let states = store.load_sync_states()?;

    let mut todos_by_task: HashMap<String, Vec<TodoView>> = HashMap::new();
    for todo in todos {
        let task_key = todo.task_key.clone();
        todos_by_task
            .entry(task_key)
            .or_default()
            .push(todo_view(todo));
    }

    let mut groups: HashMap<String, WorkspaceGroup> = HashMap::new();
    for task in &tasks {
        let view = task_view(task, &mut todos_by_task, now);
        let group = groups
            .entry(view.workspace_key.clone())
            .or_insert_with(|| WorkspaceGroup {
                key: view.workspace_key.clone(),
                name: view.workspace_name.clone(),
                project_path: view.project_path.clone(),
                tasks: Vec::new(),
                last_activity_at: None,
            });
        let activity = view.updated_at.or(Some(task.seen_at));
        if group.last_activity_at.is_none()
            || activity.unwrap_or(0.0) > group.last_activity_at.unwrap_or(0.0)
        {
            group.last_activity_at = activity;
        }
        group.tasks.push(view);
    }

    let mut workspaces: Vec<WorkspaceGroup> = groups.into_values().collect();
    for group in &mut workspaces {
        group.tasks.sort_by(|left, right| {
            (right.status == "running")
                .cmp(&(left.status == "running"))
                .then_with(|| right.source_pinned.cmp(&left.source_pinned))
                .then_with(|| {
                    right
                        .updated_at
                        .unwrap_or(0.0)
                        .total_cmp(&left.updated_at.unwrap_or(0.0))
                })
                .then_with(|| left.title.cmp(&right.title))
        });
    }
    workspaces.sort_by(|left, right| {
        (left.key == NO_PROJECT_KEY)
            .cmp(&(right.key == NO_PROJECT_KEY))
            .then_with(|| {
                right
                    .last_activity_at
                    .unwrap_or(0.0)
                    .total_cmp(&left.last_activity_at.unwrap_or(0.0))
            })
            .then_with(|| left.name.cmp(&right.name))
    });

    let counts = count_tasks(&workspaces);
    let sources = build_sources(&tasks, &states);
    let usage_summary = attach_usage(&mut workspaces, usage);

    Ok(BootstrapPayload {
        generated_at: now,
        sources,
        workspaces,
        counts,
        usage: usage_summary,
    })
}

fn attach_usage(
    workspaces: &mut [WorkspaceGroup],
    usage: Option<&UsageSnapshot>,
) -> Option<UsageOverview> {
    let snapshot = usage?;
    let mut matched: HashMap<String, i64> = HashMap::new();
    for group in workspaces.iter_mut() {
        for task in &mut group.tasks {
            if let Some(found) = snapshot.task_usage(&task.source, &task.source_id) {
                *matched.entry(task.source.clone()).or_default() += 1;
                task.usage = Some(found);
            }
        }
    }
    let mut overview = snapshot.overview.clone();
    for entry in &mut overview.sources {
        entry.matched_tasks = matched.get(&entry.source).copied().unwrap_or(0);
    }
    // A source whose sessions no longer exist in Perch would only add noise.
    overview.sources.retain(|entry| entry.matched_tasks > 0);
    Some(overview)
}

fn task_view(
    task: &TaskSnapshot,
    todos_by_task: &mut HashMap<String, Vec<TodoView>>,
    now: f64,
) -> TaskView {
    let todo_items = todos_by_task.remove(&task.key).unwrap_or_default();
    let todo_total = task.todo_total.unwrap_or(todo_items.len() as i64);
    let todo_done = task.todo_done.unwrap_or_else(|| {
        todo_items
            .iter()
            .filter(|todo| todo.status == "completed")
            .count() as i64
    });
    let activity = task.updated_at.unwrap_or(task.seen_at);
    let (workspace_key, workspace_name, project_path) = workspace_of(task);
    TaskView {
        key: task.key.clone(),
        source: task.source.clone(),
        source_label: source_label(&task.source).to_owned(),
        source_id: task.source_id.clone(),
        parent_key: task.parent_key.clone(),
        is_child: task.parent_key.is_some(),
        is_automation: task.agent_kind.as_deref() == Some("automation"),
        title: task.title.clone(),
        title_source: task.title_source.clone(),
        project_path,
        project_name: task.project_name.clone(),
        workspace_key,
        workspace_name,
        created_at: task.created_at,
        updated_at: task.updated_at,
        status: task
            .status_source
            .clone()
            .unwrap_or_else(|| "idle".to_owned()),
        status_raw: task.status_raw.clone(),
        source_archived: task.source_archived,
        source_unread: task.source_unread,
        source_pinned: task.source_pinned,
        source_missing: task.source_missing,
        model: task.model.clone(),
        git_branch: task.git_branch.clone(),
        agent_kind: task.agent_kind.clone(),
        message_count: task.message_count,
        todo_total,
        todo_done,
        deeplink: task.deeplink.clone(),
        resume_cmd: task.resume_cmd.clone(),
        is_recent: activity >= now - RECENT_WINDOW_SECONDS,
        todos: todo_items,
        usage: None,
    }
}

fn todo_view(todo: TodoSnapshot) -> TodoView {
    TodoView {
        item_id: todo.item_id,
        subject: todo.subject,
        status: todo.status,
        position: todo.position,
    }
}

fn workspace_of(task: &TaskSnapshot) -> (String, String, Option<String>) {
    if let Some(path) = task
        .project_path
        .as_deref()
        .map(crate::collectors::common::normalize_project_path)
        .filter(|value| !value.is_empty())
    {
        let name = task
            .project_name
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| crate::collectors::common::project_name(&path))
            .unwrap_or_else(|| path.clone());
        return (path.clone(), name, Some(path));
    }
    if let Some(name) = task
        .project_name
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        return (name.clone(), name, None);
    }
    (NO_PROJECT_KEY.to_owned(), NO_PROJECT_NAME.to_owned(), None)
}

const NO_PROJECT_KEY: &str = "__no_project__";
const NO_PROJECT_NAME: &str = "未关联项目";

fn count_tasks(workspaces: &[WorkspaceGroup]) -> Counts {
    let mut counts = Counts::default();
    for group in workspaces {
        for task in &group.tasks {
            if task.is_child || task.is_automation {
                continue;
            }
            if task.source_archived {
                counts.archived += 1;
                continue;
            }
            counts.total += 1;
            if task.is_recent {
                counts.recent += 1;
            } else {
                counts.older += 1;
            }
            if task.source_unread
                || task.source_missing
                || matches!(task.status.as_str(), "blocked" | "limited")
            {
                counts.attention += 1;
            }
        }
    }
    counts
}

fn build_sources(tasks: &[TaskSnapshot], states: &[SourceSyncState]) -> Vec<SourceView> {
    let by_source: HashMap<&str, &SourceSyncState> = states
        .iter()
        .map(|state| (state.source.as_str(), state))
        .collect();
    let mut task_counts: HashMap<&str, i64> = HashMap::new();
    for task in tasks {
        *task_counts.entry(task.source.as_str()).or_insert(0) += 1;
    }

    ALL_SOURCES
        .iter()
        .map(|source| {
            let state = by_source.get(*source);
            SourceView {
                source: (*source).to_owned(),
                label: source_label(source).to_owned(),
                status: state
                    .map(|item| item.status.clone())
                    .unwrap_or_else(|| "unknown".to_owned()),
                last_scan_at: state.and_then(|item| item.last_scan_at),
                last_ok_at: state.and_then(|item| item.last_ok_at),
                task_count: task_counts.get(*source).copied().unwrap_or(0),
                files_seen: state.and_then(|item| item.files_seen),
                error: state.and_then(|item| item.error.clone()),
            }
        })
        .collect()
}
