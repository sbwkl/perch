<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import type {
    BootstrapPayload,
    ContinueOutcome,
    SearchHit,
    SearchResponse,
    SyncReport,
    TaskView,
    UsageReportOutcome,
  } from "./lib/types";
  import { sourceIconMarkup } from "./lib/source-icons";

  type ViewMode = "recent" | "older" | "archived";

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
    bird: "M7 20c0-5.5 4.5-10 10-10h4l-3-4-5 2-4 4-3 6 1 2zM9 20h10",
    pin: "M12 17v5M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z",
    report: "M18 20V10M12 20V4M6 20v-6",
    search: "M21 21l-4.35-4.35M11 19a8 8 0 1 1 0-16 8 8 0 0 1 0 16z",
  } as const;

  const SOURCE_COLORS: Record<string, string> = {
    qoder: "#2adb5c",
    claude: "#d97757",
    codex: "#ffffff",
    opencode: "#201d1d",
    kilo: "#ffdd00",
    "trae-cn": "#32f08c",
  };

  const SOURCE_LABELS: Record<string, string> = {
    qoder: "Qoder",
    claude: "Claude Code",
    codex: "Codex",
    opencode: "OpenCode",
    kilo: "Kilo",
    "trae-cn": "Trae CN",
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
    idle: "空闲",
  };

  let payload = $state<BootstrapPayload | null>(null);
  let busy = $state(true);
  let refreshing = $state(false);
  let errorMessage = $state("");
  let scanNotices = $state<ScanNotice[]>([]);
  let lastScanSummary = $state("");
  let view = $state<ViewMode>("recent");
  let workspaceFilter = $state<string | null>(null);
  let sourceFilter = $state<string | null>(null);
  let selectedKey = $state<string | null>(null);
  let continuing = $state(false);
  let continueNotice = $state("");
  let showAllWorkspaces = $state(false);
  let reportOpen = $state(false);
  let reportNotice = $state("");
  let reportFailed = $state(false);
  let query = $state("");
  let searchOpen = $state(false);
  let searchIndex = $state(0);
  let searchHits = $state<SearchHit[]>([]);
  let searchTotal = $state(0);
  let searching = $state(false);
  let searchError = $state("");
  let searchInput = $state<HTMLInputElement>();
  let searchResultList = $state<HTMLDivElement>();

  const WORKSPACE_VISIBLE_LIMIT = 8;

  const smartLists = $derived([
    { id: "recent" as ViewMode, label: "最近", count: payload?.counts.recent ?? 0, icon: ICONS.clock },
    { id: "older" as ViewMode, label: "更早", count: payload?.counts.older ?? 0, icon: ICONS.history },
    { id: "archived" as ViewMode, label: "已归档", count: payload?.counts.archived ?? 0, icon: ICONS.archive },
  ]);

  const workspaceItems = $derived.by(() =>
    (payload?.workspaces ?? [])
      .map((group) => ({
        key: group.key,
        name: group.name,
        path: group.project_path,
        count: group.tasks.filter(
          (task) => !task.is_child && !task.is_automation && !task.source_archived,
        ).length,
      }))
      .filter((item) => item.count > 0),
  );

  const visibleWorkspaces = $derived.by(() => {
    if (showAllWorkspaces || workspaceItems.length <= WORKSPACE_VISIBLE_LIMIT) {
      return workspaceItems;
    }
    const visible = workspaceItems.slice(0, WORKSPACE_VISIBLE_LIMIT);
    if (workspaceFilter && !visible.some((item) => item.key === workspaceFilter)) {
      const active = workspaceItems.find((item) => item.key === workspaceFilter);
      if (active) visible.push(active);
    }
    return visible;
  });

  const hiddenWorkspaceCount = $derived(
    Math.max(0, workspaceItems.length - visibleWorkspaces.length),
  );

  const canToggleWorkspaces = $derived(workspaceItems.length > WORKSPACE_VISIBLE_LIMIT);

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
        if (task.is_child || task.is_automation) continue;
        if (view === "archived") {
          if (!task.source_archived) continue;
        } else if (task.source_archived || task.is_recent !== (view === "recent")) {
          continue;
        }
        if (sourceFilter && task.source !== sourceFilter) continue;
        tasks.push(task);
      }
    }
    return tasks.sort(compareTaskOrder);
  });

  const totalVisible = $derived(flatTasks.length);

  const hasVisibleTasks = $derived(flatTasks.length > 0);

  const allTasks = $derived.by(() => {
    const map = new Map<string, TaskView>();
    for (const group of payload?.workspaces ?? []) {
      for (const task of group.tasks) map.set(task.key, task);
    }
    return map;
  });

  const searchActive = $derived(query.trim().length > 0);

  const searchResults = $derived.by(() => {
    const results: Array<{ hit: SearchHit; task: TaskView }> = [];
    for (const hit of searchHits) {
      const task = allTasks.get(hit.task_key);
      if (task) results.push({ hit, task });
    }
    return results;
  });

  let searchSeq = 0;
  $effect(() => {
    const term = query.trim();
    const source = sourceFilter;
    const workspace = workspaceFilter;
    void payload?.generated_at;
    const seq = ++searchSeq;
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

  const selected = $derived.by<TaskView | null>(() => {
    if (!payload || !selectedKey) return null;
    for (const group of payload.workspaces) {
      const found = group.tasks.find((task) => task.key === selectedKey);
      if (found) return found;
    }
    return null;
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
    if (workspaceFilter) {
      return workspaceItems.find((item) => item.key === workspaceFilter)?.name ?? "Workspace";
    }
    if (sourceFilter) return SOURCE_LABELS[sourceFilter] ?? sourceFilter;
    return smartLists.find((item) => item.id === view)?.label ?? "任务";
  });

  const subheading = $derived.by(() => {
    const generated = payload ? fullTime(payload.generated_at) : "—";
    const summary = lastScanSummary ? ` · ${lastScanSummary}` : "";
    return `${totalVisible} 个任务 · 快照 ${generated}${summary}`;
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
      lastScanSummary = `扫描 ${report.reports.length} 个来源 · ${Math.max(
        0,
        Math.round((report.finished_at - report.started_at) * 1000),
      )} ms`;
      await loadBootstrap();
      if (selectedKey && !payloadContains(selectedKey)) selectedKey = null;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      refreshing = false;
      busy = false;
    }
  }

  function payloadContains(key: string): boolean {
    return Boolean(
      payload?.workspaces.some((group) => group.tasks.some((task) => task.key === key)),
    );
  }

  function selectTask(key: string) {
    selectedKey = key;
    continueNotice = "";
  }

  async function continueTask() {
    if (!selected || continuing) return;
    const taskKey = selected.key;
    continuing = true;
    continueNotice = "";
    try {
      const outcome = await invoke<ContinueOutcome>("continue_task", { taskKey });
      continueNotice = outcome.message;
    } catch (error) {
      continueNotice = error instanceof Error ? error.message : String(error);
    } finally {
      continuing = false;
    }
  }

  async function openUsageReport() {
    if (reportOpen) return;
    reportOpen = true;
    reportNotice = "";
    reportFailed = false;
    try {
      await invoke<UsageReportOutcome>("open_usage_report");
      reportNotice = "已生成用量报告并在浏览器打开";
    } catch (error) {
      reportFailed = true;
      reportNotice = error instanceof Error ? error.message : String(error);
    } finally {
      reportOpen = false;
    }
  }

  function selectView(next: ViewMode) {
    view = next;
    workspaceFilter = null;
    sourceFilter = null;
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
    selectTask(task.key);
    closeSearch();
  }

  function moveSearchSelection(delta: number) {
    if (searchResults.length === 0) return;
    searchIndex =
      (searchIndex + delta + searchResults.length) % searchResults.length;
    void tick().then(() => {
      const item = searchResultList?.children[searchIndex] as HTMLElement | undefined;
      item?.scrollIntoView({ block: "nearest" });
    });
  }

  function openActiveSearchResult() {
    const result = searchResults[searchIndex];
    if (result) openSearchResult(result.task);
  }

  function selectWorkspace(key: string) {
    workspaceFilter = workspaceFilter === key ? null : key;
    sourceFilter = null;
  }

  function selectSource(source: string) {
    sourceFilter = sourceFilter === source ? null : source;
    workspaceFilter = null;
  }

  function sourceColor(source: string): string {
    return SOURCE_COLORS[source] ?? "#8e8e93";
  }

  function statusLabel(task: TaskView): string {
    return STATUS_LABELS[task.status] ?? task.status;
  }

  function statusClass(task: TaskView): string {
    if (task.status === "running" || task.status === "active") return "live";
    if (task.status === "blocked") return "danger";
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

  function usageSnapshotLabel(seconds: number | null): string {
    if (!seconds) return "用量快照时间未知";
    return `用量快照 ${relativeTime(seconds)}`;
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
      void refresh();
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
    if (event.key === "Escape") selectedKey = null;
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
      if (!disposed) void loadBootstrap();
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

{#snippet taskRow(task: TaskView, showWorkspace: boolean)}
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
        {#if task.source_missing}<span class="mini-badge danger">源已删除</span>{/if}
      </span>
    </span>
    <span class="status {statusClass(task)}">{statusLabel(task)}</span>
  </button>
{/snippet}

<div class="app">
  <aside class="sidebar">
    <header class="brand">
      <span class="brand-mark">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
          <path d={ICONS.bird} />
        </svg>
      </span>
      <span class="brand-text">
        <strong>Perch</strong>
        <small>多 agent 任务总览</small>
      </span>
    </header>

    <nav class="nav-section">
      <p class="section-title">任务</p>
      {#each smartLists as item (item.id)}
        <button
          class="nav-item"
          class:active={view === item.id}
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

    <nav class="nav-section scroll-section">
      <p class="section-title">Workspaces</p>
      {#each visibleWorkspaces as workspace (workspace.key)}
        <button
          class="nav-item"
          class:active={workspaceFilter === workspace.key}
          title={workspace.path ?? workspace.name}
          onclick={() => selectWorkspace(workspace.key)}
        >
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
            <path d={ICONS.folder} />
          </svg>
          <span class="nav-label">{workspace.name}</span>
          <span class="nav-count">{workspace.count}</span>
        </button>
      {/each}
      {#if canToggleWorkspaces}
        <button class="nav-more" onclick={() => (showAllWorkspaces = !showAllWorkspaces)}>
          {showAllWorkspaces ? "收起" : `显示更多（${hiddenWorkspaceCount}）`}
        </button>
      {/if}
      {#if workspaceItems.length === 0}
        <p class="nav-empty">暂无 workspace</p>
      {/if}
    </nav>

    <nav class="nav-section sources">
      <p class="section-title">来源</p>
      {#each payload?.sources ?? [] as source (source.source)}
        <button
          class="nav-item source-item"
          class:active={sourceFilter === source.source}
          title={source.error ?? source.label}
          onclick={() => selectSource(source.source)}
        >
          <span class="source-icon">{@html sourceIconMarkup(source.source)}</span>
          <span class="nav-label">{source.label}</span>
          <span class="nav-count">{source.task_count}</span>
          {#if source.error && source.status !== "unavailable"}
            <span class="source-warning">!</span>
          {/if}
        </button>
      {/each}
      <p class="nav-note">Trae CN 需安装 usage-ledger hook 后显示会话；ChatGPT 桌面仅 Codex 会话可见。</p>
    </nav>

    <footer class="sidebar-foot">
      <span>只读采集 · 不改动源数据</span>
      {#if payload?.usage}
        <button
          class="sidebar-report"
          onclick={() => void openUsageReport()}
          disabled={reportOpen}
          title={reportOpen ? "正在生成用量报告" : "打开 usage-ledger 用量报告"}
          aria-label={reportOpen ? "正在生成用量报告" : "打开 usage-ledger 用量报告"}
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
      {/if}
    </footer>
  </aside>

  <main class="list-pane">
    <header class="list-head">
      <div class="head-text">
        <h2>{heading}</h2>
        <p>{subheading}</p>
      </div>
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
          <span class="usage-pill muted" class:stale={payload?.usage?.stale}>
            {payload?.usage?.stale
              ? "用量可能已过期"
              : usageSnapshotLabel(payload?.usage?.as_of ?? null)}
          </span>
          {#if sourceUsage.matched_tasks > 0}
            <span class="usage-pill muted">匹配 {sourceUsage.matched_tasks} 个会话</span>
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

    {#if busy && !payload}
      <div class="empty-state">正在读取本地任务…</div>
    {:else if !hasVisibleTasks}
      <div class="empty-state">
        <p>当前筛选下没有任务。</p>
        <p class="subtle">试试切换视图或清除筛选，或点击右上角刷新。</p>
      </div>
    {:else}
      <div class="groups">
        <div class="flat-list">
          {#each flatTasks as task (task.key)}
            {@render taskRow(task, !workspaceFilter)}
          {/each}
        </div>
      </div>
    {/if}
  </main>

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
            onclick={() => void continueTask()}
            disabled={continuing}
          >
            {continuing ? "打开中…" : `打开 ${selected.source_label}`}
          </button>
          {#if continueNotice}<span class="continue-note">{continueNotice}</span>{/if}
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
            <h3>恢复命令 <span class="subtle">只读展示</span></h3>
            <code class="command">{selected.resume_cmd}</code>
          </section>
        {/if}

        <footer class="inspector-foot">Perch 只读取源元数据，不会修改任何 agent 数据。</footer>
      </div>
    {:else}
      <div class="inspector-empty">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
          <path d={ICONS.bird} />
        </svg>
        <p>选择一个任务查看详情</p>
        <p class="subtle">详情只读；置顶将在下一阶段加入。</p>
      </div>
    {/if}
  </aside>
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
            <p>搜索所有会话（含归档）</p>
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
          {#each searchResults as result, index (result.task.key)}
            <button
              class="search-result"
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
        {/if}
      </div>

      <footer class="search-foot">
        <span class="search-hints">
          <kbd>↑</kbd><kbd>↓</kbd> 选择 <kbd>↵</kbd> 打开 <kbd>Esc</kbd> 关闭
        </span>
        {#if searchActive && !searchError}
          <span class="search-count">
            {#if searching}
              搜索中…
            {:else if searchTotal > 0}
              {searchTotal} 个结果{#if searchTotal > searchResults.length} · 仅显示前 {searchResults.length} 条{/if}
            {:else}
              无结果
            {/if}
          </span>
        {/if}
      </footer>
    </div>
  </div>
{/if}
