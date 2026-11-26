// T3：#27 视觉验收——900×560 无横向溢出、面板内部滚动、深浅色主题
import { runSuite, Suite, assert, assertEqual } from "./harness.mjs";
import { near, percent, bubbleProbeExpr } from "./transcript-probes.mjs";

const URL = "http://localhost:1421/?transcript=1";

const suite = new Suite("T3 视口与主题");

function rowExpr(title) {
  return `[...document.querySelectorAll('.flat-list .task-row')].find(r =>
    r.querySelector('.task-title')?.textContent?.includes(${JSON.stringify(title)})) ?? null`;
}

async function openPanel(page) {
  await page.evaluate(`(() => {
    const row = ${rowExpr("合成对话 · 分页长会话")};
    row.querySelector('.task-transcript').click();
  })()`);
  await page.waitFor("Boolean(document.querySelector('.transcript-pane'))");
  await page.waitFor(
    "(() => { const el = document.querySelector('.transcript-pane .transcript-messages'); return el && el.scrollHeight > el.clientHeight; })()",
  );
}

async function openTask(page, title) {
  await page.evaluate(`(() => {
    const row = ${rowExpr(title)};
    row.querySelector('.task-transcript').click();
  })()`);
  await page.waitFor("Boolean(document.querySelector('.transcript-pane'))");
  await page.waitFor(
    "document.querySelectorAll('.transcript-pane .transcript-msg').length > 0",
  );
}

function layoutProbe() {
  return `(() => {
    const doc = document.documentElement;
    const pane = document.querySelector('.transcript-pane');
    const head = pane?.querySelector('.transcript-head');
    const headRect = head?.getBoundingClientRect();
    const overlaps = head
      ? [...head.querySelectorAll('button')].some((b, i, all) =>
          all.slice(i + 1).some(o => {
            const r1 = b.getBoundingClientRect();
            const r2 = o.getBoundingClientRect();
            return r1.right > r2.left + 1 && r2.right > r1.left + 1;
          }))
      : true;
    return {
      overflowX: Math.max(doc.scrollWidth, document.body.scrollWidth) > innerWidth,
      messagesScrollable: pane
        ? pane.querySelector('.transcript-messages').scrollHeight > pane.querySelector('.transcript-messages').clientHeight
        : false,
      headInPane: Boolean(headRect && headRect.left >= pane.getBoundingClientRect().left - 1 && headRect.right <= pane.getBoundingClientRect().right + 1),
      buttonOverlap: overlaps,
      gridColumnStart: pane ? getComputedStyle(pane).gridColumnStart : null,
      gridColumnEnd: pane ? getComputedStyle(pane).gridColumnEnd : null,
    };
  })()`;
}

