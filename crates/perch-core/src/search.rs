use std::collections::HashMap;

use pinyin::ToPinyin;
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

use crate::collectors::common::normalize_project_path;
use crate::error::Result;
use crate::models::{source_label, TaskSnapshot};
use crate::store::CacheStore;

/// P0 returns only the best matches; the UI reports the total separately.
pub const MAX_RESULTS: usize = 10;

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub task_key: String,
    pub score: f32,
    /// Field labels such as 标题 / 待办 / 项目, ordered by weight.
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub total: usize,
}

struct FieldText {
    label: &'static str,
    weight: f32,
    folded: String,
    pinyin: String,
    initials: String,
}

struct SearchDoc {
    key: String,
    status: String,
    pinned: bool,
    updated_at: Option<f64>,
    fields: Vec<FieldText>,
}

struct ScoredHit {
    score: f32,
    updated_at: Option<f64>,
    key: String,
    fields: Vec<String>,
}

pub fn search_tasks(
    store: &CacheStore,
    query: &str,
    source: Option<&str>,
    workspace: Option<&str>,
) -> Result<SearchResponse> {
    search_tasks_at(
        store,
        query,
        source,
        workspace,
        crate::collectors::common::now_seconds(),
    )
}

fn search_tasks_at(
    store: &CacheStore,
    query: &str,
    source: Option<&str>,
    workspace: Option<&str>,
    now: f64,
) -> Result<SearchResponse> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return Ok(SearchResponse {
            hits: Vec::new(),
            total: 0,
        });
    }

    let mut todos_by_task: HashMap<String, Vec<String>> = HashMap::new();
    for todo in store.load_todos()? {
        todos_by_task
            .entry(todo.task_key.clone())
            .or_default()
            .push(todo.subject);
    }

    let mut hits = Vec::new();
    for task in store.load_tasks()? {
        if source.is_some_and(|wanted| wanted != task.source) {
            continue;
        }
        let workspace_key = workspace_key(&task);
        if workspace.is_some_and(|wanted| wanted != workspace_key.as_str()) {
            continue;
        }
        let doc = build_doc(&task, todos_by_task.remove(&task.key).unwrap_or_default());
        if let Some(scored) = score_doc(&doc, &terms, now) {
            hits.push(scored);
        }
    }

    let total = hits.len();
    hits.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| {
                right
                    .updated_at
                    .unwrap_or(0.0)
                    .total_cmp(&left.updated_at.unwrap_or(0.0))
            })
            .then_with(|| left.key.cmp(&right.key))
    });
    hits.truncate(MAX_RESULTS);

    Ok(SearchResponse {
        hits: hits
            .into_iter()
            .map(|hit| SearchHit {
                task_key: hit.key,
                score: hit.score,
                fields: hit.fields,
            })
            .collect(),
        total,
    })
}

fn build_doc(task: &TaskSnapshot, todos: Vec<String>) -> SearchDoc {
    let mut fields = Vec::new();
    push_field(&mut fields, "标题", 6.0, task.title.clone());
    if let Some(name) = task
        .project_name
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        push_field(&mut fields, "项目", 4.0, name);
    }
    if let Some(path) = task
        .project_path
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        push_field(&mut fields, "项目路径", 4.0, path);
    }
    if !todos.is_empty() {
        push_field(&mut fields, "待办", 3.0, todos.join("\n"));
    }
    if let Some(model) = task.model.clone().filter(|value| !value.trim().is_empty()) {
        push_field(&mut fields, "模型", 2.0, model);
    }
    if let Some(branch) = task
        .git_branch
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        push_field(&mut fields, "分支", 2.0, branch);
    }
    let status = task
        .status_source
        .clone()
        .unwrap_or_else(|| "idle".to_owned());
    push_field(
        &mut fields,
        "状态",
        2.0,
        format!(
            "{} {}",
            status_label(&status),
            task.status_raw.clone().unwrap_or_default()
        ),
    );
    push_field(
        &mut fields,
        "来源",
        2.0,
        source_label(&task.source).to_owned(),
    );
    if let Some(kind) = task
        .agent_kind
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        push_field(
            &mut fields,
            "类型",
            1.0,
            format!("{} {kind}", agent_kind_label(&kind)),
        );
    }
    push_field(&mut fields, "会话 ID", 1.0, task.source_id.clone());

    SearchDoc {
        key: task.key.clone(),
        status,
        pinned: task.source_pinned,
        updated_at: task.updated_at,
        fields,
    }
}

