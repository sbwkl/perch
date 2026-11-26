// T2：#27 对话内容口径 / 分页 / 截断 / 错误态与重读 / 头部（T3–T5 / T9 / T10 / T12）
import { runSuite, Suite, assert, assertEqual } from "./harness.mjs";

const URL = "http://localhost:1421/?transcript=1";

const suite = new Suite("T2 对话内容与分页");

function rowExpr(title) {
  return `[...document.querySelectorAll('.flat-list .task-row')].find(r =>
    r.querySelector('.task-title')?.textContent?.includes(${JSON.stringify(title)})) ?? null`;
}

async function open(page, title) {
  await page.evaluate(`(() => {
    const row = ${rowExpr(title)};
    row.querySelector('.task-transcript').click();
  })()`);
  await page.waitFor("Boolean(document.querySelector('.transcript-pane'))");
  // 等读取就绪：loading 态消失，且消息已渲染或错误态已出现
  await page.waitFor(
    `!document.querySelector('.transcript-pane .transcript-state.loading') &&
     (document.querySelectorAll('.transcript-pane .transcript-msg').length > 0 ||
      Boolean(document.querySelector('.transcript-pane .transcript-state.error')))`,
  );
}

async function run(page) {
  const s = suite;

  s.add("T3 角色正序、工具轮折叠、单条截断标注", async () => {
    await page.goto(URL);
    await open(page, "合成对话 · 工具轮与截断");
    const info = await page.evaluate(`(() => {
      const msgs = [...document.querySelectorAll('.transcript-pane .transcript-msg')];
      return {
        roles: msgs.map((m) => m.dataset.role),
        toolText: msgs.find(m => m.dataset.role === 'tool_call')?.textContent?.trim() ?? null,
        truncatedMarks: [...document.querySelectorAll('.transcript-pane .transcript-truncated')].map(e => e.textContent.trim()),
        unknownRoles: msgs.filter(m => !['user', 'assistant', 'tool_call'].includes(m.dataset.role)).length,
        preWrap: getComputedStyle(msgs[0]).whiteSpace,
      };
    })()`);
    assertEqual(info.roles, ["user", "tool_call", "assistant", "assistant", "user", "assistant"], "角色顺序");
    assertEqual(info.toolText, "工具调用 3 次", "工具轮折叠文案");
    assertEqual(info.unknownRoles, 0, "无未知角色");
    assertEqual(info.truncatedMarks.length, 1, "单条截断只有一处");
    assert(info.truncatedMarks[0].includes("已截断"), "截断标注文案");
    assertEqual(info.preWrap, "pre-wrap", "消息文本 pre-wrap 可换行复制");
  });

  s.add("T4 首屏 50 条 + 加载更早每次 50 条直至见底", async () => {
    await page.goto(URL);
    await open(page, "合成对话 · 分页长会话");
    const initial = await page.evaluate(`(() => {
      return {
        shown: document.querySelectorAll('.transcript-pane .transcript-msg').length,
        hasEarlier: Boolean(document.querySelector('.transcript-pane .transcript-earlier')),
      };
    })()`);
    assertEqual(initial.shown, 50, "默认最近 50 条");
    assertEqual(initial.hasEarlier, true, "提供加载更早");
    await page.evaluate(`document.querySelector('.transcript-pane .transcript-earlier').click()`);
    await page.waitFor("document.querySelectorAll('.transcript-pane .transcript-msg').length === 100");
    const second = await page.evaluate(`(() => {
      const msgs = [...document.querySelectorAll('.transcript-pane .transcript-msg')];
      return {
        shown: msgs.length,
        firstText: msgs[0]?.textContent?.trim() ?? null,
        hasEarlier: Boolean(document.querySelector('.transcript-pane .transcript-earlier')),
      };
    })()`);
    assertEqual(second.shown, 100, "追加 50 条");
    assertEqual(second.firstText, "长会话演示第 21 条消息，用于分页验收。", "追加的是更早消息");
    assertEqual(second.hasEarlier, true, "仍有更早");
    await page.evaluate(`document.querySelector('.transcript-pane .transcript-earlier').click()`);
    await page.waitFor("document.querySelectorAll('.transcript-pane .transcript-msg').length === 120");
    const third = await page.evaluate(`(() => ({
      shown: document.querySelectorAll('.transcript-pane .transcript-msg').length,
      hasEarlier: Boolean(document.querySelector('.transcript-pane .transcript-earlier')),
    }))()`);
    assertEqual(third.shown, 120, "全部消息可见");
    assertEqual(third.hasEarlier, false, "见底后按钮消失");
  });

  s.add("T5 首开定位最新消息；加载更早保持阅读位置", async () => {
    await page.goto(URL);
    await open(page, "合成对话 · 分页长会话");
    await page.waitFor("(() => { const el = document.querySelector('.transcript-pane .transcript-messages'); return el && el.scrollHeight > el.clientHeight; })()");
    const atBottom = await page.evaluate(`(() => {
      const el = document.querySelector('.transcript-pane .transcript-messages');
      return el.scrollHeight - el.scrollTop - el.clientHeight < 8;
    })()`);
    assertEqual(atBottom, true, "首开定位最新消息");
    const before = await page.evaluate(`(() => {
      const el = document.querySelector('.transcript-pane .transcript-messages');
      const rect = el.getBoundingClientRect();
      const first = [...el.querySelectorAll('.transcript-msg')].find(m => m.getBoundingClientRect().bottom > rect.top);
      return first?.textContent ?? null;
    })()`);
    await page.evaluate(`document.querySelector('.transcript-pane .transcript-earlier').click()`);
    await page.waitFor("document.querySelectorAll('.transcript-pane .transcript-msg').length === 100");
    const after = await page.evaluate(`(() => {
      const el = document.querySelector('.transcript-pane .transcript-messages');
      const rect = el.getBoundingClientRect();
      const first = [...el.querySelectorAll('.transcript-msg')].find(m => m.getBoundingClientRect().bottom > rect.top);
      return first?.textContent ?? null;
    })()`);
    assertEqual(after, before, "阅读位置保持不跳动");
  });

  s.add("T9 读取失败显示原因、不退出；空会话同样", async () => {
    await page.goto(URL);
    await open(page, "合成对话 · 首读失败后成功");
    await page.waitFor("Boolean(document.querySelector('.transcript-pane .transcript-state.error'))");
    const err = await page.evaluate(`(() => {
      const pane = document.querySelector('.transcript-pane');
      return {
        reason: pane.querySelector('.transcript-state.error')?.textContent ?? '',
        reread: Boolean(pane.querySelector('.transcript-reread')),
        paneStill: Boolean(document.querySelector('.transcript-pane')),
        noToast: !document.querySelector('.banner.error'),
      };
    })()`);
    assert(err.reason.includes("源文件不存在"), `显示失败原因（实际「${err.reason.trim()}」）`);
    assertEqual(err.reread, true, "头部保留重新读取");
    assertEqual(err.paneStill, true, "面板未退出");
    assertEqual(err.noToast, true, "无全局弹条");
    // 空会话
    await page.evaluate(`document.querySelector('.transcript-return')?.click()`);
    await page.waitFor("!document.querySelector('.transcript-pane')");
    await open(page, "合成对话 · 空会话");
    await page.waitFor("Boolean(document.querySelector('.transcript-pane .transcript-state.error'))");
    const empty = await page.evaluate(
      `document.querySelector('.transcript-pane .transcript-state.error')?.textContent ?? ''`,
    );
    assert(empty.includes("会话没有可显示的对话内容"), `空态原因（实际「${empty.trim()}」）`);
  });

  s.add("T10 重新读取成功替换内容并保留头部；失败保留原错误", async () => {
    await page.goto(URL);
    await open(page, "合成对话 · 首读失败后成功");
    await page.waitFor("Boolean(document.querySelector('.transcript-pane .transcript-state.error'))");
    await page.evaluate(`document.querySelector('.transcript-pane .transcript-reread').click()`);
    await page.waitFor("document.querySelectorAll('.transcript-pane .transcript-msg').length === 2");
    const after = await page.evaluate(`(() => {
      const pane = document.querySelector('.transcript-pane');
      return {
        errGone: !pane.querySelector('.transcript-state.error'),
        title: pane.querySelector('.transcript-title')?.textContent?.trim() ?? null,
        first: pane.querySelector('.transcript-msg')?.textContent?.includes('重新读取成功') ?? false,
      };
    })()`);
    assertEqual(after.errGone, true, "错误态清除");
    assertEqual(after.title, "合成对话 · 首读失败后成功", "头部保留");
    assertEqual(after.first, true, "内容已替换");
    // 重新读取仍失败：空会话任务重读后继续显示原因
    await page.evaluate(`document.querySelector('.transcript-return')?.click()`);
    await page.waitFor("!document.querySelector('.transcript-pane')");
    await open(page, "合成对话 · 空会话");
    await page.waitFor("Boolean(document.querySelector('.transcript-pane .transcript-state.error'))");
    await page.evaluate(`document.querySelector('.transcript-pane .transcript-reread').click()`);
    await page.waitFor("(() => { const b = document.querySelector('.transcript-pane .transcript-reread'); return b && !b.disabled; })()");
    const still = await page.evaluate(`(() => ({
      err: Boolean(document.querySelector('.transcript-pane .transcript-state.error')),
      msgs: document.querySelectorAll('.transcript-pane .transcript-msg').length,
    }))()`);
    assertEqual(still.err, true, "重读失败保持错误态");
    assertEqual(still.msgs, 0, "不渲染消息");
  });

  s.add("T12 头部齐全：来源 / 标题 / 状态徽标 / 三按钮；无隐私小字", async () => {
    await page.goto(URL);
    await open(page, "合成对话 · 超大会话截断");
    const head = await page.evaluate(`(() => {
      const pane = document.querySelector('.transcript-pane');
      const btn = (sel) => pane.querySelector(sel)?.textContent?.trim() ?? null;
      return {
        sourceIcon: Boolean(pane.querySelector('.transcript-head .source-icon')),
        sourceLabel: pane.querySelector('.transcript-head .transcript-source-label')?.textContent?.trim() ?? null,
        title: pane.querySelector('.transcript-head .transcript-title')?.textContent?.trim() ?? null,
        statusBadge: Boolean(pane.querySelector('.transcript-head .status')),
        continue: btn('.transcript-head .transcript-continue'),
        reread: btn('.transcript-head .transcript-reread'),
        back: btn('.transcript-head .transcript-return'),
        privacyLeak: /只读|仅本机|隐私|内存/.test(pane.textContent),
        sessionNotice: Boolean(pane.querySelector('.transcript-session-truncated')),
      };
    })()`);
    assertEqual(head.sourceIcon, true, "来源图标");
    assertEqual(head.sourceLabel, "Claude Code", "来源名");
    assertEqual(head.title, "合成对话 · 超大会话截断", "标题");
    assertEqual(head.statusBadge, true, "状态徽标");
    assertEqual(head.continue, "继续工作", "继续工作");
    assertEqual(head.reread, "重新读取", "重新读取");
    assertEqual(head.back, "返回", "返回");
    assertEqual(head.privacyLeak, false, "无隐私/只读常驻小字");
    assertEqual(head.sessionNotice, true, "超大会话保留提示");
  });

  s.add("超大会话提示 + 消息顺序保持", async () => {
    await page.goto(URL);
    await open(page, "合成对话 · 超大会话截断");
    const info = await page.evaluate(`(() => {
      const msgs = [...document.querySelectorAll('.transcript-pane .transcript-msg')];
      return {
        notice: document.querySelector('.transcript-pane .transcript-session-truncated')?.textContent ?? '',
        roles: msgs.map(m => m.dataset.role),
      };
    })()`);
    assert(info.notice.includes("最近"), `提示保留最近部分（实际「${info.notice.trim()}」）`);
    assertEqual(info.roles, ["user", "assistant", "user", "assistant"], "顺序保持");
  });

}

await runSuite("T2 结果", suite, run);

