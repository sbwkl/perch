// T3：#27 视觉验收——900×560 无横向溢出、面板内部滚动、深浅色主题
import { launchChrome, Page, Suite, assert, assertEqual } from "./harness.mjs";

const URL = "http://localhost:1421/?transcript=1";

export const suite = new Suite("T3 视口与主题");

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
    };
  })()`;
}

export async function run(page) {
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

  const { pass, failures } = await suite.run(process.argv[2]);
  console.log(`\nT3 结果：${pass} 通过，${failures.length} 失败`);
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
