export type SourceStatus = "ok" | "partial" | "unavailable" | "error" | "unknown";

export interface TodoView {
  item_id: string;
  subject: string;
  status: string;
  position: number;
}

export interface TokenUsage {
  input: number;
  output: number;
  cache_read: number;
  cache_write: number;
  reasoning: number;
  total: number;
  turns: number;
  models: string[];
  last_day: string | null;
}

export interface CreditUsage {
  billable: number;
  free: number;
  billable_requests: number;
  free_requests: number;
  last_day: string | null;
}

export interface EstUsage {
  source: string;
  input: number;
  output: number;
  total: number;
  requests: number;
  last_day: string | null;
}

export interface TaskUsage {
  token?: TokenUsage;
  credits?: CreditUsage;
  est?: EstUsage;
}

export interface SourceUsageSummary {
  source: string;
  token_total: number;
  token_7d: number;
  credits_billable_7d: number;
  est_total: number;
  last_day: string | null;
  matched_tasks: number;
}

export interface UsageOverview {
  as_of: number | null;
  stale: boolean;
  sources: SourceUsageSummary[];
}

export interface TaskView {
  key: string;
  source: string;
  source_label: string;
  source_id: string;
  parent_key: string | null;
  is_child: boolean;
  is_automation: boolean;
  title: string;
  title_source: string;
  project_path: string | null;
  project_name: string | null;
  workspace_key: string;
  workspace_name: string;
  created_at: number | null;
  updated_at: number | null;
  status: string;
  status_raw: string | null;
  source_archived: boolean;
  source_unread: boolean;
  source_pinned: boolean;
  source_missing: boolean;
  model: string | null;
  git_branch: string | null;
  agent_kind: string | null;
  message_count: number | null;
  todo_total: number;
  todo_done: number;
  deeplink: string | null;
  resume_cmd: string | null;
  is_recent: boolean;
  todos: TodoView[];
  usage?: TaskUsage;
}

export interface WorkspaceGroup {
  key: string;
  name: string;
  project_path: string | null;
  tasks: TaskView[];
  last_activity_at: number | null;
}

export interface SourceView {
  source: string;
  label: string;
  status: SourceStatus;
  last_scan_at: number | null;
  last_ok_at: number | null;
  task_count: number;
  files_seen: number | null;
  error: string | null;
}

export interface Counts {
  total: number;
  recent: number;
  older: number;
  archived: number;
  attention: number;
}

export interface BootstrapPayload {
  generated_at: number;
  sources: SourceView[];
  workspaces: WorkspaceGroup[];
  counts: Counts;
  usage?: UsageOverview;
}

export interface UsageReportOutcome {
  path: string;
}

export interface SearchHit {
  task_key: string;
  score: number;
  fields: string[];
}

export interface SearchResponse {
  hits: SearchHit[];
  total: number;
}

export interface SyncReport {
  started_at: number;
  finished_at: number;
  reports: Array<{
    source: string;
    status: SourceStatus;
    mode: string;
    task_count: number;
    todo_count: number;
    missing_count: number;
    duration_ms: number;
    error: string | null;
  }>;
}

export interface ContinueOutcome {
  channel: "app" | "cli";
  target: string;
  message: string;
}