fn push_field(fields: &mut Vec<FieldText>, label: &'static str, weight: f32, text: String) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let folded = fold(text);
    if folded.is_empty() {
        return;
    }
    let (pinyin, initials) = pinyin_forms(text);
    fields.push(FieldText {
        label,
        weight,
        folded,
        pinyin,
        initials,
    });
}

fn score_doc(doc: &SearchDoc, terms: &[String], now: f64) -> Option<ScoredHit> {
    let mut score = 0.0f32;
    let mut field_scores: Vec<(&'static str, f32)> = Vec::new();

    for term in terms {
        let pinyin_term = is_ascii_word(term).then_some(term.as_str());
        let mut term_best: Option<f32> = None;
        for field in &doc.fields {
            let Some(quality) = field_match(term, pinyin_term, field) else {
                continue;
            };
            let weighted = quality * field.weight;
            term_best = Some(term_best.map_or(weighted, |best| best.max(weighted)));
            match field_scores
                .iter_mut()
                .find(|(label, _)| *label == field.label)
            {
                Some((_, current)) => *current = current.max(weighted),
                None => field_scores.push((field.label, weighted)),
            }
        }
        let best = term_best?;
        score += best;
    }

    let age_days = ((now - doc.updated_at.unwrap_or(now)) / 86_400.0).max(0.0);
    score += 0.6 / (1.0 + age_days as f32 / 30.0);
    if doc.status == "running" {
        score += 0.4;
    }
    if doc.pinned {
        score += 0.2;
    }

    field_scores
        .sort_by(|left, right| right.1.total_cmp(&left.1).then_with(|| left.0.cmp(right.0)));

    Some(ScoredHit {
        score,
        updated_at: doc.updated_at,
        key: doc.key.clone(),
        fields: field_scores
            .into_iter()
            .map(|(label, _)| label.to_owned())
            .collect(),
    })
}

fn field_match(term: &str, pinyin_term: Option<&str>, field: &FieldText) -> Option<f32> {
    let mut best: Option<f32> = None;
    let mut keep = |quality: f32| {
        best = Some(best.map_or(quality, |current: f32| current.max(quality)));
    };

    if field.folded == term {
        keep(1.0);
    } else if field.folded.starts_with(term) {
        keep(0.9);
    } else if field.folded.contains(term) {
        keep(0.8);
    }

    if is_ascii_word(term) {
        for token in field
            .folded
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| !token.is_empty())
        {
            if token == term {
                keep(0.85);
            } else if token.starts_with(term) && term.len() >= 2 {
                keep(0.75);
            } else if term.len() >= 3 && edit_within(term, token) {
                keep(0.55);
            }
        }
        if term.len() >= 2 && is_subsequence(term, &field.folded) {
            keep(0.45);
        }
        if let Some(pinyin) = pinyin_term.filter(|value| value.len() >= 2) {
            if field.pinyin == pinyin {
                keep(1.0);
            } else if field.pinyin.starts_with(pinyin) {
                keep(0.85);
            } else if field.pinyin.contains(pinyin) {
                keep(0.7);
            }
            if field.initials == pinyin {
                keep(0.8);
            } else if field.initials.contains(pinyin) {
                keep(0.6);
            }
        }
    }

    best
}

