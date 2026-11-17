import type {
  BootstrapPayload,
  SourceView,
  TaskUsage,
  TaskView,
  TodoView,
  WorkspaceGroup,
} from "../src/lib/types";

const NOW = Math.floor(Date.now() / 1000);
const MINUTE = 60;
const DAY = 86_400;

const SOURCE_LABELS: Record<string, string> = {
  qoder: "Qoder",
  claude: "Claude Code",
  codex: "Codex",
  opencode: "OpenCode",
  kilo: "Kilo",
  cline: "Cline",
};

const SOURCES = ["qoder", "claude", "codex", "opencode", "kilo", "cline"] as const;

const WORKSPACES = [
  { name: "acme-storefront", path: "/Users/demo/code/acme-storefront" },
  { name: "docs-portal", path: "/Users/demo/code/docs-portal" },
  { name: "data-pipeline", path: "/Users/demo/code/data-pipeline" },
  { name: "scratchpad", path: "/Users/demo/code/scratchpad" },
];

interface TaskSeed {
  source: string;
  title: string;
  workspace: number;
  status: string;
  updated_minutes_ago: number;
  created_days_ago: number;
  todos?: Array<[string, TodoView["status"]]>;
  archived?: boolean;
  unread?: boolean;
  pinned?: boolean;
  model?: string;
  branch?: string;
  messages?: number;
  title_source?: string;
  resume_cmd?: string;
  usage?: TaskUsage;
}

let sequence = 0;

function makeTask(seed: TaskSeed): TaskView {
  sequence += 1;
  const workspace = WORKSPACES[seed.workspace];
  const todos: TodoView[] = (seed.todos ?? []).map(([subject, status], index) => ({
    item_id: `${sequence}-${index}`,
    subject,
    status,
    position: index,
  }));
  const updated = NOW - seed.updated_minutes_ago * MINUTE;
  return {
    key: `${seed.source}:demo-${sequence}`,
    source: seed.source,
    source_label: SOURCE_LABELS[seed.source],
    source_id: `demo-${seed.source}-${String(sequence).padStart(4, "0")}`,
    parent_key: null,
    is_child: false,
    is_automation: false,
    title: seed.title,
    title_source: seed.title_source ?? "source",
    project_path: workspace.path,
    project_name: workspace.name,
    workspace_key: workspace.path,
    workspace_name: workspace.name,
    created_at: NOW - seed.created_days_ago * DAY,
    updated_at: updated,
    status: seed.status,
    status_raw: seed.status === "idle" ? null : seed.status,
    source_archived: Boolean(seed.archived),
    source_unread: Boolean(seed.unread),
    source_pinned: Boolean(seed.pinned),
    source_missing: false,
    model: seed.model ?? null,
    git_branch: seed.branch ?? null,
    agent_kind: null,
    message_count: seed.messages ?? null,
    todo_total: todos.length,
    todo_done: todos.filter((todo) => todo.status === "completed").length,
    deeplink: null,
    resume_cmd: seed.resume_cmd ?? null,
    is_recent: updated >= NOW - 7 * DAY,
    todos,
    usage: seed.usage,
  };
}

const DEMO_DAY = new Date(NOW * 1000).toISOString().slice(0, 10);

const TOKEN_USAGE: TaskUsage = {
  token: {
    input: 1_284_000,
    output: 218_400,
    cache_read: 3_920_000,
    cache_write: 640_000,
    reasoning: 96_000,
    total: 6_158_400,
    turns: 84,
    models: ["claude-sonnet-4-5"],
    last_day: DEMO_DAY,
  },
};

const CODEX_USAGE: TaskUsage = {
  token: {
    input: 860_000,
    output: 142_000,
    cache_read: 2_110_000,
    cache_write: 0,
    reasoning: 128_000,
    total: 3_240_000,
    turns: 41,
    models: ["gpt-5-codex"],
    last_day: DEMO_DAY,
  },
};

const QODER_USAGE: TaskUsage = {
  credits: {
    billable: 18.42,
    free: 0.8,
    billable_requests: 27,
    free_requests: 3,
    last_day: DEMO_DAY,
  },
  est: {
    source: "qoder.est",
    input: 1_120_000,
    output: 96_000,
    total: 1_216_000,
    requests: 27,
    last_day: DEMO_DAY,
  },
};

