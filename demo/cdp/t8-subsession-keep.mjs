// T8 补充：评审修复 —— 子会话 / 自动化任务经搜索进入对话后，
// sources-updated 实时刷新不得关闭面板（兜底口径纳入）；被筛出仍关闭。
// 入口 npm run cdp（自动起 demo）；单独运行先 npm run demo（端口 1421）。
import { launchChrome, Page, Suite, assert, assertEqual } from "./harness.mjs";

const URL = "http://localhost:1421/";
const T = "?transcript=1";
const CHILD = "合成对话 · 子会话（搜索可见）";
const AUTOMATION = "合成对话 · 自动化任务（搜索可见）";
const CHILD_TERM = "子会话";
const AUTOMATION_TERM = "自动化任务";

export const suite = new Suite("T8 子会话/自动化兜底口径");

function rowExpr(title) {
  return `[...document.querySelectorAll('.flat-list .task-row')].find(r =>
    r.querySelector('.task-title')?.textContent?.includes(${JSON.stringify(title)})) ?? null`;
}

// 搜索 → 点结果（revealTask：子会话/自动化任务以选中态出现在列表）→
// 点行上对话图标打开对话面板。
async function revealAndOpenTranscript(page, term, title) {
  await page.evaluate(
    `window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', metaKey: true }))`,
  );
  await page.waitFor("Boolean(document.querySelector('.search-input'))");
  await page.evaluate(`(() => {
    const input = document.querySelector('.search-input');
    input.value = ${JSON.stringify(term)};
    input.dispatchEvent(new Event('input', { bubbles: true }));
  })()`);
  await page.waitFor(
    `[...document.querySelectorAll('.search-result')].some(r =>
      r.textContent.includes(${JSON.stringify(title)}))`,
  );
  await page.evaluate(`(() => {
    const hit = [...document.querySelectorAll('.search-result')].find(r =>
      r.textContent.includes(${JSON.stringify(title)}));
    hit.click();
  })()`);
  // revealTask 以选中态展示该任务；任务非 primary，行出现即代表选中兜底生效。
  await page.waitFor("Boolean(document.querySelector('.flat-list .task.selected'))");
  // 探针：确认选中行即为目标任务且对话图标已渲染（revealTask 内部有 tick + 滚动定位链）
  const probe = await page.evaluate(`JSON.stringify((() => {
    const row = ${rowExpr(title)};
    return {
      found: Boolean(row),
      selected: Boolean(row?.querySelector('.task.selected')),
      icon: Boolean(row?.querySelector('.task-transcript')),
    };
  })())`);
  const state = JSON.parse(probe);
  if (!state.found || !state.selected || !state.icon) {
    throw new Error(`revealTask 后目标行状态异常：${probe}`);
  }
  await page.evaluate(`(() => {
    const main = document.querySelector('.flat-list .task.selected');
    main.parentElement.querySelector('.task-transcript').click();
  })()`);
  try {
    await page.waitFor("Boolean(document.querySelector('.transcript-pane'))", 3000);
  } catch (error) {
    const dump = await page.evaluate(`JSON.stringify((() => ({
      pane: Boolean(document.querySelector('.transcript-pane')),
      rowExists: Boolean(${rowExpr(title)}),
      anySelected: document.querySelectorAll('.flat-list .task.selected').length,
      reads: (window.__perchDemo ?? {}).readSessionCalls,
      searchLayer: Boolean(document.querySelector('.search-layer')),
      flatList: Boolean(document.querySelector('.flat-list')),
    }))())`);
    console.log(`[debug] ${title} 点击后面板未开：${dump}`);
    throw error;
  }
  await page.waitFor(
    "document.querySelectorAll('.transcript-pane .transcript-msg').length > 0",
  );
}

export async function run(page) {
  const s = suite;

  s.add("子会话经搜索打开对话 → sources-updated 刷新 → 面板保持、选中保持", async () => {
    await page.goto(`${URL}${T}`);
    await revealAndOpenTranscript(page, CHILD_TERM, CHILD);
    const before = await page.evaluate(
      `window.__perchDemo.readSessionCalls.length`,
    );
    // 实时刷新事件（不经过手动刷新路径）
    await page.evaluate(`window.__perchEmit('sources-updated', {})`);
    await new Promise((resolve) => setTimeout(resolve, 400));
    const state = await page.evaluate(`(() => {
      const pane = document.querySelector('.transcript-pane');
      return {
        paneOpen: Boolean(pane),
        title: pane?.querySelector('.transcript-title')?.textContent?.trim() ?? null,
        messages: pane ? pane.querySelectorAll('.transcript-msg').length : -1,
        reads: window.__perchDemo.readSessionCalls.length,
      };
    })()`);
    assertEqual(state.paneOpen, true, "sources-updated 后对话面板保持打开");
    assertEqual(state.title, CHILD, "面板标题仍为子会话任务");
    assert(state.messages > 0, `消息仍在（实际 ${state.messages}）`);
    assertEqual(state.reads, before, "面板保持时正文未重读（无关闭重开）");
    // Esc 关面板（T6：任务保持选中）→ 选中保持可观察
    await page.evaluate(
      `window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))`,
    );
    await page.waitFor("!document.querySelector('.transcript-pane')");
    const selection = await page.evaluate(`(() => {
      const row = ${rowExpr(CHILD)};
      return {
        rowListed: Boolean(row),
        rowSelected: Boolean(row?.querySelector('.task.selected')),
      };
    })()`);
    assertEqual(selection.rowListed, true, "子会话行仍在列表（选中兜底口径）");
    assertEqual(selection.rowSelected, true, "子会话任务仍为选中态");
  });

  s.add("自动化任务经搜索打开对话 → sources-updated 刷新 → 面板保持", async () => {
    await page.goto(`${URL}${T}`);
    await revealAndOpenTranscript(page, AUTOMATION_TERM, AUTOMATION);
    await page.evaluate(`window.__perchEmit('sources-updated', {})`);
    await new Promise((resolve) => setTimeout(resolve, 400));
    const state = await page.evaluate(`(() => {
      const pane = document.querySelector('.transcript-pane');
      return {
        paneOpen: Boolean(pane),
        title: pane?.querySelector('.transcript-title')?.textContent?.trim() ?? null,
      };
    })()`);
    assertEqual(state.paneOpen, true, "sources-updated 后对话面板保持打开");
    assertEqual(state.title, AUTOMATION, "面板标题仍为自动化任务");
  });

  s.add("子会话被筛出当前筛选列表 → 刷新后面板关闭、选中清除（负例）", async () => {
    await page.goto(`${URL}${T}&filterout=${encodeURIComponent(CHILD)}`);
    await revealAndOpenTranscript(page, CHILD_TERM, CHILD);
    // 面板打开时刷新按钮不在 DOM，用 ⌘R 触发刷新（refreshCount 达 2，filterout 生效）
    await page.evaluate(
      `window.dispatchEvent(new KeyboardEvent('keydown', { key: 'r', metaKey: true }))`,
    );
    await page.waitFor("!document.querySelector('.transcript-pane')");
    await page.waitFor("!document.querySelector('.refresh-icon.spinning')");
    const state = await page.evaluate(`(() => {
      const row = ${rowExpr(CHILD)};
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
      "被筛出的子会话不再出现在「最近」视图",
    );
  });

  const { pass, failures } = await suite.run(process.argv[2]);
  console.log(`\nT8 兜底口径结果：${pass} 通过，${failures.length} 失败`);
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
