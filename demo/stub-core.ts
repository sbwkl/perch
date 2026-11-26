import type { SyncReport } from "../src/lib/types";
import { transcriptSupported } from "../src/lib/transcript";
import { demoPayload, demoTranscriptOutcome } from "./fixture";

export function convertFileSrc(filePath: string): string {
  return filePath;
}

// 供 CDP 验收读取的演示状态：重读次数、首读失败标记与刷新计数。
const demoState = {
  refreshCount: 0,
  failedReads: new Set<string>(),
  readSessionCalls: [] as string[],
};

if (typeof window !== "undefined") {
  (window as unknown as { __perchDemo?: typeof demoState }).__perchDemo = demoState;
}

// ?vanish=<标题>：第二次刷新（首次是挂载时的自动刷新）后让该任务从快照消失，
// 用于驱动「刷新后任务不在当前列表 → 关闭对话并清除选中」（T8）。
const VANISH_PARAM = new URLSearchParams(location.search).get("vanish") ?? "";

function currentVanishTitle(): string | null {
  return demoState.refreshCount >= 2 && VANISH_PARAM ? VANISH_PARAM : null;
}

function findDemoTask(taskKey: string) {
  const payload = demoPayload({ excludeTitle: currentVanishTitle() });
  return (
    payload.workspaces
      .flatMap((group) => group.tasks)
      .find((item) => item.key === taskKey) ?? null
  );
}

export async function invoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  await new Promise((resolve) => setTimeout(resolve, 80));

  switch (command) {
    case "bootstrap":
      return demoPayload({ excludeTitle: currentVanishTitle() }) as T;
    case "open_usage_report":
      return { path: "/demo-usage-report.html" } as T;
    case "refresh_sources": {
      demoState.refreshCount += 1;
      const started = Date.now() / 1000;
      const payload = demoPayload({ excludeTitle: currentVanishTitle() });
      const report: SyncReport = {
        started_at: started,
        finished_at: started + 0.012,
        reports: payload.sources.map((source) => ({
          source: source.source,
          status: "ok",
          mode: "demo",
          task_count: source.task_count,
          todo_count: 0,
          missing_count: 0,
          duration_ms: 12,
          error: null,
        })),
      };
      return report as T;
    }
    case "search_tasks": {
      const query = String(args?.query ?? "").trim().toLocaleLowerCase();
      if (!query) return { hits: [], total: 0 } as T;
      const source = args?.source ? String(args.source) : null;
      const workspace = args?.workspace ? String(args.workspace) : null;
      const terms = query.split(/\s+/).filter(Boolean);
      const payload = demoPayload({ excludeTitle: currentVanishTitle() });
      const matches: Array<{
        task_key: string;
        score: number;
        fields: string[];
        updated_at: number;
      }> = [];
      for (const group of payload.workspaces) {
        for (const task of group.tasks) {
          if (source && task.source !== source) continue;
          if (workspace && task.workspace_key !== workspace) continue;
          const fields: Array<[string, string]> = [
            ["标题", task.title],
            ["项目", task.workspace_name],
            ["项目路径", task.project_path ?? ""],
            ["待办", task.todos.map((todo) => todo.subject).join(" ")],
            ["模型", task.model ?? ""],
            ["分支", task.git_branch ?? ""],
            ["来源", task.source_label],
            ["会话 ID", task.source_id],
          ];
          let matched: string[] | null = [];
          for (const term of terms) {
            const fieldHits = fields
              .filter(([, value]) => value.toLocaleLowerCase().includes(term))
              .map(([label]) => label);
            if (fieldHits.length === 0) {
              matched = null;
              break;
            }
            matched.push(...fieldHits);
          }
          if (!matched) continue;
          matches.push({
            task_key: task.key,
            score: matched.length,
            fields: [...new Set(matched)],
            updated_at: task.updated_at ?? 0,
          });
        }
      }
      matches.sort(
        (left, right) => right.score - left.score || right.updated_at - left.updated_at,
      );
      return {
        total: matches.length,
        hits: matches.map(({ task_key, score, fields }) => ({
          task_key,
          score,
          fields,
        })),
      } as T;
    }
    case "continue_task": {
      const taskKey = String(args?.taskKey ?? "");
      const task = findDemoTask(taskKey);
      if (!task) throw new Error(`任务不存在：${taskKey}`);
      // Success is silent in the real command, so the demo stays silent too.
      return undefined as T;
    }
    case "read_session_content": {
      // 与 src-tauri read_session_content_impl 同构：key 查找 → 来源能力 → 读取结果。
      const taskKey = String(args?.taskKey ?? "");
      demoState.readSessionCalls.push(taskKey);
      const task = findDemoTask(taskKey);
      if (!task) throw new Error(`任务不存在：${taskKey}`);
      if (!transcriptSupported(task.source)) {
        throw new Error(`来源不支持读取对话正文：${task.source}`);
      }
      const firstAttempt = !demoState.failedReads.has(taskKey);
      demoState.failedReads.add(taskKey);
      return demoTranscriptOutcome(task.title, { firstAttempt }) as T;
    }
    default:
      throw new Error(`demo 未实现命令：${command}`);
  }
}
