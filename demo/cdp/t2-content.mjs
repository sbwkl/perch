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

// 等首开「定位最新消息」的滚动落定（tick 后 scrollTop = scrollHeight）。
async function waitAtBottom(page) {
  await page.waitFor(
    "(() => { const el = document.querySelector('.transcript-pane .transcript-messages'); return el && el.scrollHeight > el.clientHeight && el.scrollHeight - el.scrollTop - el.clientHeight < 8; })()",
  );
}

// 把消息列滚到最顶部并等待生效（「加载更早」是第一个内容行，只有滚到顶部才可见）。
async function scrollMessagesTop(page) {
  await page.evaluate(
    "(() => { const el = document.querySelector('.transcript-pane .transcript-messages'); el.scrollTop = 0; })()",
  );
  await page.waitFor(
    "(() => { const el = document.querySelector('.transcript-pane .transcript-messages'); return el && el.scrollTop === 0; })()",
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

  s.add("T4 加载更早在列表首行：滚顶可见、逐次 -50 直至消失", async () => {
    await page.goto(URL);
    await open(page, "合成对话 · 分页长会话");
    await waitAtBottom(page);

    const initial = await page.evaluate(`(() => {
      const container = document.querySelector('.transcript-pane .transcript-messages');
      const btn = container.querySelector('.transcript-earlier');
      const cr = container.getBoundingClientRect();
      const br = btn ? btn.getBoundingClientRect() : null;
      return {
        shown: document.querySelectorAll('.transcript-pane .transcript-msg').length,
        hasEarlier: Boolean(btn),
        parentIsContainer: btn ? btn.parentElement === container : false,
        firstRowIsButton: container.firstElementChild === btn,
        offscreenAtBottom: br ? br.bottom <= cr.top + 1 : true,
      };
    })()`);
    assertEqual(initial.shown, 50, "默认最近 50 条");
    assertEqual(initial.hasEarlier, true, "提供加载更早");
    assertEqual(initial.parentIsContainer, true, "按钮在滚动容器内部");
    assertEqual(initial.firstRowIsButton, true, "按钮是列表第一个内容行");
    assertEqual(initial.offscreenAtBottom, true, "首开定位底部时按钮不在视口（反向）");

    // 滚到顶部 → 按钮进入视口，文案显示剩余 70
    await scrollMessagesTop(page);
    const atTop = await page.evaluate(`(() => {
      const container = document.querySelector('.transcript-pane .transcript-messages');
      const btn = container.querySelector('.transcript-earlier');
      const cr = container.getBoundingClientRect();
      const br = btn.getBoundingClientRect();
      return { inView: br.top >= cr.top - 1 && br.bottom <= cr.bottom + 1, text: btn.textContent.trim() };
    })()`);
    assert(atTop.inView, "滚到顶部后按钮在视口内");
    assert(atTop.text.includes("还有 70 条"), `顶部文案显示剩余 70 条（实际「${atTop.text}」）`);

    // 第一次点击 → 100 条；锚定阅读位置会离开顶部，按钮随之不可见（反向）
    await page.evaluate(`document.querySelector('.transcript-pane .transcript-earlier').click()`);
    await page.waitFor("document.querySelectorAll('.transcript-pane .transcript-msg').length === 100");
    await page.waitFor(
      "(() => { const el = document.querySelector('.transcript-pane .transcript-messages'); return el && el.scrollTop > 1; })()",
    );
    const afterFirst = await page.evaluate(`(() => {
      const container = document.querySelector('.transcript-pane .transcript-messages');
      const btn = container.querySelector('.transcript-earlier');
      const cr = container.getBoundingClientRect();
      const br = btn ? btn.getBoundingClientRect() : null;
      return {
        shown: document.querySelectorAll('.transcript-pane .transcript-msg').length,
        hasEarlier: Boolean(btn),
        belowViewport: br ? br.bottom <= cr.top + 1 : false,
        text: btn ? btn.textContent.trim() : null,
      };
    })()`);
    assertEqual(afterFirst.shown, 100, "追加 50 条 → 100");
    assertEqual(afterFirst.hasEarlier, true, "仍有更早（还剩 20）");
    assertEqual(afterFirst.belowViewport, true, "未滚回顶部时按钮不可见（反向）");
    assert(afterFirst.text.includes("还有 20 条"), `文案递减为剩余 20 条（实际「${afterFirst.text}」）`);

    // 再滚到顶 → 点击 → 120 条，按钮从 DOM 消失
    await scrollMessagesTop(page);
    await page.evaluate(`document.querySelector('.transcript-pane .transcript-earlier').click()`);
    await page.waitFor("document.querySelectorAll('.transcript-pane .transcript-msg').length === 120");
    const done = await page.evaluate(`(() => ({
      shown: document.querySelectorAll('.transcript-pane .transcript-msg').length,
      gone: document.querySelector('.transcript-pane .transcript-earlier') === null,
    }))()`);
    assertEqual(done.shown, 120, "反复点击至全部 120 条");
    assertEqual(done.gone, true, "剩余为 0 时按钮从 DOM 消失（反向）");
  });

  s.add("T5 首开定位最新消息；加载更早保持阅读位置", async () => {
    await page.goto(URL);
    await open(page, "合成对话 · 分页长会话");
    await waitAtBottom(page);
    const atBottom = await page.evaluate(`(() => {
      const el = document.querySelector('.transcript-pane .transcript-messages');
      return el.scrollHeight - el.scrollTop - el.clientHeight < 8;
    })()`);
    assertEqual(atBottom, true, "首开定位最新消息");
    // 滚到顶部，记录当前顶端可见消息及其相对位置
    await scrollMessagesTop(page);
    const before = await page.evaluate(`(() => {
      const el = document.querySelector('.transcript-pane .transcript-messages');
      const rect = el.getBoundingClientRect();
      const first = [...el.querySelectorAll('.transcript-msg')].find(m => m.getBoundingClientRect().bottom > rect.top);
      return {
        text: first?.textContent ?? null,
        relTop: first ? first.getBoundingClientRect().top - rect.top : null,
      };
    })()`);
    await page.evaluate(`document.querySelector('.transcript-pane .transcript-earlier').click()`);
    await page.waitFor("document.querySelectorAll('.transcript-pane .transcript-msg').length === 100");
    await page.waitFor(
      "(() => { const el = document.querySelector('.transcript-pane .transcript-messages'); return el && el.scrollTop > 1; })()",
    );
    const after = await page.evaluate(`(() => {
      const el = document.querySelector('.transcript-pane .transcript-messages');
      const rect = el.getBoundingClientRect();
      const same = [...el.querySelectorAll('.transcript-msg')].find(m => m.textContent === ${JSON.stringify(before.text)});
      return {
        found: Boolean(same),
        relTop: same ? same.getBoundingClientRect().top - rect.top : null,
        atTop: el.scrollTop <= 1,
        atBottom: el.scrollHeight - el.scrollTop - el.clientHeight < 8,
      };
    })()`);
    assertEqual(after.found, true, "原顶端消息仍在列表");
    assert(Math.abs(after.relTop - before.relTop) < 3, `阅读位置保持不跳动（原 ${before.relTop} / 后 ${after.relTop}）`);
    assertEqual(after.atTop, false, "不跳回顶部");
    assertEqual(after.atBottom, false, "不跳到底部");
  });

  s.add("短会话一屏内：加载更早在顶部直接可见", async () => {
    await page.setViewport(1200, 2600);
    try {
      await page.goto(URL);
      await open(page, "合成对话 · 分页长会话");
      await page.waitFor("document.querySelectorAll('.transcript-pane .transcript-msg').length === 50");
      const info = await page.evaluate(`(() => {
        const el = document.querySelector('.transcript-pane .transcript-messages');
        const btn = el.querySelector('.transcript-earlier');
        const cr = el.getBoundingClientRect();
        const br = btn ? btn.getBoundingClientRect() : null;
        return {
          notScrollable: el.scrollHeight - el.clientHeight < 2,
          hasEarlier: Boolean(btn),
          inView: br ? br.top >= cr.top - 1 && br.bottom <= cr.bottom + 1 : false,
          text: btn ? btn.textContent.trim() : null,
        };
      })()`);
      assertEqual(info.notScrollable, true, "50 条一屏放得下、容器不可滚");
      assertEqual(info.hasEarlier, true, "仍有更早消息");
      assert(info.inView, "按钮在容器视口内直接可见");
      assert(info.text.includes("还有 70 条"), `文案显示剩余 70 条（实际「${info.text}」）`);
    } finally {
      await page.setViewport(1200, 780);
    }
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

await runSuite("T2 结果", suite, run, { viewport: [1200, 780] });

