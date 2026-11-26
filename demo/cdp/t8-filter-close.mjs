// T8 补充：评审修复 B —— 「当前筛选列表」口径的双侧用例。
// 入口 npm run cdp（自动起 demo）；单独运行先 npm run demo（端口 1421）。
import { launchChrome, Page, Suite, assert, assertEqual } from "./harness.mjs";

const URL = "http://localhost:1421/";
const T = "?transcript=1";
const FILTER_OUT = "合成对话 · 超大会话截断"; // 被筛出「最近」视图
const STAY_OPEN = "合成对话 · 工具轮与截断"; // 仍在可见列表

export const suite = new Suite("T8 当前筛选列表口径");

function rowExpr(title) {
  return `[...document.querySelectorAll('.flat-list .task-row')].find(r =>
    r.querySelector('.task-title')?.textContent?.includes(${JSON.stringify(title)})) ?? null`;
}

export async function run(page) {
  const s = suite;

  s.add("T8 任务被筛出当前筛选列表但仍在快照 → 面板关闭、选中清除", async () => {
    await page.goto(`${URL}${T}&filterout=${encodeURIComponent(FILTER_OUT)}`);
    await page.evaluate(`(() => {
      const row = ${rowExpr(FILTER_OUT)};
      row.querySelector('.task-transcript').click();
    })()`);
    await page.waitFor("Boolean(document.querySelector('.transcript-pane'))");
    // 面板打开时刷新按钮不在 DOM（列表让位），用 ⌘R 触发刷新
    await page.evaluate(
      `window.dispatchEvent(new KeyboardEvent('keydown', { key: 'r', metaKey: true }))`,
    );
    await page.waitFor("!document.querySelector('.transcript-pane')");
    await page.waitFor("!document.querySelector('.refresh-icon.spinning')");
    const state = await page.evaluate(`(() => {
      const row = ${rowExpr(FILTER_OUT)};
      return {
        listBack: Boolean(document.querySelector('.flat-list')),
        anySelected: Boolean(document.querySelector('.flat-list .task.selected')),
        rowStillListed: Boolean(row),
      };
    })()`);
    assertEqual(state.listBack, true, "列表恢复");
    assertEqual(state.anySelected, false, "选中已清除（与列表选中规则一致）");
    assertEqual(
      state.rowStillListed,
      false,
      "该任务已不在「最近」视图（仍在快照，属筛选移除而非消失）",
    );
  });

  s.add("T8 任务仍在当前筛选列表 → 刷新后面板保持、内容不丢", async () => {
    await page.goto(`${URL}${T}&filterout=${encodeURIComponent(FILTER_OUT)}`);
    await page.evaluate(`(() => {
      const row = ${rowExpr(STAY_OPEN)};
      row.querySelector('.task-transcript').click();
    })()`);
    await page.waitFor("Boolean(document.querySelector('.transcript-pane'))");
    await page.waitFor(
      "document.querySelectorAll('.transcript-pane .transcript-msg').length > 0",
    );
    await page.evaluate(
      `window.dispatchEvent(new KeyboardEvent('keydown', { key: 'r', metaKey: true }))`,
    );
    await page.waitFor("!document.querySelector('.refresh-icon.spinning')");
    const state = await page.evaluate(`(() => {
      const pane = document.querySelector('.transcript-pane');
      return {
        paneOpen: Boolean(pane),
        title: pane?.querySelector('.transcript-title')?.textContent?.trim() ?? null,
        messages: pane ? pane.querySelectorAll('.transcript-msg').length : -1,
      };
    })()`);
    assertEqual(state.paneOpen, true, "刷新后对话面板保持打开");
    assertEqual(state.title, STAY_OPEN, "面板标题不变");
    assert(state.messages > 0, `消息仍在（实际 ${state.messages}）`);
  });

  const { pass, failures } = await suite.run(process.argv[2]);
  console.log(`\nT8 结果：${pass} 通过，${failures.length} 失败`);
  process.exitCode = failures.length === 0 ? 0 : 1;
}

export async function main() {
  const chrome = await launchChrome();
  const page = await Page.connect(chrome.port);
  try {
    await run(page);
  } finally {
    chrome.proc.kill();
  }
}

await main();