const TASKS: TaskView[] = [
  makeTask({
    source: "qoder",
    title: "重构结算页的价格计算逻辑",
    workspace: 0,
    status: "running",
    updated_minutes_ago: 2,
    created_days_ago: 1,
    unread: true,
    model: "Qwen3.5-Plus",
    branch: "feature/checkout-pricing",
    messages: 48,
    usage: QODER_USAGE,
  }),
  makeTask({
    source: "claude",
    title: "为订单导出补充 CSV 单元测试",
    workspace: 0,
    status: "idle",
    updated_minutes_ago: 35,
    created_days_ago: 2,
    branch: "main",
    messages: 26,
    title_source: "ai",
    usage: TOKEN_USAGE,
    todos: [
      ["补齐空订单导出用例", "completed"],
      ["覆盖 UTF-8 与 BOM 场景", "in_progress"],
    ],
  }),
  makeTask({
    source: "codex",
    title: "修复移动端导航栏的层级问题",
    workspace: 0,
    status: "active",
    updated_minutes_ago: 60,
    created_days_ago: 3,
    model: "GPT-5 Codex",
    branch: "fix/mobile-nav",
    messages: 34,
    usage: CODEX_USAGE,
    resume_cmd:
      "cd '/Users/demo/code/acme-storefront' && codex resume demo-codex-0003",
  }),
  makeTask({
    source: "opencode",
    title: "接入 OAuth 登录回调",
    workspace: 0,
    status: "paused",
    updated_minutes_ago: 300,
    created_days_ago: 4,
    model: "Claude Sonnet 4.5",
    branch: "feature/oauth",
    messages: 57,
    usage: TOKEN_USAGE,
    resume_cmd:
      "cd '/Users/demo/code/acme-storefront' && opencode --session ses_demo_oauth",
    todos: [
      ["注册 OAuth 应用与回调地址", "completed"],
      ["实现授权码换取 token", "completed"],
      ["持久化 refresh token", "completed"],
      ["处理授权失败与重试提示", "in_progress"],
      ["补充端到端登录测试", "pending"],
    ],
  }),
  makeTask({
    source: "kilo",
    title: "清理未使用的 Tailwind 插件",
    workspace: 0,
    status: "complete",
    updated_minutes_ago: 1_440,
    created_days_ago: 6,
    messages: 18,
  }),
  makeTask({
    source: "qoder",
    title: "更新 API 参考文档",
    workspace: 1,
    status: "running",
    updated_minutes_ago: 12,
    created_days_ago: 1,
    unread: true,
    model: "Qwen3.5-Plus",
    messages: 21,
  }),
  makeTask({
    source: "claude",
    title: "翻译快速上手章节",
    workspace: 1,
    status: "idle",
    updated_minutes_ago: 120,
    created_days_ago: 2,
    title_source: "ai",
    todos: [
      ["同步术语表中的译法", "completed"],
      ["校对示例代码片段", "completed"],
    ],
  }),
  makeTask({
    source: "codex",
    title: "生成 changelog 草稿",
    workspace: 1,
    status: "complete",
    updated_minutes_ago: 2_880,
    created_days_ago: 5,
    messages: 9,
  }),
  makeTask({
    source: "opencode",
    title: "搭建日志聚合原型",
    workspace: 2,
    status: "blocked",
    updated_minutes_ago: 180,
    created_days_ago: 3,
    model: "Claude Sonnet 4.5",
    messages: 41,
    todos: [
      ["选定采集端方案", "completed"],
      ["验证日志字段映射", "in_progress"],
      ["压测写入吞吐", "pending"],
      ["补充告警规则", "pending"],
    ],
  }),
  makeTask({
    source: "kilo",
    title: "补齐指标看板图表",
    workspace: 2,
    status: "active",
    updated_minutes_ago: 360,
    created_days_ago: 5,
    pinned: true,
    messages: 62,
    todos: [
      ["新增延迟分位图", "completed"],
      ["新增错误率趋势图", "completed"],
      ["review 配色与图例", "completed"],
    ],
  }),
  makeTask({
    source: "cline",
    title: "梳理 Cline 会话存储结构",
    workspace: 2,
    status: "running",
    updated_minutes_ago: 75,
    created_days_ago: 1,
    model: "Claude Sonnet 4.5",
    resume_cmd:
      "cd '/Users/demo/code/data-pipeline' && cline --id demo-cline-0001",
  }),
  makeTask({
    source: "qoder",
    title: "实验：多模态输入解析",
    workspace: 3,
    status: "limited",
    updated_minutes_ago: 9 * 1_440,
    created_days_ago: 14,
    model: "Qwen3.5-Plus",
  }),
  makeTask({
    source: "claude",
    title: "调研向量检索方案",
    workspace: 3,
    status: "idle",
    updated_minutes_ago: 12 * 1_440,
    created_days_ago: 20,
    title_source: "provisional",
  }),
  makeTask({
    source: "codex",
    title: "批量重命名截图文件",
    workspace: 3,
    status: "complete",
    updated_minutes_ago: 4 * 1_440,
    created_days_ago: 9,
    messages: 7,
  }),
  makeTask({
    source: "cline",
    title: "修复 Cline 配置迁移脚本",
    workspace: 3,
    status: "failed",
    updated_minutes_ago: 6 * 1_440,
    created_days_ago: 8,
    messages: 14,
  }),
  makeTask({
    source: "codex",
    title: "旧版落地页迁移",
    workspace: 0,
    status: "complete",
    updated_minutes_ago: 20 * 1_440,
    created_days_ago: 40,
    archived: true,
    messages: 73,
  }),
  makeTask({
    source: "qoder",
    title: "临时排查构建失败",
    workspace: 1,
    status: "archived",
    updated_minutes_ago: 25 * 1_440,
    created_days_ago: 33,
    archived: true,
    model: "Qwen3.5-Plus",
  }),
  makeTask({
    source: "claude",
    title: "整理会议纪要",
    workspace: 3,
    status: "complete",
    updated_minutes_ago: 30 * 1_440,
    created_days_ago: 35,
    archived: true,
  }),
  makeTask({
    source: "kilo",
    title: "验证图片压缩流程",
    workspace: 2,
    status: "complete",
    updated_minutes_ago: 18 * 1_440,
    created_days_ago: 28,
    archived: true,
    messages: 12,
  }),
];