fn query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in query.chars() {
        if character == '"' {
            quoted = !quoted;
            if !quoted {
                push_term(&mut terms, &current);
                current.clear();
            }
        } else if character.is_whitespace() && !quoted {
            push_term(&mut terms, &current);
            current.clear();
        } else {
            current.push(character);
        }
    }
    push_term(&mut terms, &current);
    terms
}

fn push_term(terms: &mut Vec<String>, raw: &str) {
    let term = fold(raw);
    if !term.is_empty() {
        terms.push(term);
    }
}

fn fold(text: &str) -> String {
    text.nfkc().collect::<String>().to_lowercase()
}

fn pinyin_forms(text: &str) -> (String, String) {
    let mut full = String::new();
    let mut initials = String::new();
    for character in text.chars() {
        if let Some(pinyin) = character.to_pinyin() {
            full.push_str(pinyin.plain());
            initials.push_str(pinyin.first_letter());
        } else if character.is_ascii_alphanumeric() {
            let lower = character.to_ascii_lowercase();
            full.push(lower);
            initials.push(lower);
        }
    }
    (full, initials)
}

fn is_ascii_word(term: &str) -> bool {
    !term.is_empty()
        && term
            .chars()
            .all(|character| character.is_ascii_alphabetic())
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut wanted = needle.chars();
    let Some(mut next) = wanted.next() else {
        return true;
    };
    for character in haystack.chars() {
        if character == next {
            match wanted.next() {
                Some(value) => next = value,
                None => return true,
            }
        }
    }
    false
}

fn edit_within(left: &str, right: &str) -> bool {
    let limit = if left.chars().count() <= 5 { 1 } else { 2 };
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.len().abs_diff(right.len()) > limit {
        return false;
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];
    for (row, left_char) in left.iter().enumerate() {
        current[0] = row + 1;
        for (column, right_char) in right.iter().enumerate() {
            let substitution = previous[column] + usize::from(left_char != right_char);
            current[column + 1] = (previous[column + 1] + 1)
                .min(current[column] + 1)
                .min(substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()] <= limit
}

fn workspace_key(task: &TaskSnapshot) -> String {
    if let Some(path) = task
        .project_path
        .as_deref()
        .map(normalize_project_path)
        .filter(|value| !value.is_empty())
    {
        return path;
    }
    if let Some(name) = task
        .project_name
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        return name;
    }
    "__no_project__".to_owned()
}

fn status_label(status: &str) -> &'static str {
    match status {
        "running" => "运行中",
        "active" => "活跃",
        "waiting" => "等待中",
        "paused" => "已暂停",
        "blocked" => "受阻",
        "limited" => "受限",
        "complete" => "已完成",
        "archived" => "已归档",
        "idle" => "空闲",
        _ => "",
    }
}

