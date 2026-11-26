<script lang="ts">
  import { convertFileSrc, invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import type {
    BootstrapPayload,
    ContinueHint,
    SearchHit,
    SourceView,
    SearchResponse,
    SyncReport,
    TaskView,
    TranscriptMessage,
    TranscriptOutcome,
    UsageOverview,
    UsageReportOutcome,
  } from "./lib/types";
  import { transcriptSupported } from "./lib/transcript";
  import { sourceIconMarkup } from "./lib/source-icons";

  type TaskViewSelection = "recent" | "older" | "archived";

  interface ScanNotice {
    label: string;
    message: string;
  }

  const ICONS = {
    history: "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8M3 3v5h5M12 7v5l4 2",
    clock: "M12 7v5l3 2M21 12a9 9 0 1 1-9-9 9 9 0 0 1 9 9z",
    archive: "M4 7h16M6 7v12h12V7M9 11h6",
    refresh: "M21 12a9 9 0 1 1-2.64-6.36M21 3v6h-6",
    folder: "M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z",
    pin: "M12 17v5M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z",
    report: "M18 20V10M12 20V4M6 20v-6",
    search: "M21 21l-4.35-4.35M11 19a8 8 0 1 1 0-16 8 8 0 0 1 0 16z",
    transcript: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z",
  } as const;

  const SOURCE_COLORS: Record<string, string> = {
    qoder: "#2adb5c",
    claude: "#d97757",
    codex: "#ffffff",
    opencode: "#201d1d",
    kilo: "#ffdd00",
    "trae-cn": "#32f08c",
    cline: "#e7e5e7",
    antigravity: "#0891b2",
    minimax: "#c026d3",
    codearts: "#d946ef",
    codebuddy: "#f97316",
  };

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

  const STATUS_LABELS: Record<string, string> = {
    running: "运行中",
    active: "活跃",
    waiting: "等待中",
    paused: "已暂停",
    blocked: "受阻",
    limited: "受限",
    complete: "已完成",
    archived: "已归档",
    failed: "失败",
    idle: "空闲",
  };

  let payload = $state<BootstrapPayload | null>(null);
  let busy = $state(true);
  let refreshing = $state(false);
  let errorMessage = $state("");
  let scanNotices = $state<ScanNotice[]>([]);
  // 任务组可空：null = 全部（最近 + 更早，不含归档）。三组选中数 ∈ [1, 3]
  // 的不变量由 setTaskView / setWorkspaceFilter / setSourceFilter 集中维护。
  let view = $state<TaskViewSelection | null>("recent");
  let taskReturnView = $state<TaskViewSelection | null>("recent");
  let workspaceFilter = $state<string | null>(null);
  let sourceFilter = $state<string | null>(null);
  // 报告是一次旁路：独立于三组选中，进入清空任务组选中，返回恢复。
  let reportView = $state(false);
  let selectedKey = $state<string | null>(null);
  let continuing = $state(false);
  let continueError = $state("");
  let showAllWorkspaces = $state(false);
  let showAllSources = $state(false);
  let reportOpen = $state(false);
  let reportNotice = $state("");
  let reportFailed = $state(false);
  let reportUrl = $state("");
  let reportGeneration = $state(0);
  let reportUsageKey = $state("");
  let query = $state("");
  let searchOpen = $state(false);
  let searchIndex = $state(0);
  let searchHits = $state<SearchHit[]>([]);
  let searchTotal = $state(0);
  let searching = $state(false);
  let searchError = $state("");
  let searchInput = $state<HTMLInputElement>();
  let searchResultList = $state<HTMLDivElement>();
  let listLimit = $state(60);
  let searchLimit = $state(50);
  // 对话面板（#27）：按需读取、只在内存中；打开时替换第 2、3 列，侧栏保持可交互。
  // transcriptKey 是唯一开关；读取 Err 与 Outcome error 统一为面板内一行原因。
  type TranscriptPhase = "loading" | "ready";
  const TRANSCRIPT_PAGE_SIZE = 50;
  let transcriptKey = $state<string | null>(null);
  let transcriptPhase = $state<TranscriptPhase>("loading");
  let transcriptOutcome = $state<TranscriptOutcome | null>(null);
  let transcriptShown = $state(TRANSCRIPT_PAGE_SIZE);
  let transcriptSeq = 0;
  let transcriptMessagesEl = $state<HTMLDivElement>();

  const NAV_VISIBLE_LIMIT = 8;
  const RECENT_WINDOW_SECONDS = 7 * 24 * 60 * 60;
  const TASK_PAGE_SIZE = 60;
  const SEARCH_PAGE_SIZE = 50;

  function matchesView(task: TaskView, mode: TaskViewSelection | null): boolean {
    if (mode === null) return !task.source_archived;
    if (mode === "archived") return task.source_archived;
    return !task.source_archived && task.is_recent === (mode === "recent");
  }

  function primaryTask(task: TaskView): boolean {
    return !task.is_child && !task.is_automation;
  }

  const allPrimaryTasks = $derived.by(() =>
    (payload?.workspaces ?? []).flatMap((group) => group.tasks).filter(primaryTask),
  );

  const countView = $derived<TaskViewSelection | null>(
    reportView ? taskReturnView : view,
  );
  interface ActivityRank {
    recentCount: number;
    lastActivity: number;
  }

  const activityIndex = $derived.by(() => {
    const workspaces = new Map<string, ActivityRank>();
    const sources = new Map<string, ActivityRank>();
    const now = payload?.generated_at ?? Date.now() / 1000;
    const cutoff = now - RECENT_WINDOW_SECONDS;
    const rank = (map: Map<string, ActivityRank>, key: string, task: TaskView) => {
      const activity = task.updated_at ?? task.created_at ?? 0;
      const entry = map.get(key) ?? { recentCount: 0, lastActivity: 0 };
      if (activity >= cutoff) entry.recentCount += 1;
      entry.lastActivity = Math.max(entry.lastActivity, activity);
      map.set(key, entry);
    };
    for (const task of allPrimaryTasks) {
      if (task.source_archived || task.source_missing) continue;
      rank(workspaces, task.workspace_key, task);
      rank(sources, task.source, task);
    }
    return { workspaces, sources };
  });

  function activityOf(
    map: Map<string, ActivityRank>,
    key: string,
  ): ActivityRank {
    return map.get(key) ?? { recentCount: 0, lastActivity: 0 };
  }

  function compareActivity(left: ActivityRank, right: ActivityRank): number {
    return (
      right.recentCount - left.recentCount ||
      right.lastActivity - left.lastActivity
    );
  }


  function tasksMatching(
    mode: TaskViewSelection | null,
    workspace: string | null,
    source: string | null,
  ): TaskView[] {
    return allPrimaryTasks.filter(
      (task) =>
        matchesView(task, mode) &&
        (!workspace || task.workspace_key === workspace) &&
        (!source || task.source === source),
    );
  }

  const smartLists = $derived([
    { id: "recent" as TaskViewSelection, label: "最近", icon: ICONS.clock, count: tasksMatching("recent", workspaceFilter, sourceFilter).length },
    { id: "older" as TaskViewSelection, label: "更早", icon: ICONS.history, count: tasksMatching("older", workspaceFilter, sourceFilter).length },
    { id: "archived" as TaskViewSelection, label: "已归档", icon: ICONS.archive, count: tasksMatching("archived", workspaceFilter, sourceFilter).length },
  ]);

  const workspaceItems = $derived.by(() => {
    const all = (payload?.workspaces ?? [])
      .map((group) => ({
        key: group.key,
        name: group.name,
        path: group.project_path,
        count: tasksMatching(countView, group.key, sourceFilter).length,
        activity: activityOf(activityIndex.workspaces, group.key),
      }))
      .filter((item) => item.count > 0);
    return all
      .map((item) => ({ ...item, label: item.name }))
      .sort(
        (left, right) =>
          compareActivity(left.activity, right.activity) ||
          left.name.localeCompare(right.name),
      );
  });

  const visibleWorkspaces = $derived.by(() => {
    if (showAllWorkspaces || workspaceItems.length <= NAV_VISIBLE_LIMIT) {
      return workspaceItems;
    }
    const visible = workspaceItems.slice(0, NAV_VISIBLE_LIMIT);
    if (workspaceFilter && !visible.some((item) => item.key === workspaceFilter)) {
      const active = workspaceItems.find((item) => item.key === workspaceFilter);
      if (active) visible.push(active);
    }
    return visible;
  });

  const hiddenWorkspaceCount = $derived(
    Math.max(0, workspaceItems.length - visibleWorkspaces.length),
  );

  const canToggleWorkspaces = $derived(workspaceItems.length > NAV_VISIBLE_LIMIT);

  const sourceItems = $derived.by(() =>
    (payload?.sources ?? [])
      .map((source) => ({
        ...source,
        count: tasksMatching(countView, workspaceFilter, source.source).length,
        activity: activityOf(activityIndex.sources, source.source),
      }))
      .sort(
        (left, right) =>
          compareActivity(left.activity, right.activity) ||
          left.label.localeCompare(right.label),
      ),
  );

  const visibleSources = $derived.by(() => {
    if (showAllSources || sourceItems.length <= NAV_VISIBLE_LIMIT) {
      return sourceItems;
    }
    const visible = sourceItems.slice(0, NAV_VISIBLE_LIMIT);
    if (sourceFilter && !visible.some((item) => item.source === sourceFilter)) {
      const active = sourceItems.find((item) => item.source === sourceFilter);
      if (active) visible.push(active);
    }
    return visible;
  });

  const hiddenSourceCount = $derived(
    Math.max(0, sourceItems.length - visibleSources.length),
  );

  const canToggleSources = $derived(sourceItems.length > NAV_VISIBLE_LIMIT);

  function compareTaskOrder(left: TaskView, right: TaskView): number {
    const leftRunning = left.status === "running";
    const rightRunning = right.status === "running";
    if (leftRunning !== rightRunning) return leftRunning ? -1 : 1;
    if (left.source_pinned !== right.source_pinned) return left.source_pinned ? -1 : 1;
    return (right.updated_at ?? 0) - (left.updated_at ?? 0);
  }

  const flatTasks = $derived.by(() => {
    if (!payload) return [] as TaskView[];
    const tasks: TaskView[] = [];
    for (const group of payload.workspaces) {
      if (workspaceFilter && group.key !== workspaceFilter) continue;
      for (const task of group.tasks) {
        const isSelected = task.key === selectedKey;
        if (!primaryTask(task) && !isSelected) continue;
        if (!matchesView(task, view)) continue;
        if (sourceFilter && task.source !== sourceFilter) continue;
        tasks.push(task);
      }
    }
    return tasks.sort(compareTaskOrder);
  });

  const renderedTasks = $derived(flatTasks.slice(0, listLimit));
  const hasMoreTasks = $derived(flatTasks.length > renderedTasks.length);
  const totalVisible = $derived(flatTasks.length);
  const hasVisibleTasks = $derived(flatTasks.length > 0);

  const selected = $derived.by<TaskView | null>(() =>
    flatTasks.find((task) => task.key === selectedKey) ?? null,
  );

  const continueHint = $derived<ContinueHint | null>(selected?.continue_hint ?? null);

  // The verb carries the channel: "打开" means the session is located,
  // "激活" means the app only comes forward. Wording lives here, not in Rust.
  // A missing hint means a source was never registered in `plan_continue`; the
  // button still renders so the click surfaces that error instead of hiding the
  // action, and the neutral verb promises neither locating nor activating.
  const continueVerb = $derived(
    continueHint === null ? "继续" : continueHint.channel === "app" ? "激活" : "打开",
  );
  const continueButtonLabel = $derived(
    continuing
      ? `${continueVerb}中…`
      : `${continueVerb} ${selected?.source_label ?? "App"}`.trim(),
  );

  const searchScope = $derived.by(() => {
    const parts: string[] = [];
    if (workspaceFilter) {
      parts.push(
        `Workspace：${workspaceItems.find((item) => item.key === workspaceFilter)?.name ?? workspaceFilter}`,
      );
    }
    if (sourceFilter) parts.push(`来源：${SOURCE_LABELS[sourceFilter] ?? sourceFilter}`);
    return parts;
  });

  const searchScopeLabel = $derived(
    searchScope.length > 0
      ? `作用域：${searchScope.join(" · ")}`
      : "搜索所有会话（含归档）",
  );

  function searchScopeTitle(): string {
    return searchScope.length > 0
      ? `当前仅搜索 ${searchScope.join("、")}；视图不影响搜索，仍包含归档 / 子会话 / 自动化`
      : "忽略最近 / 更早 / 已归档视图，搜索全部会话（含归档 / 子会话 / 自动化）";
  }

  const allTasks = $derived.by(() => {
    const map = new Map<string, TaskView>();
    for (const group of payload?.workspaces ?? []) {
      for (const task of group.tasks) map.set(task.key, task);
    }
    return map;
  });

  const searchActive = $derived(query.trim().length > 0);

  // —— 对话面板（#27）——
  const transcriptTask = $derived<TaskView | null>(
    transcriptKey ? (allTasks.get(transcriptKey) ?? null) : null,
  );
  // 打开中 = key 存在且任务仍在快照里；刷新后任务消失会先由 refresh 清理 key。
  const transcriptOpen = $derived(transcriptKey !== null && transcriptTask !== null);
  const transcriptMessages = $derived<TranscriptMessage[]>(
    transcriptPhase === "ready" && transcriptOutcome?.status === "ready"
      ? transcriptOutcome.messages
      : [],
  );
  const transcriptSessionTruncated = $derived(
    transcriptPhase === "ready" && transcriptOutcome?.status === "ready"
      ? transcriptOutcome.session_truncated
      : false,
  );
  // 正序渲染；「最近 50 条」= 尾部切片，加载更早向前扩窗。
  const transcriptVisible = $derived(
    transcriptMessages.slice(Math.max(0, transcriptMessages.length - transcriptShown)),
  );
  const transcriptEarlierCount = $derived(
    transcriptMessages.length - transcriptVisible.length,
  );
  const transcriptReason = $derived(
    transcriptPhase === "ready" && transcriptOutcome?.status === "error"
      ? transcriptOutcome.reason
      : "",
  );

  function closeTranscript() {
    transcriptSeq += 1; // 使在途读取失效
    transcriptKey = null;
    transcriptPhase = "loading";
    transcriptOutcome = null;
    transcriptShown = TRANSCRIPT_PAGE_SIZE;
  }

  async function loadTranscript(key: string, seq: number) {
    transcriptPhase = "loading";
    transcriptOutcome = null;
    transcriptShown = TRANSCRIPT_PAGE_SIZE;
    try {
      const outcome = await invoke<TranscriptOutcome>("read_session_content", { taskKey: key });
      if (seq !== transcriptSeq || transcriptKey !== key) return;
      transcriptOutcome = outcome;
      transcriptPhase = "ready";
      // 首屏定位最新消息（正序渲染，最新在底部）。
      void tick().then(() => {
        const el = transcriptMessagesEl;
        if (el) el.scrollTop = el.scrollHeight;
      });
    } catch (error) {
      if (seq !== transcriptSeq || transcriptKey !== key) return;
      // Err 三变体（非法 key / 任务不存在 / 来源不支持）与读取失败同权：面板内一行原因。
      transcriptOutcome = {
        status: "error",
        reason: error instanceof Error ? error.message : String(error),
      };
      transcriptPhase = "ready";
    }
  }

  function openTranscript(key: string) {
    selectTask(key);
    const seq = ++transcriptSeq;
    transcriptKey = key;
    void loadTranscript(key, seq);
  }

  function rereadTranscript() {
    if (!transcriptKey) return;
    void loadTranscript(transcriptKey, ++transcriptSeq);
  }

  // 追加更早消息后把滚动锚回原来的阅读位置：新增高度只出现在顶部。
  function showEarlierMessages() {
    const el = transcriptMessagesEl;
    const beforeHeight = el?.scrollHeight ?? 0;
    const beforeTop = el?.scrollTop ?? 0;
    transcriptShown += TRANSCRIPT_PAGE_SIZE;
    void tick().then(() => {
      const node = transcriptMessagesEl;
      if (node) node.scrollTop = node.scrollHeight - beforeHeight + beforeTop;
    });
  }

  const searchResults = $derived.by(() => {
    const results: Array<{ hit: SearchHit; task: TaskView }> = [];
    for (const hit of searchHits) {
      const task = allTasks.get(hit.task_key);
      if (task) results.push({ hit, task });
    }
    return results;
  });

  const renderedSearchResults = $derived(searchResults.slice(0, searchLimit));
  const hasMoreSearchResults = $derived(searchResults.length > renderedSearchResults.length);

  function showMoreSearchResults() {
    searchLimit += SEARCH_PAGE_SIZE;
  }

  function showMoreTasks() {
    listLimit += TASK_PAGE_SIZE;
  }

  let searchSeq = 0;
  $effect(() => {
    const term = query.trim();
    const source = sourceFilter;
    const workspace = workspaceFilter;
    void payload?.generated_at;
    const seq = ++searchSeq;
    searchLimit = SEARCH_PAGE_SIZE;
    if (!term) {
      searchHits = [];
      searchTotal = 0;
      searchIndex = 0;
      searching = false;
      searchError = "";
      return;
    }
    searching = true;
    searchError = "";
    const timer = setTimeout(() => {
      void (async () => {
        try {
          const response = await invoke<SearchResponse>("search_tasks", {
            query: term,
            source,
            workspace,
          });
          if (seq !== searchSeq) return;
          searchHits = response.hits;
          searchTotal = response.total;
          searchIndex = 0;
        } catch (error) {
          if (seq !== searchSeq) return;
          searchHits = [];
          searchTotal = 0;
          searchIndex = 0;
          searchError = error instanceof Error ? error.message : String(error);
        } finally {
          if (seq === searchSeq) searching = false;
        }
      })();
    }, 150);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    void view;
    void workspaceFilter;
    void sourceFilter;
    listLimit = TASK_PAGE_SIZE;
  });

  const selectedUsage = $derived(selected?.usage ?? null);
  const selectedToken = $derived(selectedUsage?.token ?? null);
  const selectedCredits = $derived(selectedUsage?.credits ?? null);
  const selectedEst = $derived(selectedUsage?.est ?? null);

  // Sources do not share a unit: Qoder reports credits plus estimated tokens,
  // while the CLI agents report token truth. A mixed summary would be
  // meaningless, so only show totals once a single source is selected.
  const sourceUsage = $derived(
    sourceFilter
      ? (payload?.usage?.sources.find((item) => item.source === sourceFilter) ?? null)
      : null,
  );

  const heading = $derived.by(() => {
    if (reportView) return "用量报告";
    const parts: string[] = [];
    if (workspaceFilter) {
      parts.push(
        workspaceItems.find((item) => item.key === workspaceFilter)?.label ??
          payload?.workspaces.find((group) => group.key === workspaceFilter)?.name ??
          "Workspace",
      );
    }
    if (sourceFilter) parts.push(SOURCE_LABELS[sourceFilter] ?? sourceFilter);
    if (parts.length > 0) return parts.join(" · ");
    return smartLists.find((item) => item.id === view)?.label ?? "任务";
  });

  const subheading = $derived.by(() => {
    if (reportView) return "usage-ledger 生成 · 全局报告 · 不随任务筛选变化";
    const generated = payload ? fullTime(payload.generated_at) : "—";
    const shown = Math.min(totalVisible, listLimit);
    const count = totalVisible > shown ? `${shown} / ${totalVisible} 个任务` : `${totalVisible} 个任务`;
    return `${count} · 快照 ${generated}`;
  });

  async function loadBootstrap() {
    payload = await invoke<BootstrapPayload>("bootstrap");
  }

  async function refresh() {
    if (refreshing) return;
    refreshing = true;
    errorMessage = "";
    try {
      const report = await invoke<SyncReport>("refresh_sources", { source: null });
      scanNotices = report.reports
        .filter((item) => item.error)
        .map((item) => ({
          label: SOURCE_LABELS[item.source] ?? item.source,
          message: item.error as string,
        }));
      await loadBootstrap();
      if (selectedKey && !flatTasks.some((task) => task.key === selectedKey)) {
        selectedKey = null;
      }
      // 刷新后任务不在当前筛选列表：关闭对话并清除选中（T8，与列表选中规则一致）。
      if (transcriptKey && !flatTasks.some((task) => task.key === transcriptKey)) {
        closeTranscript();
        if (selectedKey) selectedKey = null;
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      refreshing = false;
      busy = false;
    }
  }

  function clearSelection() {
    selectedKey = null;
    continueError = "";
  }

  function selectTask(key: string) {
    selectedKey = key;
    continueError = "";
  }

  async function continueTask() {
    if (!selected || continuing) return;
    const taskKey = selected.key;
    continuing = true;
    continueError = "";
    try {
      // Success stays silent: the button already states the fixed channel.
      await invoke("continue_task", { taskKey });
    } catch (error) {
      continueError = error instanceof Error ? error.message : String(error);
    } finally {
      continuing = false;
    }
  }

  function usageContentKey(usage: UsageOverview | undefined): string {
    if (!usage) return "";
    // matched_tasks follows task matching, as_of follows file mtimes; neither
    // reflects usage content, so both are excluded from the change gate.
    return JSON.stringify(
      usage.sources.map((item) => [
        item.source,
        item.token_total,
        item.token_7d,
        item.credits_billable_7d,
        item.est_total,
        item.last_day,
      ]),
    );
  }

  // 三组选中数 ∈ [1, 3] 的唯一守门入口：只有当被取消的组是当前唯一选中组时
  // 才拒绝取消（保持原选中）。选中转移与多组并存下的取消都从这里经过。
  function selectedGroupCount(
    viewSelection: TaskViewSelection | null,
    workspace: string | null,
    source: string | null,
  ): number {
    return (
      (viewSelection !== null ? 1 : 0) +
      (workspace !== null ? 1 : 0) +
      (source !== null ? 1 : 0)
    );
  }

  function setTaskView(next: TaskViewSelection | null) {
    if (
      view !== null &&
      next === null &&
      selectedGroupCount(view, workspaceFilter, sourceFilter) === 1
    ) {
      return;
    }
    view = next;
    if (!reportView) taskReturnView = next;
  }

  function setWorkspaceFilter(next: string | null) {
    if (
      workspaceFilter !== null &&
      next === null &&
      selectedGroupCount(view, workspaceFilter, sourceFilter) === 1
    ) {
      return;
    }
    workspaceFilter = next;
  }

  function setSourceFilter(next: string | null) {
    if (
      sourceFilter !== null &&
      next === null &&
      selectedGroupCount(view, workspaceFilter, sourceFilter) === 1
    ) {
      return;
    }
    sourceFilter = next;
  }

  async function generateUsageReport(silent: boolean) {
    if (reportOpen) return;
    reportOpen = true;
    const usageKey = usageContentKey(payload?.usage);
    try {
      const outcome = await invoke<UsageReportOutcome>("open_usage_report");
      reportUrl = convertFileSrc(outcome.path);
      reportGeneration += 1;
      reportUsageKey = usageKey;
      reportNotice = "";
      reportFailed = false;
    } catch (error) {
      if (!silent) {
        reportFailed = true;
        reportNotice = error instanceof Error ? error.message : String(error);
      }
    } finally {
      reportOpen = false;
    }
  }

  function openUsageReport() {
    closeTranscript();
    taskReturnView = view;
    // 报告不计入三组：进入时清空任务组选中（workspaces / 来源筛选保留），
    // 返回时经 taskReturnView 恢复原任务视图。
    view = null;
    reportView = true;
    clearSelection();
    closeSearch();
    void generateUsageReport(false);
  }

  function returnToTasks() {
    reportView = false;
    view = taskReturnView;
    clearSelection();
    reportNotice = "";
    reportFailed = false;
  }

  function selectView(next: TaskViewSelection) {
    // 任务组点击总是退出报告；组内转移或取消经 setTaskView 守门。
    closeTranscript();
    reportView = false;
    setTaskView(view === next ? null : next);
    clearSelection();
    closeSearch();
  }

  function openSearch() {
    searchOpen = true;
    searchIndex = 0;
    void tick().then(() => {
      searchInput?.focus();
      searchInput?.select();
    });
  }

  function closeSearch() {
    searchOpen = false;
    query = "";
    searchHits = [];
    searchTotal = 0;
    searchIndex = 0;
    searching = false;
    searchError = "";
    searchInput?.blur();
  }

  function openSearchResult(task: TaskView) {
    revealTask(task);
    closeSearch();
  }

  function revealTask(task: TaskView) {
    const nextView: TaskViewSelection = task.source_archived
      ? "archived"
      : task.is_recent
        ? "recent"
        : "older";
    closeTranscript();
    reportView = false;
    setTaskView(nextView);
    workspaceFilter = task.workspace_key;
    setSourceFilter(null);
    selectTask(task.key);
    void tick().then(() => {
      const index = flatTasks.findIndex((item) => item.key === task.key);
      if (index >= listLimit) listLimit = Math.ceil((index + 1) / TASK_PAGE_SIZE) * TASK_PAGE_SIZE;
      void tick().then(() => {
        document
          .querySelector(".flat-list .task.selected")
          ?.scrollIntoView({ block: "nearest" });
      });
    });
  }

  function moveSearchSelection(delta: number) {
    if (searchResults.length === 0) return;
    searchIndex =
      (searchIndex + delta + searchResults.length) % searchResults.length;
    if (searchIndex >= searchLimit) {
      searchLimit = Math.ceil((searchIndex + 1) / SEARCH_PAGE_SIZE) * SEARCH_PAGE_SIZE;
    }
    void tick().then(() => {
      const item = searchResultList?.querySelector<HTMLElement>(
        `.search-result[data-index="${searchIndex}"]`,
      );
      item?.scrollIntoView({ block: "nearest" });
    });
  }

  function openActiveSearchResult() {
    const result = searchResults[searchIndex];
    if (result) openSearchResult(result.task);
  }

  function selectWorkspace(key: string) {
    closeTranscript();
    setWorkspaceFilter(workspaceFilter === key ? null : key);
    clearSelection();
  }

  function selectSource(source: string) {
    closeTranscript();
    setSourceFilter(sourceFilter === source ? null : source);
    clearSelection();
  }

  function clearSearchScope() {
    // 清除作用域只针对 workspace / 来源；若任务组本来就未选中，
    // 清除会让三组全空，此时任务组回落「最近」而不是保持空。
    closeTranscript();
    if (view === null) setTaskView("recent");
    setWorkspaceFilter(null);
    setSourceFilter(null);
    clearSelection();
    void tick().then(() => searchInput?.focus());
  }

  function sourceStatusLabel(status: string): string {
    const labels: Record<string, string> = {
      ok: "正常",
      partial: "部分扫描",
      unavailable: "不可用",
      error: "错误",
      unknown: "未扫描",
    };
    return labels[status] ?? status;
  }

  function sourceTitle(source: SourceView & { count?: number }): string {
    const parts = [
      `${source.label}：${sourceStatusLabel(source.status)}`,
      `当前视图 / workspace 下 ${source.count ?? 0} 个任务`,
    ];
    if (source.error) parts.push(source.error);
    return parts.join("；");
  }

  function sourceColor(source: string): string {
    return SOURCE_COLORS[source] ?? "#8e8e93";
  }

  // Picks the label colour with the better WCAG contrast against the source
  // brand fill. Almost every brand is light enough for the dark ink — Codex
  // white, Cline grey, Kilo yellow, but also Claude's brown and CodeArts'
  // magenta; only the near-black OpenCode fill and MiniMax's dark magenta take
  // white. Every current source clears 4.5:1 this way, so the brand colour
  // itself never has to be darkened.
  function contrastText(hex: string): string {
    const value = hex.replace("#", "");
    const channels = [0, 2, 4].map((offset) => {
      const channel = parseInt(value.slice(offset, offset + 2), 16) / 255;
      return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
    });
    const luminance = 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
    const dark = 0.0104; // #1d1d1f
    const light = 1;
    return (light + 0.05) / (luminance + 0.05) > (luminance + 0.05) / (dark + 0.05)
      ? "#ffffff"
      : "#1d1d1f";
  }

  function statusLabel(task: TaskView): string {
    return STATUS_LABELS[task.status] ?? task.status;
  }

  function statusClass(task: TaskView): string {
    if (task.status === "running" || task.status === "active") return "live";
    if (task.status === "blocked" || task.status === "failed") return "danger";
    if (task.status === "limited" || task.status === "paused" || task.status === "waiting") {
      return "warn";
    }
    if (task.status === "complete") return "done";
    if (task.status === "archived") return "muted";
    return "idle";
  }

  function todoLabel(status: string): string {
    const labels: Record<string, string> = {
      pending: "待处理",
      in_progress: "进行中",
      completed: "已完成",
      cancelled: "已取消",
      unknown: "未知",
    };
    return labels[status] ?? status;
  }

  function relativeTime(seconds: number | null | undefined): string {
    if (!seconds) return "时间未知";
    const diff = Date.now() / 1000 - seconds;
    if (diff < 60) return "刚刚";
    if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
    if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
    if (diff < 86400 * 7) return `${Math.floor(diff / 86400)} 天前`;
    return new Date(seconds * 1000).toLocaleDateString("zh-CN", {
      month: "short",
      day: "numeric",
    });
  }

  function fullTime(seconds: number | null | undefined): string {
    if (!seconds) return "—";
    return new Date(seconds * 1000).toLocaleString("zh-CN", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function formatTokens(value: number): string {
    if (!Number.isFinite(value) || value <= 0) return "0";
    if (value >= 1_000_000_000) return `${(value / 1_000_000_000).toFixed(2)}B`;
    if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(2)}M`;
    if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
    return String(Math.round(value));
  }

  function formatCredits(value: number): string {
    return value.toLocaleString("zh-CN", { maximumFractionDigits: 2 });
  }

  function joinModels(models: string[]): string {
    return models.length > 0 ? models.join(" · ") : "模型未知";
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.isComposing) return;
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
      event.preventDefault();
      openSearch();
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "r") {
      event.preventDefault();
      if (reportView) void generateUsageReport(false);
      else void refresh();
    }
    if (searchOpen) {
      if (event.key === "Escape") {
        event.preventDefault();
        closeSearch();
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        moveSearchSelection(1);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        moveSearchSelection(-1);
      } else if (event.key === "Enter") {
        event.preventDefault();
        openActiveSearchResult();
      }
      return;
    }
    if (event.key === "Escape") {
      // 对话面板优先于清空选中：Esc 只关对话，任务保持选中（T6）。
      if (transcriptKey) closeTranscript();
      else selectedKey = null;
    }
  }

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void (async () => {
      try {
        await loadBootstrap();
      } catch (error) {
        errorMessage = error instanceof Error ? error.message : String(error);
      }
      await refresh();
    })();

    void listen("sources-updated", () => {
      if (disposed) return;
      void (async () => {
        try {
          await loadBootstrap();
        } catch {
          return;
        }
        if (disposed) return;
        // 实时刷新后任务不在当前列表：关闭对话（T8）；选中沿用既有规则。
        if (transcriptKey && !flatTasks.some((task) => task.key === transcriptKey)) {
          closeTranscript();
        }
        if (!reportView) return;
        // `usage html` rewrites db mtimes on every run, so as_of cannot gate
        // regeneration; only a change in usage totals reloads the iframe.
        if (usageContentKey(payload?.usage) !== reportUsageKey) {
          void generateUsageReport(true);
        }
      })();
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });


    return () => {
      disposed = true;
      unlisten?.();
    };
  });
</script>

<svelte:window onkeydown={handleKeydown} />

{#snippet pinIcon()}
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d={ICONS.pin} />
  </svg>
{/snippet}

{#snippet appIcon(uid: string)}
  <!-- 与 src-tauri/icons/icon.svg 同一矢量图形；渐变 id 加位置后缀避免同页重复 -->
  <svg viewBox="0 0 1024 1024" role="img" aria-label="Perch">
    <defs>
      <linearGradient id={`perch-icon-bg-${uid}`} x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#e2a56f" />
        <stop offset="1" stop-color="#a35626" />
      </linearGradient>
      <radialGradient id={`perch-icon-glow-${uid}`} cx="0.3" cy="0.2" r="0.9">
        <stop offset="0" stop-color="#ffffff" stop-opacity="0.22" />
        <stop offset="1" stop-color="#ffffff" stop-opacity="0" />
      </radialGradient>
    </defs>
    <g transform="translate(100 100) scale(0.8046875)">
      <rect width="1024" height="1024" rx="228" fill={`url(#perch-icon-bg-${uid})`} />
      <rect width="1024" height="1024" rx="228" fill={`url(#perch-icon-glow-${uid})`} />
      <rect x="188" y="706" width="648" height="44" rx="22" fill="#6f3a1a" />
      <path d="M356 548 L176 646 L320 690 Z" fill="#f7ead9" />
      <ellipse cx="512" cy="566" rx="196" ry="174" fill="#fbeedd" />
      <circle cx="660" cy="386" r="122" fill="#fbeedd" />
      <path d="M772 372 L872 406 L770 442 Z" fill="#f2b45f" />
      <circle cx="700" cy="360" r="15" fill="#3a2317" />
      <circle cx="705" cy="355" r="4.5" fill="#ffffff" opacity="0.85" />
      <path d="M424 556c64-44 170-44 232 0-42 96-182 106-232 0z" fill="#ecd7bf" />
      <path d="M492 704v34M566 704v34" stroke="#6f3a1a" stroke-width="20" stroke-linecap="round" />
      <path d="M472 738h44M546 738h44" stroke="#6f3a1a" stroke-width="18" stroke-linecap="round" />
    </g>
  </svg>
{/snippet}

{#snippet taskRow(task: TaskView, showWorkspace: boolean)}
  <div class="task-row">
    <button
      class="task"
      class:selected={selectedKey === task.key}
      onclick={() => selectTask(task.key)}
    >
      <span class="task-main">
        <span class="task-title">
          {#if task.source_unread}<span class="unread-dot"></span>{/if}
          {task.title}
        </span>
        <span class="task-meta">
          {#if showWorkspace}
            <span class="task-workspace" title={task.project_path ?? task.workspace_name}>
              {task.workspace_name}
            </span>
            <span class="dot-sep">·</span>
          {/if}
          <span>{task.source_label}</span>
          <span class="dot-sep">·</span>
          <span>{relativeTime(task.updated_at)}</span>
          {#if task.todo_total > 0}
            <span class="todo-chip">{task.todo_done}/{task.todo_total}</span>
          {/if}
          {#if task.source_pinned}<span class="pin-badge" title="源置顶" aria-label="源置顶">{@render pinIcon()}</span>{/if}
          {#if task.is_child}<span class="mini-badge">子会话</span>{/if}
          {#if task.is_automation}<span class="mini-badge">自动化</span>{/if}
          {#if task.source_missing}<span class="mini-badge danger">源已删除</span>{/if}
        </span>
      </span>
      <span class="status {statusClass(task)}">{statusLabel(task)}</span>
    </button>
    {#if transcriptSupported(task.source)}
      <button
        class="task-transcript"
        aria-label="查看对话"
        title="查看对话"
        onclick={() => openTranscript(task.key)}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d={ICONS.transcript} />
        </svg>
      </button>
    {/if}
  </div>
{/snippet}

<div class="app">
  <aside class="sidebar">
    <header class="brand">
      <span class="brand-mark">
        {@render appIcon("brand")}
      </span>
      <span class="brand-text">
        <strong>Perch</strong>
        <small>多 agent 任务总览</small>
      </span>
    </header>

    <div class="sidebar-scroll">
      <nav class="nav-section">
        <p class="section-title">任务</p>
        {#each smartLists as item (item.id)}
          <button
            class="nav-item"
            class:active={view === item.id}
            aria-pressed={view === item.id}
            onclick={() => selectView(item.id)}
          >
            <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
              <path d={item.icon} />
            </svg>
            <span class="nav-label">{item.label}</span>
            <span class="nav-count">{item.count}</span>
          </button>
        {/each}
        <button
          class="nav-item search-entry"
          class:active={searchOpen}
          aria-pressed={searchOpen}
          title="搜索任务（⌘F）"
          onclick={openSearch}
        >
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
            <path d={ICONS.search} />
          </svg>
          <span class="nav-label">搜索</span>
          <kbd>⌘F</kbd>
        </button>
      </nav>

      <nav class="nav-section">
        <p class="section-title">Workspaces</p>
        {#each visibleWorkspaces as workspace (workspace.key)}
          <button
            class="nav-item"
            class:active={workspaceFilter === workspace.key}
            aria-pressed={workspaceFilter === workspace.key}
            data-recent-count={workspace.activity.recentCount}
            data-last-activity={workspace.activity.lastActivity}
            title={workspace.path ?? workspace.name}
            onclick={() => selectWorkspace(workspace.key)}
          >
            <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
              <path d={ICONS.folder} />
            </svg>
            <span class="nav-label">{workspace.label}</span>
            <span class="nav-count">{workspace.count}</span>
          </button>
        {/each}
        {#if canToggleWorkspaces}
          <button
            class="nav-more"
            aria-expanded={showAllWorkspaces}
            onclick={() => (showAllWorkspaces = !showAllWorkspaces)}
          >
            {showAllWorkspaces ? "收起" : `显示更多（${hiddenWorkspaceCount}）`}
          </button>
        {/if}
        {#if workspaceItems.length === 0}
          <p class="nav-empty">暂无 workspace</p>
        {/if}
      </nav>

      <nav class="nav-section sources">
        <p class="section-title">来源</p>
        {#each visibleSources as source (source.source)}
          <button
            class="nav-item source-item"
            class:active={sourceFilter === source.source}
            class:degraded={source.status !== "ok"}
            aria-pressed={sourceFilter === source.source}
            style={`--source:${sourceColor(source.source)}`}
            data-recent-count={source.activity.recentCount}
            data-last-activity={source.activity.lastActivity}
            aria-label={`${source.label}，${sourceStatusLabel(source.status)}，当前视图 ${source.count} 个任务`}
            title={sourceTitle(source)}
            onclick={() => selectSource(source.source)}
          >
            <span class="source-icon">{@html sourceIconMarkup(source.source)}</span>
            <span class="nav-label">{source.label}</span>
            {#if source.status !== "ok"}
              <span class="source-status" aria-label={`来源状态：${sourceStatusLabel(source.status)}`}>
                {sourceStatusLabel(source.status)}
              </span>
            {/if}
            <span class="nav-count" title="当前视图 / workspace 下可点击任务数">{source.count}</span>
            {#if source.error && source.status !== "unavailable"}
              <span class="source-warning" aria-hidden="true">!</span>
            {/if}
          </button>
        {/each}
        {#if canToggleSources}
          <button
            class="nav-more"
            aria-expanded={showAllSources}
            onclick={() => (showAllSources = !showAllSources)}
          >
            {showAllSources ? "收起" : `显示更多（${hiddenSourceCount}）`}
          </button>
        {/if}
      </nav>
    </div>

    <footer class="sidebar-foot">
      {#if payload?.usage}
        <button
          class="sidebar-report"
          class:active={reportView}
          onclick={openUsageReport}
          disabled={reportOpen}
          title={reportOpen
            ? "正在生成用量报告"
            : reportView
              ? "重新生成用量报告"
              : "在应用内查看 usage-ledger 用量报告"}
          aria-label={reportOpen
            ? "正在生成用量报告"
            : reportView
              ? "重新生成用量报告"
              : "在应用内查看 usage-ledger 用量报告"}
          aria-current={reportView ? "page" : undefined}
          aria-pressed={reportView}
        >
          <svg
            class="report-icon"
            class:spinning={reportOpen}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d={reportOpen ? ICONS.refresh : ICONS.report} />
          </svg>
        </button>
      {:else}
        <button
          class="sidebar-report unavailable"
          aria-label="用量报告不可用：未检测到 usage-ledger 用量数据"
          title="未检测到 usage-ledger 用量数据；安装并同步后可在应用内查看报告"
          onclick={openUsageReport}
        >
          <svg class="report-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d={ICONS.report} />
          </svg>
        </button>
      {/if}
    </footer>
  </aside>

  {#if !transcriptOpen}
  <main class="list-pane" class:report-wide={reportView}>
    <header class="list-head">
      <div class="head-text">
        <h2>{heading}</h2>
        <p>{subheading}</p>
      </div>
      {#if reportView}
        <button class="refresh-button" onclick={returnToTasks} title="返回任务视图">
          <span>返回任务</span>
        </button>
        <button
          class="refresh-button"
          onclick={() => void generateUsageReport(false)}
          disabled={reportOpen}
        >
          <svg
            class="refresh-icon"
            class:spinning={reportOpen}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d={ICONS.refresh} />
          </svg>
          <span>{reportOpen ? "生成中" : "重新生成"}</span>
          <kbd>⌘R</kbd>
        </button>
      {:else}
        <button class="refresh-button" onclick={() => refresh()} disabled={refreshing}>
          <svg
            class="refresh-icon"
            class:spinning={refreshing}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d={ICONS.refresh} />
          </svg>
          <span>{refreshing ? "扫描中" : "刷新"}</span>
          <kbd>⌘R</kbd>
        </button>
      {/if}

      {#if sourceUsage}
        <div class="usage-strip">
          {#if sourceUsage.token_7d > 0}
            <span class="usage-pill" title="usage-ledger token 真值（近 7 天）">
              <strong>{formatTokens(sourceUsage.token_7d)}</strong><span>tokens · 7d</span>
            </span>
          {:else if sourceUsage.token_total > 0}
            <span class="usage-pill" title="usage-ledger token 真值（全量）">
              <strong>{formatTokens(sourceUsage.token_total)}</strong><span>tokens · 全量</span>
            </span>
          {/if}
          {#if sourceUsage.credits_billable_7d > 0}
            <span class="usage-pill credits" title="Qoder 服务端计费，非 token">
              <strong>{formatCredits(sourceUsage.credits_billable_7d)}</strong><span>credits · 7d</span>
            </span>
          {/if}
          {#if sourceUsage.est_total > 0}
            <span class="usage-pill est" title="估算 token（全量），不可与真值相加">
              <strong>{formatTokens(sourceUsage.est_total)}</strong><span>est · 全量</span>
            </span>
          {/if}
        </div>
      {/if}
    </header>

    {#if errorMessage}
      <div class="banner error">{errorMessage}</div>
    {/if}

    {#if reportNotice}
      <div class="banner" class:error={reportFailed} class:notice={!reportFailed}>
        {reportNotice}
      </div>
    {/if}

    {#if scanNotices.length > 0}
      <details class="banner notice">
        <summary>{scanNotices.length} 条数据源提示</summary>
        <ul>
          {#each scanNotices as notice}
            <li><strong>{notice.label}</strong><span>{notice.message}</span></li>
          {/each}
        </ul>
      </details>
    {/if}

    {#if reportView}
      <div class="report-pane">
        {#if reportUrl}
          {#key reportGeneration}
            <iframe class="report-frame" src={reportUrl} title="usage-ledger 用量报告"></iframe>
          {/key}
        {:else}
          <div class="empty-state">
            <p>{reportOpen ? "正在生成用量报告…" : "报告尚未生成。"}</p>
          </div>
        {/if}
      </div>
    {:else if busy && !payload}
      <div class="empty-state">正在读取本地任务…</div>
    {:else if !hasVisibleTasks}
      <div class="empty-state">
        <p>当前筛选下没有任务。</p>
        <p class="subtle">试试切换视图或清除筛选，或点击右上角刷新。</p>
      </div>
    {:else}
      <div class="groups">
        <div class="flat-list">
          {#each renderedTasks as task (task.key)}
            {@render taskRow(task, !workspaceFilter)}
          {/each}
        </div>
        {#if hasMoreTasks}
          <button class="list-more" onclick={showMoreTasks}>
            显示更多（还有 {flatTasks.length - renderedTasks.length} 条 · 当前每页 {TASK_PAGE_SIZE} 条）
          </button>
        {/if}
      </div>
    {/if}
  </main>
  {/if}

  {#if transcriptOpen && transcriptTask}
  <section class="transcript-pane" aria-label="会话对话记录">
    <header class="transcript-head">
      <div class="transcript-ident">
        <span class="source-icon">{@html sourceIconMarkup(transcriptTask.source)}</span>
        <span class="transcript-source-label">{transcriptTask.source_label}</span>
        <span class="status {statusClass(transcriptTask)}">{statusLabel(transcriptTask)}</span>
        <h2 class="transcript-title">{transcriptTask.title}</h2>
      </div>
      <div class="transcript-actions">
        {#if continueError}<span class="continue-note error">{continueError}</span>{/if}
        <button
          class="transcript-continue"
          style={`--source:${sourceColor(transcriptTask.source)};--on-source:${contrastText(sourceColor(transcriptTask.source))}`}
          onclick={() => void continueTask()}
          disabled={continuing}
        >
          继续工作
        </button>
        <button
          class="transcript-reread"
          onclick={rereadTranscript}
          disabled={transcriptPhase === "loading"}
        >
          重新读取
        </button>
        <button class="transcript-return" onclick={closeTranscript}>返回</button>
      </div>
    </header>

    {#if transcriptSessionTruncated}
      <p class="transcript-session-truncated">会话过长，仅保留最近部分。</p>
    {/if}

    <div class="transcript-messages" bind:this={transcriptMessagesEl}>
      {#if transcriptPhase === "loading"}
        <div class="transcript-state loading">正在读取对话…</div>
      {:else if transcriptReason}
        <div class="transcript-state error">{transcriptReason}</div>
      {:else if transcriptVisible.length === 0}
        <div class="transcript-state error">会话没有可显示的对话内容</div>
      {:else}
        {#each transcriptVisible as message, index (index)}
          <div class="transcript-msg" data-role={message.role}>
            <span class="transcript-text">{message.text}</span>
            {#if message.truncated}<span class="transcript-truncated">已截断</span>{/if}
          </div>
        {/each}
      {/if}
    </div>

    {#if transcriptEarlierCount > 0}
      <button class="transcript-earlier" onclick={showEarlierMessages}>
        加载更早（还有 {transcriptEarlierCount} 条）
      </button>
    {/if}
  </section>
  {/if}

  {#if !reportView && !transcriptOpen}
  <aside class="inspector">
    {#if selected}
      <div class="inspector-content">
        <header class="inspector-head">
          <div class="source-line">
            <span class="source-dot" style={`--source:${sourceColor(selected.source)}`}></span>
            <span>{selected.source_label}</span>
            <span class="status {statusClass(selected)}">{statusLabel(selected)}</span>
          </div>
          <h2>{selected.title}</h2>
          <div class="tag-line">
            {#if selected.source_archived}<span class="badge">源归档</span>{/if}
            {#if selected.source_unread}<span class="badge">未读</span>{/if}
            {#if selected.parent_key}<span class="badge">子会话</span>{/if}
            {#if selected.is_automation}<span class="badge">自动化</span>{/if}
            {#if selected.source_missing}<span class="badge danger">源已删除</span>{/if}
          </div>
        </header>

        <div class="continue-row">
          <button
            class="continue-button"
            style={`--source:${sourceColor(selected.source)};--on-source:${contrastText(sourceColor(selected.source))}`}
            onclick={() => void continueTask()}
            disabled={continuing}
          >
            {continueButtonLabel}
          </button>
          {#if continueError}<span class="continue-note error">{continueError}</span>{/if}
        </div>

        <section class="detail-block">
          <h3>任务信息</h3>
          <dl class="meta-grid">
            <dt>Workspace</dt><dd>{selected.workspace_name}</dd>
            <dt>项目路径</dt><dd class="mono">{selected.project_path ?? "—"}</dd>
            <dt>会话 ID</dt><dd class="mono">{selected.source_id}</dd>
            <dt>最近活动</dt><dd>{fullTime(selected.updated_at)}</dd>
            <dt>创建时间</dt><dd>{fullTime(selected.created_at)}</dd>
            {#if selected.model}<dt>模型</dt><dd>{selected.model}</dd>{/if}
            {#if selected.git_branch}<dt>分支</dt><dd class="mono">{selected.git_branch}</dd>{/if}
            {#if selected.message_count !== null}<dt>消息数</dt><dd>{selected.message_count}</dd>{/if}
            {#if selected.status_raw}<dt>源状态</dt><dd class="mono">{selected.status_raw}</dd>{/if}
          </dl>
        </section>

        {#if payload?.usage}
          <section class="detail-block usage-block">
            <h3>用量 <span class="subtle">usage-ledger · 只读</span></h3>
            {#if selectedUsage}
              {#if selectedToken}
                <div class="usage-track">
                  <div class="usage-track-head">
                    <span class="usage-track-label">token 真值</span>
                    <span class="usage-track-total">{formatTokens(selectedToken.total)}</span>
                  </div>
                  <div class="usage-grid">
                    <span>输入</span><strong>{formatTokens(selectedToken.input)}</strong>
                    <span>输出</span><strong>{formatTokens(selectedToken.output)}</strong>
                    {#if selectedToken.cache_read > 0}
                      <span>缓存读</span><strong>{formatTokens(selectedToken.cache_read)}</strong>
                    {/if}
                    {#if selectedToken.cache_write > 0}
                      <span>缓存写</span><strong>{formatTokens(selectedToken.cache_write)}</strong>
                    {/if}
                    {#if selectedToken.reasoning > 0}
                      <span>推理</span><strong>{formatTokens(selectedToken.reasoning)}</strong>
                    {/if}
                  </div>
                  <p class="usage-meta">
                    {joinModels(selectedToken.models)} · {selectedToken.turns} 轮
                    · 最近 {selectedToken.last_day ?? "—"}
                  </p>
                </div>
              {/if}
              {#if selectedCredits}
                <div class="usage-track">
                  <div class="usage-track-head">
                    <span class="usage-track-label credits">credits · 服务端计费</span>
                    <span class="usage-track-total">{formatCredits(selectedCredits.billable)}</span>
                  </div>
                  <p class="usage-meta">
                    {selectedCredits.billable_requests} 个计费请求
                    {#if selectedCredits.free > 0}
                      · 免费 {formatCredits(selectedCredits.free)} credits / {selectedCredits.free_requests} 请求
                    {/if}
                    · 最近 {selectedCredits.last_day ?? "—"}
                  </p>
                </div>
              {/if}
              {#if selectedEst}
                <div class="usage-track">
                  <div class="usage-track-head">
                    <span class="usage-track-label est">est 估算</span>
                    <span class="usage-track-total">{formatTokens(selectedEst.total)}</span>
                  </div>
                  <div class="usage-grid">
                    <span>输入</span><strong>{formatTokens(selectedEst.input)}</strong>
                    <span>输出</span><strong>{formatTokens(selectedEst.output)}</strong>
                  </div>
                  <p class="usage-meta">
                    {selectedEst.source} · {selectedEst.requests} 请求
                    · 最近 {selectedEst.last_day ?? "—"}
                  </p>
                </div>
              {/if}
            {:else}
              <p class="usage-empty">usage-ledger 中没有该会话的用量记录。</p>
            {/if}
            <p class="usage-note">四轨口径不同：token / credits / est 永不相加。</p>
          </section>
        {/if}

        {#if selected.todos.length > 0}
          <section class="detail-block">
            <h3>待办 <span class="subtle">{selected.todo_done}/{selected.todo_total}</span></h3>
            <ul class="todo-list">
              {#each selected.todos as todo (todo.item_id)}
                <li class="todo {todo.status}" title={todoLabel(todo.status)}>
                  <span class="todo-mark"></span>
                  <span>{todo.subject}</span>
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if selected.resume_cmd}
          <section class="detail-block">
            <h3>恢复命令 <span class="subtle">只读展示，Perch 不会执行</span></h3>
            <code class="command">{selected.resume_cmd}</code>
          </section>
        {/if}

        <footer class="inspector-foot">Perch 只读取源元数据，不会修改任何 agent 数据。</footer>
      </div>
    {:else}
      <div class="inspector-empty">
        <span class="inspector-empty-icon">
          {@render appIcon("empty")}
        </span>
        <p>选择一个任务查看详情</p>
        <p class="subtle">详情只读；置顶将在下一阶段加入。</p>
      </div>
    {/if}
  </aside>
  {/if}
</div>

{#if searchOpen}
  <div class="search-layer">
    <button class="search-backdrop" aria-label="关闭搜索" tabindex="-1" onclick={closeSearch}></button>
    <div class="search-panel" role="dialog" aria-modal="true" aria-label="搜索任务">
      <div class="search-field">
        <svg
          class="search-icon"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d={ICONS.search} />
        </svg>
        <input
          bind:this={searchInput}
          bind:value={query}
          class="search-input"
          type="text"
          placeholder="搜索任务、项目、待办…"
          spellcheck="false"
          autocomplete="off"
          aria-label="搜索任务"
        />
        {#if query}
          <button
            class="search-clear"
            onclick={() => (query = "")}
            title="清除搜索"
            aria-label="清除搜索"
          >
            ×
          </button>
        {:else}
          <kbd>Esc</kbd>
        {/if}
      </div>

      <div class="search-body" bind:this={searchResultList}>
        {#if searchError}
          <div class="search-state danger">
            <p>搜索失败：{searchError}</p>
            <p class="subtle">请重试，或重新扫描数据源。</p>
          </div>
        {:else if !searchActive}
          <div class="search-state">
            <p title={searchScopeTitle()}>{searchScopeLabel}</p>
            {#if searchScope.length > 0}
              <button class="search-scope-clear" onclick={clearSearchScope}>清除作用域</button>
            {/if}
            <p class="subtle">
              支持标题、项目、路径、待办、模型、分支、状态、来源、类型、会话 ID；支持拼音与模糊匹配。
            </p>
          </div>
        {:else if searchResults.length === 0}
          <div class="search-state">
            {#if searching}
              <p>搜索中…</p>
            {:else}
              <p>没有匹配“{query.trim()}”的任务。</p>
            {/if}
          </div>
        {:else}
          {#each renderedSearchResults as result, index (result.task.key)}
            <button
              class="search-result"
              data-index={index}
              class:active={index === searchIndex}
              onmouseenter={() => (searchIndex = index)}
              onclick={() => openSearchResult(result.task)}
            >
              <span
                class="source-dot"
                style={`--source:${sourceColor(result.task.source)}`}
              ></span>
              <span class="result-main">
                <span class="result-title">{result.task.title}</span>
                <span class="result-meta">
                  <span>{result.task.source_label}</span>
                  <span class="dot-sep">·</span>
                  <span class="result-workspace">{result.task.workspace_name}</span>
                  <span class="dot-sep">·</span>
                  <span>{relativeTime(result.task.updated_at)}</span>
                  {#if result.task.source_archived}<span class="mini-badge">源归档</span>{/if}
                  {#if result.task.is_child}<span class="mini-badge">子会话</span>{/if}
                  {#if result.task.is_automation}<span class="mini-badge">自动化</span>{/if}
                </span>
                {#if result.hit.fields.length > 0}
                  <span class="result-fields">匹配 {result.hit.fields.join(" · ")}</span>
                {/if}
              </span>
              <span class="status {statusClass(result.task)}">{statusLabel(result.task)}</span>
            </button>
          {/each}
          {#if hasMoreSearchResults}
            <button class="search-more" onclick={showMoreSearchResults}>
              显示更多（还有 {searchResults.length - renderedSearchResults.length} 条）
            </button>
          {/if}
        {/if}
      </div>

      <footer class="search-foot">
        <span class="search-hints">
          <kbd>↑</kbd><kbd>↓</kbd> 选择 <kbd>↵</kbd> 打开 <kbd>Esc</kbd> 关闭
        </span>
        {#if searchScope.length > 0}
          <span class="search-scope" title={searchScopeTitle()}>
            {searchScopeLabel}
            <button onclick={clearSearchScope} aria-label="清除搜索作用域">清除</button>
          </span>
        {/if}
        {#if searchActive && !searchError}
          <span class="search-count">
            {#if searching}
              搜索中…
            {:else if searchTotal > 0}
              {searchTotal} 个结果{#if hasMoreSearchResults} · 已展示 {renderedSearchResults.length} 条{/if}
            {:else}
              无结果
            {/if}
          </span>
        {/if}
      </footer>
    </div>
  </div>
{/if}
