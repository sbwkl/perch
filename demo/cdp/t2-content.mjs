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

// 气泡几何探针：以消息滚动容器的内容盒为基准，读出各角色消息的边界、宽度与对齐。
function bubbleProbeExpr() {
  return `(() => {
    const el = document.querySelector('.transcript-pane .transcript-messages');
    const cs = getComputedStyle(el);
    const padL = parseFloat(cs.paddingLeft);
    const padR = parseFloat(cs.paddingRight);
    const left = el.getBoundingClientRect().left + el.clientLeft + padL;
    const contentWidth = el.clientWidth - padL - padR;
    const rect = (m) => {
      const r = m.getBoundingClientRect();
      const style = getComputedStyle(m);
      return {
        left: r.left,
        right: r.right,
        width: r.width,
        textAlign: style.textAlign,
        background: style.backgroundColor,
      };
    };
    const pick = (role) => [...el.querySelectorAll('.transcript-msg')]
      .filter((m) => m.dataset.role === role)
      .map(rect);
    return {
      left,
      right: left + contentWidth,
      contentWidth,
      overflowX: el.scrollWidth > el.clientWidth,
      user: pick('user'),
      assistant: pick('assistant'),
      tool: pick('tool_call'),
    };
  })()`;
}

const near = (a, b, tolerance = 2) => Math.abs(a - b) <= tolerance;
const percent = (part, whole) => `${((part / whole) * 100).toFixed(1)}%`;

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

    // 排版几何：用户气泡靠右且限宽，助手整行，工具行居左透明底。
    const geo = await page.evaluate(bubbleProbeExpr());
    assertEqual(geo.user.length, 2, "两条用户消息");
    assertEqual(geo.assistant.length, 3, "三条助手消息");
    assertEqual(geo.tool.length, 1, "一条工具调用行");
    for (const [i, u] of geo.user.entries()) {
      assert(
        near(u.right, geo.right),
        `第 ${i + 1} 条用户气泡右边界贴近消息列右内缘（偏差 ${(u.right - geo.right).toFixed(1)}px）`,
      );
      assert(
        u.width <= geo.contentWidth * 0.78 + 0.5,
        `第 ${i + 1} 条用户气泡宽度不超过内容宽 78%（实际 ${percent(u.width, geo.contentWidth)}）`,
      );
      assertEqual(u.textAlign, "left", `第 ${i + 1} 条用户气泡内文左对齐`);
      assert(u.background !== "rgba(0, 0, 0, 0)", `第 ${i + 1} 条用户气泡保留底色`);
    }
    const userBg = geo.user[0].background;
    for (const [i, a] of geo.assistant.entries()) {
      assert(
        a.width >= geo.contentWidth * 0.96,
        `第 ${i + 1} 条助手消息约占内容整行宽（实际 ${percent(a.width, geo.contentWidth)}）`,
      );
      assert(near(a.left, geo.left), `第 ${i + 1} 条助手消息自内容左内缘起排（非右对齐气泡）`);
      assert(a.background !== userBg, `第 ${i + 1} 条助手消息不套用用户气泡底色`);
    }
    for (const [i, t] of geo.tool.entries()) {
      assert(near(t.left, geo.left), `第 ${i + 1} 条工具调用行左边界贴近内容左内缘`);
      assertEqual(t.background, "rgba(0, 0, 0, 0)", `第 ${i + 1} 条工具调用行背景透明`);
    }
    assertEqual(geo.overflowX, false, "气泡未造成消息区横向溢出");
  });

  s.add("用户长输入：气泡宽度受限且右对齐，助手整行宽", async () => {
    await page.goto(URL);
    await open(page, "合成对话 · 长用户输入");
    const geo = await page.evaluate(bubbleProbeExpr());
    assertEqual(geo.user.length, 1, "一条长用户消息");
    assertEqual(geo.assistant.length, 1, "一条助手回复");
    const user = geo.user[0];
    const assistant = geo.assistant[0];
    assert(
      user.width <= geo.contentWidth * 0.78 + 0.5,
      `长用户气泡宽度 ≤ 内容宽 78%（实际 ${percent(user.width, geo.contentWidth)}）`,
    );
    assert(
      user.width < geo.contentWidth * 0.9,
      `长用户气泡明显窄于整行，最大宽度生效（实际 ${percent(user.width, geo.contentWidth)}）`,
    );
    assert(
      near(user.right, geo.right),
      `长用户气泡仍右对齐（偏差 ${(user.right - geo.right).toFixed(1)}px）`,
    );
    assert(user.left > geo.left + 2, "长用户气泡不贴内容左缘");
    assertEqual(user.textAlign, "left", "长用户气泡内文左对齐");
    assert(
      assistant.width >= geo.contentWidth * 0.96,
      `助手回复占整行宽（实际 ${percent(assistant.width, geo.contentWidth)}）`,
    );
    assert(near(assistant.left, geo.left), "助手回复自内容左缘起排");
    assertEqual(geo.overflowX, false, "长用户气泡无横向溢出");
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