fn agent_kind_label(kind: &str) -> &'static str {
    match kind {
        "cli" => "CLI",
        "desktop" => "桌面",
        "automation" => "自动化",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{SourceResult, TaskSnapshot};

    fn store_with(tasks: Vec<TaskSnapshot>) -> CacheStore {
        let store = CacheStore::open_in_memory().unwrap();
        let scan_id = store.begin_scan("test", "full", "source", "test").unwrap();
        // The scan source is irrelevant for search; tasks keep their own source.
        store
            .save_result(&SourceResult::ok("test", tasks, Vec::new()), scan_id)
            .unwrap();
        store
    }

    fn task(id: &str, title: &str) -> TaskSnapshot {
        let mut task = TaskSnapshot::new("claude", id);
        task.title = title.to_owned();
        task.updated_at = Some(1_000.0);
        task.refresh_fingerprint();
        task
    }

    fn search(store: &CacheStore, query: &str) -> SearchResponse {
        search_tasks_at(store, query, None, None, 2_000.0).unwrap()
    }

    #[test]
    fn matches_title_substring_and_terms() {
        let store = store_with(vec![task("a", "修复登录超时"), task("b", "重构搜索模块")]);

        let hits = search(&store, "登录");
        assert_eq!(hits.total, 1);
        assert_eq!(hits.hits[0].task_key, "claude:a");
        assert!(hits.hits[0].fields.contains(&"标题".to_owned()));

        assert_eq!(search(&store, "修复 搜索").total, 0);
    }

    #[test]
    fn matches_pinyin_full_and_initials() {
        let store = store_with(vec![task("a", "修复登录超时")]);

        assert_eq!(search(&store, "denglu").total, 1);
        assert_eq!(search(&store, "dlcs").total, 1);
        assert_eq!(search(&store, "xiufu ceshi").total, 0);
    }

    #[test]
    fn matches_latin_fuzzy_and_subsequence() {
        let store = store_with(vec![task("a", "authentication timeout")]);

        assert_eq!(search(&store, "authentication").total, 1);
        assert_eq!(search(&store, "authetication").total, 1);
        assert_eq!(search(&store, "atmo").total, 1);
    }

    #[test]
    fn matches_todos_and_session_id() {
        let store = CacheStore::open_in_memory().unwrap();
        let scan_id = store
            .begin_scan("claude", "full", "source", "test")
            .unwrap();
        let mut todo_task = task("session-123", "无标题");
        todo_task.refresh_fingerprint();
        let todo = crate::models::TodoSnapshot {
            task_key: "claude:session-123".to_owned(),
            item_id: "1".to_owned(),
            subject: "补充单元测试".to_owned(),
            status: "pending".to_owned(),
            position: 0,
        };
        store
            .save_result(
                &SourceResult::ok("claude", vec![todo_task], vec![todo]),
                scan_id,
            )
            .unwrap();

        assert!(search(&store, "单元测试").total == 1);
        assert!(search(&store, "session-123").total == 1);
    }

    #[test]
    fn includes_archived_and_child_sessions() {
        let store = CacheStore::open_in_memory().unwrap();
        let scan_id = store
            .begin_scan("claude", "full", "source", "test")
            .unwrap();
        let mut archived = task("a", "旧会话");
        archived.source_archived = true;
        let mut child = task("b", "子会话");
        child.parent_key = Some("claude:a".to_owned());
        store
            .save_result(
                &SourceResult::ok("claude", vec![archived, child], Vec::new()),
                scan_id,
            )
            .unwrap();

        assert_eq!(search(&store, "旧会话").total, 1);
        assert_eq!(search(&store, "子会话").total, 1);
    }

    #[test]
    fn limits_hits_and_reports_total() {
        let tasks = (0..12)
            .map(|index| task(&format!("task-{index}"), &format!("匹配任务 {index}")))
            .collect();
        let store = store_with(tasks);

        let response = search(&store, "匹配");
        assert_eq!(response.total, 12);
        assert_eq!(response.hits.len(), MAX_RESULTS);
    }

    #[test]
    fn filters_by_source_and_workspace() {
        let store = CacheStore::open_in_memory().unwrap();
        let scan_id = store.begin_scan("test", "full", "source", "test").unwrap();
        let mut first = task("a", "搜索 alpha");
        first.source = "claude".to_owned();
        first.key = "claude:a".to_owned();
        first.project_path = Some("/tmp/project-one".to_owned());
        first.refresh_fingerprint();
        let mut second = task("b", "搜索 beta");
        second.source = "codex".to_owned();
        second.key = "codex:b".to_owned();
        second.project_path = Some("/tmp/project-two".to_owned());
        second.refresh_fingerprint();
        store
            .save_result(
                &SourceResult::ok("test", vec![first, second], Vec::new()),
                scan_id,
            )
            .unwrap();

        assert_eq!(
            search_tasks_at(&store, "搜索", Some("codex"), None, 2_000.0)
                .unwrap()
                .total,
            1
        );
        assert_eq!(
            search_tasks_at(&store, "搜索", None, Some("/tmp/project-one"), 2_000.0)
                .unwrap()
                .total,
            1
        );
    }
}