async function run(page) {
  const s = suite;

  s.add("9 个正文来源任务行有图标，Trae CN / CodeBuddy 无", async () => {
    await page.setViewport(1200, 780);
    await page.goto(URL);
    const marks = await page.evaluate(`(() => {
      const supported = ["Qoder", "Claude Code", "Codex", "OpenCode", "Kilo", "Cline", "Antigravity", "MiniMax Code", "CodeArts"];
      const unsupported = ["Trae CN", "CodeBuddy"];
      const rows = [...document.querySelectorAll('.flat-list .task-row')];
      const has = (label) => {
        const row = rows.find(r => r.querySelector('.task-meta')?.textContent?.includes(label));
        return row ? Boolean(row.querySelector('.task-transcript')) : null;
      };
      return {
        supported: Object.fromEntries(supported.map(l => [l, has(l)])),
        unsupported: Object.fromEntries(unsupported.map(l => [l, has(l)])),
      };
    })()`);
    for (const [label, value] of Object.entries(marks.supported)) {
      assertEqual(value, true, `${label} 行有对话图标`);
    }
    for (const [label, value] of Object.entries(marks.unsupported)) {
      assertEqual(value, false, `${label} 行无对话图标`);
    }
  });

  s.add("900×560 浅色：对话面板无横向溢出、内部滚动、头部按钮不重叠", async () => {
    await page.setViewport(900, 560);
    await page.setDark(false);
    try {
      await page.goto(URL);
      await openPanel(page);
      const layout = await page.evaluate(layoutProbe());
      assertEqual(layout.overflowX, false, "无横向溢出");
      assertEqual(layout.messagesScrollable, true, "消息区内部滚动");
      assertEqual(layout.headInPane, true, "头部未超出面板");
      assertEqual(layout.buttonOverlap, false, "头部按钮不重叠");
      assertEqual(layout.gridColumnStart, "2", "对话面板从第 2 列开始");
      assertEqual(layout.gridColumnEnd, "-1", "对话面板跨到最后一列");
      // 用户气泡与消息列内容盒的几何关系（不依赖滚动位置）。
      await page.goto(URL);
      await openTask(page, "合成对话 · 长用户输入");
      const bubbles = await page.evaluate(bubbleProbeExpr());
      const user = bubbles.user[0];
      const assistant = bubbles.assistant[0];
      assertEqual(bubbles.overflowX, false, "长用户气泡无横向溢出");
      assert(
        near(user.right, bubbles.right),
        `用户气泡右边界在消息列内（偏差 ${(user.right - bubbles.right).toFixed(1)}px）`,
      );
      assert(user.left >= bubbles.left - 1, "用户气泡左边界不越出消息列");
      assert(
        user.width <= bubbles.contentWidth * 0.78 + 0.5,
        `用户气泡宽度不超过内容宽 78%（实际 ${percent(user.width, bubbles.contentWidth)}）`,
      );
      assert(
        assistant.width >= bubbles.contentWidth * 0.96,
        `助手回复仍占整行宽（实际 ${percent(assistant.width, bubbles.contentWidth)}）`,
      );
      const longLayout = await page.evaluate(layoutProbe());
      assertEqual(longLayout.overflowX, false, "长用户输入下文档无横向溢出");
      assertEqual(longLayout.headInPane, true, "长用户输入下头部未超出面板");
    } finally {
      await page.setViewport(1200, 780);
    }
  });

  s.add("900×560 深色：同样布局约束", async () => {
    await page.setViewport(900, 560);
    await page.setDark(true);
    try {
      await page.goto(URL);
      await openPanel(page);
      const layout = await page.evaluate(layoutProbe());
      assertEqual(layout.overflowX, false, "深色无横向溢出");
      assertEqual(layout.messagesScrollable, true, "深色消息区内部滚动");
      assertEqual(layout.buttonOverlap, false, "深色头部按钮不重叠");
      const colors = await page.evaluate(`(() => {
        const pane = document.querySelector('.transcript-pane');
        const msg = pane.querySelector('.transcript-msg');
        return {
          paneBg: getComputedStyle(pane).backgroundColor,
          msgColor: getComputedStyle(msg).color,
        };
      })()`);
      assert(colors.paneBg !== "" && colors.paneBg !== "rgba(0, 0, 0, 0)", "深色面板有背景");
      assert(colors.msgColor !== "", "消息有前景色");
    } finally {
      await page.setDark(false);
      await page.setViewport(1200, 780);
    }
  });

  s.add("1200×780 深浅两色切换下面板保持可用", async () => {
    await page.setViewport(1200, 780);
    await page.goto(URL);
    await openPanel(page);
    const layout = await page.evaluate(layoutProbe());
    assertEqual(layout.gridColumnStart, "2", "1200×780 下面板从第 2 列开始");
    assertEqual(layout.gridColumnEnd, "-1", "1200×780 下面板跨到最后一列");
    await page.setDark(true);
    const darkOk = await page.evaluate(
      `Boolean(document.querySelector('.transcript-pane .transcript-msg'))`,
    );
    assertEqual(darkOk, true, "深色下消息仍在");
    await page.setDark(false);
    const lightOk = await page.evaluate(
      `Boolean(document.querySelector('.transcript-pane .transcript-msg'))`,
    );
    assertEqual(lightOk, true, "浅色下消息仍在");
  });

}

await runSuite("T3 结果", suite, run);

