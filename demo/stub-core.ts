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
