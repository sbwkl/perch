// 提供对话正文的 9 个来源；与 perch_core::models::SUPPORTED_TRANSCRIPT_SOURCES
// 保持同步（models.rs 是事实源，改动时两侧同步更新）。
// Trae CN 只有摘要、CodeBuddy 只有日志行，不提供正文：任务行不渲染对话图标。
const TRANSCRIPT_SOURCES: ReadonlySet<string> = new Set([
  "qoder",
  "claude",
  "codex",
  "opencode",
  "kilo",
  "cline",
  "antigravity",
  "minimax",
  "codearts",
]);

export function transcriptSupported(source: string): boolean {
  return TRANSCRIPT_SOURCES.has(source);
}
