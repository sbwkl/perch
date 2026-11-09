import type { SyncReport } from "../src/lib/types";
import { demoPayload } from "./fixture";

export async function invoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  await new Promise((resolve) => setTimeout(resolve, 80));

  switch (command) {
    case "bootstrap":
      return demoPayload() as T;
    case "open_usage_report":
      return { path: "/tmp/demo-usage-report.html" } as T;
    case "refresh_sources": {
      const started = Date.now() / 1000;
      const payload = demoPayload();
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
      const payload = demoPayload();
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
        hits: matches
          .slice(0, 10)
          .map(({ task_key, score, fields }) => ({ task_key, score, fields })),
      } as T;
    }
    case "continue_task":
      return {
        channel: "app",
        target: String(args?.taskKey ?? ""),
        message: "已打开 Codex（演示数据，不会启动任何应用）",
      } as T;
    default:
      throw new Error(`demo 未实现命令：${command}`);
  }
}
