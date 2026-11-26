import type {
  BootstrapPayload,
  ContinueHint,
  SourceView,
  TaskUsage,
  TaskView,
  TodoView,
  TranscriptMessage,
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
  "trae-cn": "Trae CN",
  cline: "Cline",
  antigravity: "Antigravity",
  minimax: "MiniMax Code",
  codearts: "CodeArts",
  codebuddy: "CodeBuddy",
};

const SOURCES = [
  "qoder",
  "claude",
  "codex",
  "opencode",
  "kilo",
  "trae-cn",
  "cline",
  "antigravity",
  "minimax",
  "codearts",
  "codebuddy",
] as const;

const WORKSPACES = [
  { name: "acme-storefront", path: "/Users/demo/code/acme-storefront" },
  { name: "docs-portal", path: "/Users/demo/code/docs-portal" },
  { name: "data-pipeline", path: "/Users/demo/code/data-pipeline" },
  { name: "scratchpad", path: "/Users/demo/code/scratchpad" },
  { name: "docs-portal", path: "/Users/demo/archive/docs-portal" },
  { name: "tmp", path: "/private/tmp/perch-demo" },
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
  agent_kind?: string;
  source_id?: string;
  usage?: TaskUsage;
  is_child?: boolean;
  is_automation?: boolean;
}

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

let sequence = 0;

