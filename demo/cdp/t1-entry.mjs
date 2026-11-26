// T1：#27 对话面板入口与退出（T1 / T2 / T6 / T7 / T8）
// 入口 npm run cdp（自动起 demo）；单独运行先 npm run demo（端口 1421）。
import { runSuite, Suite, assert, assertEqual } from "./harness.mjs";

const URL = "http://localhost:1421/";
const T = "?transcript=1";

const suite = new Suite("T1 对话入口与退出");

// 帮助：按标题找到任务行容器
function rowExpr(title) {
  return `[...document.querySelectorAll('.flat-list .task-row')].find(r =>
    r.querySelector('.task-title')?.textContent?.includes(${JSON.stringify(title)})) ?? null`;
}

async function run(page) {
  const s = suite;

  s.add("T1 图标点击 = 选中任务 + 打开对话，面板占满 2、3 列", async () => {
    await page.goto(`${URL}${T}`);
    await page.evaluate(`(() => {
      const row = ${rowExpr("合成对话 · 工具轮与截断")};
      row.querySelector('.task-transcript').click();
    })()`);
    await page.waitFor("Boolean(document.querySelector('.transcript-pane'))");
    await page.waitFor(
      "document.querySelectorAll('.transcript-pane .transcript-msg').length > 0",
    );
    const state = await page.evaluate(`(() => {
      const pane = document.querySelector('.transcript-pane');
      return {
        title: pane?.querySelector('.transcript-title')?.textContent?.trim() ?? null,
        listGone: !document.querySelector('.flat-list'),
        inspectorGone: !document.querySelector('.inspector'),
        column: pane ? getComputedStyle(pane).gridColumnStart : null,
        columnEnd: pane ? getComputedStyle(pane).gridColumnEnd : null,
        sidebarAlive: Boolean(document.querySelector('.sidebar .nav-section')),
        messages: pane ? pane.querySelectorAll('.transcript-msg').length : -1,
      };
    })()`);
    assertEqual(state.title, "合成对话 · 工具轮与截断", "头部标题正确");
    assertEqual(state.listGone, true, "任务列表让位于对话面板");
    assertEqual(state.inspectorGone, true, "详情面板让位于对话面板");
    assertEqual(state.column, "2", "面板从第 2 列开始");
    assertEqual(state.columnEnd, "-1", "面板跨到最后一列");
    assertEqual(state.sidebarAlive, true, "侧栏保持可交互（仍在 DOM）");
    assert(state.messages > 0, `消息行已渲染（实际 ${state.messages}）`);
  });

  s.add("T2 Trae CN / CodeBuddy 行无对话图标；详情无解释文字", async () => {
    await page.goto(`${URL}${T}`);
    const icons = await page.evaluate(`(() => {
      const rows = [...document.querySelectorAll('.flat-list .task-row')];
      const grab = (label) => {
        const row = rows.find(r => r.querySelector('.task-title')?.textContent?.includes(label)) ?? null;
        return { row: Boolean(row), icon: Boolean(row?.querySelector('.task-transcript')) };
      };
      return {
        trae: grab('合成对话 · Trae CN 无图标'),
        codebuddy: grab('合成对话 · CodeBuddy 无图标'),
        claude: grab('合成对话 · 超大会话截断'),
      };
    })()`);
    assertEqual(icons.trae, { row: true, icon: false }, "Trae CN 行无图标");
    assertEqual(icons.codebuddy, { row: true, icon: false }, "CodeBuddy 行无图标");
    assertEqual(icons.claude, { row: true, icon: true }, "支持来源行有图标");
    // 选中 Trae CN 任务后详情面板不得解释正文缺失（标题本身含「对话」，先剥掉）
    await page.evaluate(`(() => {
      const row = ${rowExpr("合成对话 · Trae CN 无图标")};
      row.querySelector('.task').click();
    })()`);
    await page.waitFor("Boolean(document.querySelector('.inspector .inspector-head h2'))");
    const hasExplanation = await page.evaluate(`(() => {
      const inspector = document.querySelector('.inspector').cloneNode(true);
      inspector.querySelector('.inspector-head h2')?.remove();
      return inspector.textContent.includes('对话');
    })()`);
    assertEqual(hasExplanation, false, "详情面板不出现对话缺失解释");
  });

  s.add("T6 返回按钮恢复原列表与详情，任务保持选中", async () => {
    await page.goto(`${URL}${T}`);
    await page.evaluate(`(() => {
      const row = ${rowExpr("合成对话 · 超大会话截断")};
      row.querySelector('.task-transcript').click();
    })()`);
    await page.waitFor("Boolean(document.querySelector('.transcript-pane'))");
    await page.evaluate(`document.querySelector('.transcript-return')?.click()`);
    await page.waitFor("!document.querySelector('.transcript-pane')");
    const state = await page.evaluate(`(() => {
      const row = ${rowExpr("合成对话 · 超大会话截断")};
      return {
        listBack: Boolean(document.querySelector('.flat-list')),
        inspectorBack: Boolean(document.querySelector('.inspector')),
        rowSelected: Boolean(row?.querySelector('.task.selected')),
        inspectorTitle: document.querySelector('.inspector-head h2')?.textContent?.trim() ?? null,
      };
    })()`);
    assertEqual(state.listBack, true, "列表恢复");
    assertEqual(state.inspectorBack, true, "详情恢复");
    assertEqual(state.rowSelected, true, "该任务仍为选中态");
    assertEqual(state.inspectorTitle, "合成对话 · 超大会话截断", "详情标题一致");
  });

  s.add("T6 Esc 关闭对话并恢复，任务保持选中", async () => {
    await page.goto(`${URL}${T}`);
    await page.evaluate(`(() => {
      const row = ${rowExpr("合成对话 · 首读失败后成功")};
      row.querySelector('.task-transcript').click();
    })()`);
    await page.waitFor("Boolean(document.querySelector('.transcript-pane'))");
    await page.evaluate(
      `window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))`,
    );
    await page.waitFor("!document.querySelector('.transcript-pane')");
    const state = await page.evaluate(`(() => {
      const row = ${rowExpr("合成对话 · 首读失败后成功")};
      return {
        listBack: Boolean(document.querySelector('.flat-list')),
        rowSelected: Boolean(row?.querySelector('.task.selected')),
      };
    })()`);
    assertEqual(state.listBack, true, "Esc 后列表恢复");
    assertEqual(state.rowSelected, true, "Esc 后任务仍选中");
  });

  s.add("T7 对话打开时切换 workspace / 来源 / 视图 → 面板关闭", async () => {
    const openAntigravity = `(() => {
      const row = ${rowExpr("合成对话 · Antigravity 来源")};
      row.querySelector('.task-transcript').click();
    })()`;
    // workspace
    await page.goto(`${URL}${T}`);
    await page.evaluate(openAntigravity);
    await page.waitFor("Boolean(document.querySelector('.transcript-pane'))");
    await page.evaluate(`(() => {
      const ws = [...document.querySelectorAll('.nav-item')].find(b =>
        (b.getAttribute('title') ?? '').startsWith('/Users/demo/'));
      ws?.click();
    })()`);
    await page.waitFor("!document.querySelector('.transcript-pane')");
    assertEqual(
      await page.evaluate(`Boolean(document.querySelector('.flat-list'))`),
      true,
      "切 workspace 后列表恢复",
    );
    // source
    await page.goto(`${URL}${T}`);
    await page.evaluate(openAntigravity);
    await page.waitFor("Boolean(document.querySelector('.transcript-pane'))");
    await page.evaluate(`(() => {
      const src = [...document.querySelectorAll('.source-item')].find(b => b.getAttribute('aria-label')?.startsWith('Claude'));
      src?.click();
    })()`);
    await page.waitFor("!document.querySelector('.transcript-pane')");
    assertEqual(
      await page.evaluate(`Boolean(document.querySelector('.flat-list'))`),
      true,
      "切来源后列表恢复",
    );
    // 任务视图（更早）
    await page.goto(`${URL}${T}`);
    await page.evaluate(openAntigravity);
    await page.waitFor("Boolean(document.querySelector('.transcript-pane'))");
    await page.evaluate(`(() => {
      const nav = [...document.querySelectorAll('.nav-item')].find(b =>
        b.querySelector('.nav-label')?.textContent?.trim() === '更早');
      nav?.click();
    })()`);
    await page.waitFor("!document.querySelector('.transcript-pane')");
    assertEqual(
      await page.evaluate(`Boolean(document.querySelector('.flat-list'))`),
      true,
      "切视图后列表恢复",
    );
  });

  s.add("T8 刷新后任务消失 → 对话关闭、选中清除", async () => {
    await page.goto(`${URL}${T}&vanish=${encodeURIComponent("合成对话 · 超大会话截断")}`);
    await page.evaluate(`(() => {
      const row = ${rowExpr("合成对话 · 超大会话截断")};
      row.querySelector('.task-transcript').click();
    })()`);
    await page.waitFor("Boolean(document.querySelector('.transcript-pane'))");
    // 面板打开时刷新按钮不在 DOM（列表让位），用 ⌘R 触发刷新
    await page.evaluate(
      `window.dispatchEvent(new KeyboardEvent('keydown', { key: 'r', metaKey: true }))`,
    );
    await page.waitFor("!document.querySelector('.transcript-pane')");
    const state = await page.evaluate(`(() => {
      const anySelected = Boolean(document.querySelector('.flat-list .task.selected'));
      return {
        listBack: Boolean(document.querySelector('.flat-list')),
        anySelected,
      };
    })()`);
    assertEqual(state.listBack, true, "列表恢复");
    assertEqual(state.anySelected, false, "选中已清除（与列表选中规则一致）");
  });

}

await runSuite("T1 结果", suite, run);