function buildWorkspaces(): WorkspaceGroup[] {
  return WORKSPACES.map((workspace) => {
    const tasks = TASKS.filter(
      (task) => task.workspace_key === workspace.path,
    ).sort((left, right) => (right.updated_at ?? 0) - (left.updated_at ?? 0));
    return {
      key: workspace.path,
      name: workspace.name,
      project_path: workspace.path,
      tasks,
      last_activity_at: tasks[0]?.updated_at ?? null,
    };
  })
    .filter((group) => group.tasks.length > 0)
    .sort(
      (left, right) =>
        (right.last_activity_at ?? 0) - (left.last_activity_at ?? 0),
    );
}

function buildSources(): SourceView[] {
  return SOURCES.map((source) => ({
    source,
    label: SOURCE_LABELS[source],
    status: "ok" as const,
    last_scan_at: NOW,
    last_ok_at: NOW,
    task_count: TASKS.filter((task) => task.source === source).length,
    files_seen: 6,
    error: null,
  }));
}

export function demoPayload(): BootstrapPayload {
  const workspaces = buildWorkspaces();
  const visible = TASKS.filter((task) => !task.source_archived);
  return {
    generated_at: NOW,
    sources: buildSources(),
    workspaces,
    counts: {
      total: visible.length,
      recent: visible.filter((task) => task.is_recent).length,
      older: visible.filter((task) => !task.is_recent).length,
      archived: TASKS.filter((task) => task.source_archived).length,
      attention: visible.filter(
        (task) =>
          task.source_unread ||
          task.status === "blocked" ||
          task.status === "limited",
      ).length,
    },
    usage: {
      as_of: NOW - 90,
      stale: false,
      sources: [
        {
          source: "qoder",
          token_total: 0,
          token_7d: 0,
          credits_billable_7d: 18.42,
          est_total: 1_216_000,
          last_day: DEMO_DAY,
          matched_tasks: TASKS.filter(
            (task) => task.source === "qoder" && task.usage,
          ).length,
        },
        {
          source: "claude",
          token_total: 18_702_000,
          token_7d: 6_158_400,
          credits_billable_7d: 0,
          est_total: 0,
          last_day: DEMO_DAY,
          matched_tasks: TASKS.filter(
            (task) => task.source === "claude" && task.usage,
          ).length,
        },
        {
          source: "codex",
          token_total: 6_158_400,
          token_7d: 3_240_000,
          credits_billable_7d: 0,
          est_total: 0,
          last_day: DEMO_DAY,
          matched_tasks: TASKS.filter(
            (task) => task.source === "codex" && task.usage,
          ).length,
        },
      ],
    },
  };
}