function makeTask(seed: TaskSeed): TaskView {
  sequence += 1;
  const sourceId =
    seed.source_id ?? `demo-${seed.source}-${String(sequence).padStart(4, "0")}`;
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
    source_id: sourceId,
    parent_key: null,
    is_child: Boolean(seed.is_child),
    is_automation: Boolean(seed.is_automation),
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
    agent_kind: seed.agent_kind ?? null,
    message_count: seed.messages ?? null,
    todo_total: todos.length,
    todo_done: todos.filter((todo) => todo.status === "completed").length,
    deeplink: null,
    resume_cmd: seed.resume_cmd ?? null,
    is_recent: updated >= NOW - 7 * DAY,
    todos,
    continue_hint: demoContinueHint(seed.source, seed.agent_kind, sourceId),
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
    source_id: "demo0a1b-2c3d-4e5f-8a9b-0c1d2e3f4a5b",
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
    source_id: "01a0f16e-6064-7033-a99a-f0800a7f5477",
    agent_kind: "desktop",
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
    source_id: "demo6f7a-8b9c-40d1-9e2f-3a4b5c6d7e8f",
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
// —— #27 对话面板合成场景 ——
// 仅在 ?transcript=1 时注入任务；默认列表口径（计数与标题）不变，S1 断言不受影响。
const TRANSCRIPT_SCENARIO = new URLSearchParams(location.search).get("transcript") === "1";

interface TranscriptSpec {
  messages?: TranscriptMessage[];
  sessionTruncated?: boolean;
  error?: string;
  failFirst?: boolean;
  failReason?: string;
}

function demoDialogue(count: number, label: string): TranscriptMessage[] {
  return Array.from({ length: count }, (_, index) => ({
    role: index % 2 === 0 ? ("user" as const) : ("assistant" as const),
    text: `${label}演示消息 ${index + 1}。`,
    truncated: false,
  }));
}

const LONG_MESSAGES: TranscriptMessage[] = Array.from({ length: 120 }, (_, index) => ({
  role: index % 2 === 0 ? ("user" as const) : ("assistant" as const),
  text: `长会话演示第 ${index + 1} 条消息，用于分页验收。`,
  truncated: false,
}));

// 单条 8000 字符的合成消息：正文读取通道会截断并标注「已截断」。
const LONG_TEXT = "报错堆栈行，用于演示单条消息截断。".repeat(500);

const TRANSCRIPT_SPECS: Record<string, TranscriptSpec> = {
  "合成对话 · 分页长会话": { messages: LONG_MESSAGES },
  "合成对话 · 工具轮与截断": {
    messages: [
      { role: "user", text: "帮我看一下结账页的报错。", truncated: false },
      { role: "tool_call", text: "工具调用 3 次", truncated: false },
      { role: "assistant", text: "我看完了日志：报错来自价格计算里的除零分支。", truncated: false },
      { role: "assistant", text: LONG_TEXT, truncated: true },
      { role: "user", text: "好，帮我顺手修掉。", truncated: false },
      { role: "assistant", text: "已提交修复，等 CI 结果。", truncated: false },
    ],
  },
  "合成对话 · 超大会话截断": {
    sessionTruncated: true,
    messages: [
      { role: "user", text: "继续刚才的修复。", truncated: false },
      { role: "assistant", text: "修复已验证通过，回归用例全绿。", truncated: false },
      { role: "user", text: "辛苦，我合并了。", truncated: false },
      { role: "assistant", text: "好的，我更新任务状态。", truncated: false },
    ],
  },
  "合成对话 · 首读失败后成功": {
    failFirst: true,
    failReason: "源文件不存在：/Users/demo/.claude/projects/demo/missing-session.jsonl",
    messages: [
      { role: "user", text: "重新读取成功：这条会话可以正常显示了。", truncated: false },
      { role: "assistant", text: "好的，我继续。", truncated: false },
    ],
  },
  "合成对话 · 空会话": {
    error: "会话没有可显示的对话内容",
  },
  "合成对话 · Antigravity 来源": { messages: demoDialogue(4, "Antigravity 来源") },
  "合成对话 · MiniMax 来源": { messages: demoDialogue(4, "MiniMax 来源") },
  "合成对话 · CodeArts 来源": { messages: demoDialogue(4, "CodeArts 来源") },
};

// 不在剧本表内的任务给一段通用合成对话，保证默认任务也能打开对话面板。
function genericConversation(title: string): TranscriptMessage[] {
  return [1, 2, 3, 4, 5, 6].map((n) => ({
    role: n % 2 === 1 ? ("user" as const) : ("assistant" as const),
    text: `「${title}」的演示消息 ${n}。`,
    truncated: false,
  }));
}

export interface DemoTranscriptOutcome {
  status: "ready" | "error";
  reason?: string;
  messages: TranscriptMessage[];
  session_truncated: boolean;
}

export function demoTranscriptOutcome(
  title: string,
  options: { firstAttempt: boolean },
): DemoTranscriptOutcome {
  const spec = TRANSCRIPT_SPECS[title];
  if (!spec) {
    return {
      status: "ready",
      messages: genericConversation(title),
      session_truncated: false,
    };
  }
  if (spec.error) {
    return { status: "error", reason: spec.error, messages: [], session_truncated: false };
  }
  if (spec.failFirst && options.firstAttempt) {
    return {
      status: "error",
      reason: spec.failReason ?? "源文件暂不可读",
      messages: [],
      session_truncated: false,
    };
  }
  return {
    status: "ready",
    messages: spec.messages ?? [],
    session_truncated: spec.sessionTruncated ?? false,
  };
}

if (TRANSCRIPT_SCENARIO) {
  const scenarioTasks: TaskSeed[] = [
    { source: "claude", title: "合成对话 · 分页长会话", workspace: 0, status: "idle", updated_minutes_ago: 18, created_days_ago: 1, messages: 120 },
    { source: "claude", title: "合成对话 · 工具轮与截断", workspace: 0, status: "running", updated_minutes_ago: 12, created_days_ago: 1, unread: true },
    { source: "claude", title: "合成对话 · 超大会话截断", workspace: 0, status: "idle", updated_minutes_ago: 40, created_days_ago: 2 },
    { source: "claude", title: "合成对话 · 首读失败后成功", workspace: 0, status: "idle", updated_minutes_ago: 65, created_days_ago: 2 },
    { source: "claude", title: "合成对话 · 空会话", workspace: 0, status: "idle", updated_minutes_ago: 90, created_days_ago: 3 },
    { source: "antigravity", title: "合成对话 · Antigravity 来源", workspace: 0, status: "idle", updated_minutes_ago: 24, created_days_ago: 1 },
    { source: "minimax", title: "合成对话 · MiniMax 来源", workspace: 0, status: "complete", updated_minutes_ago: 55, created_days_ago: 2 },
    { source: "codearts", title: "合成对话 · CodeArts 来源", workspace: 0, status: "idle", updated_minutes_ago: 70, created_days_ago: 2 },
    { source: "trae-cn", title: "合成对话 · Trae CN 无图标", workspace: 0, status: "idle", updated_minutes_ago: 30, created_days_ago: 1 },
    { source: "codebuddy", title: "合成对话 · CodeBuddy 无图标", workspace: 0, status: "idle", updated_minutes_ago: 45, created_days_ago: 1 },
    // 子会话 / 自动化任务：不进 primary 集合，仅经搜索 reveal 后以选中态出现在
    // 列表并可点开对话（T8 判定兜底口径的驱动场景）；默认列表口径不变。
    { source: "claude", title: "合成对话 · 子会话（搜索可见）", workspace: 0, status: "idle", updated_minutes_ago: 20, created_days_ago: 1, is_child: true },
    { source: "codex", title: "合成对话 · 自动化任务（搜索可见）", workspace: 0, status: "running", updated_minutes_ago: 22, created_days_ago: 1, agent_kind: "automation", is_automation: true },
  ];
  for (const seed of scenarioTasks) TASKS.push(makeTask(seed));
}

const STRESS_COUNT = Number(new URLSearchParams(location.search).get("stress") ?? 0);
if (STRESS_COUNT > 0) {
  for (let index = 0; index < 12; index += 1) {
    WORKSPACES.push({
      name: `合成工作区 ${String(index + 1).padStart(2, "0")}`,
      path: `/Users/demo/synthetic/workspace-${index + 1}`,
    });
  }
  for (let index = 0; index < STRESS_COUNT; index += 1) {
    TASKS.push(
      makeTask({
        source: SOURCES[index % SOURCES.length],
        title: `合成历史任务 ${String(index + 1).padStart(3, "0")}`,
        workspace: index % WORKSPACES.length,
        status: index % 11 === 0 ? "blocked" : "idle",
        updated_minutes_ago: 8 * 1_440 + index * 30,
        created_days_ago: 20 + (index % 40),
        archived: index % 17 === 0,
        unread: index % 29 === 0,
      }),
    );
  }
  // Synthetic recent sessions make the sidebar ordering deterministic under stress.
  for (let index = 0; index < WORKSPACES.length; index += 1) {
    const recentSessions = 1 + (index % 5);
    for (let session = 0; session < recentSessions; session += 1) {
      TASKS.push(
        makeTask({
          source: SOURCES[(index + session * 3) % SOURCES.length],
          title: `合成近期会话 ${index + 1}-${session + 1}`,
          workspace: index,
          status: "idle",
          updated_minutes_ago: 30 + index * 10 + session,
          created_days_ago: 1,
        }),
      );
    }
  }
  for (let index = 0; index < SOURCES.length; index += 1) {
    const recentSessions = index % 4;
    for (let session = 0; session < recentSessions; session += 1) {
      TASKS.push(
        makeTask({
          source: SOURCES[index],
          title: `合成来源近期会话 ${index + 1}-${session + 1}`,
          workspace: 0,
          status: "idle",
          updated_minutes_ago: 15 + index * 6 + session,
          created_days_ago: 1,
        }),
      );
    }
  }
}

function buildWorkspaces(
  excludeTitle: string | null = null,
  demoteTitle: string | null = null,
): WorkspaceGroup[] {
  return WORKSPACES.map((workspace) => {
    const tasks = TASKS.filter(
      (task) =>
        task.workspace_key === workspace.path &&
        (!excludeTitle || task.title !== excludeTitle),
    )
      .map((task) => (demoteTitle && task.title === demoteTitle ? { ...task, is_recent: false } : task))
      .sort((left, right) => (right.updated_at ?? 0) - (left.updated_at ?? 0));
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

// Mirrors perch_core::continue_work::describe_continue. The naming is demo-local
// because the fixture's seed only carries the fields that decide the channel.
function demoContinueHint(
  source: string,
  agentKind?: string,
  sourceId?: string,
): ContinueHint {
  if (source === "qoder" && sourceId && /^[A-Za-z0-9_-]{1,128}$/.test(sourceId)) {
    return { channel: "deeplink" };
  }
  if (
    source === "codex" &&
    (agentKind === "desktop" || agentKind === "automation") &&
    sourceId !== undefined &&
    UUID_PATTERN.test(sourceId)
  ) {
    return { channel: "deeplink" };
  }
  return { channel: "app" };
}

function buildSources(tasks: TaskView[]): SourceView[] {
  return SOURCES.map((source) => ({
    source,
    label: SOURCE_LABELS[source],
    status: "ok" as const,
    last_scan_at: NOW,
    last_ok_at: NOW,
    last_trigger: "demo",
    task_count: tasks.filter((task) => task.source === source).length,
    files_seen: 6,
    error: null,
  }));
}

export function demoPayload(
  options: { excludeTitle?: string | null; demoteTitle?: string | null } = {},
): BootstrapPayload {
  // excludeTitle 仅供 T8 驱动（?vanish=<标题>）：刷新后任务从快照中消失。
  // demoteTitle 仅供 T8 驱动（?filterout=<标题>）：任务仍在快照中，但被移出
  // 「最近」视图（is_recent 翻转），验证「被筛出当前筛选列表 → 关闭」。
  const excludeTitle = options.excludeTitle ?? null;
  const demoteTitle = options.demoteTitle ?? null;
  const demote = (task: TaskView): TaskView =>
    demoteTitle && task.title === demoteTitle ? { ...task, is_recent: false } : task;
  const tasks = (excludeTitle ? TASKS.filter((task) => task.title !== excludeTitle) : TASKS).map(
    demote,
  );
  const workspaces = buildWorkspaces(excludeTitle, demoteTitle);
  const visible = tasks.filter((task) => !task.source_archived);
  return {
    generated_at: NOW,
    sources: buildSources(tasks),
    workspaces,
    counts: {
      total: visible.length,
      recent: visible.filter((task) => task.is_recent).length,
      older: visible.filter((task) => !task.is_recent).length,
      archived: tasks.filter((task) => task.source_archived).length,
      attention: tasks.filter(
        (task) =>
          task.source_unread ||
          task.source_missing ||
          task.status === "failed" ||
          task.status === "blocked" ||
          task.status === "limited" ||
          task.status === "waiting" ||
          task.status === "paused" ||
          task.status === "running",
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
          matched_tasks: tasks.filter(
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
          matched_tasks: tasks.filter(
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
          matched_tasks: tasks.filter(
            (task) => task.source === "codex" && task.usage,
          ).length,
        },
      ],
    },
  };
}
