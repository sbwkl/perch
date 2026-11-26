// S1：三组单选状态机 / 全部口径 / 报告旁路 / 清除作用域兜底（票据 #25）
import { runSuite, Suite, assert, assertEqual } from "./harness.mjs";

const URL = "http://localhost:1421/";

const q = (s) => JSON.stringify(s);

const expr = {
  navPressed: (label) =>
    `[...document.querySelectorAll('.nav-item')].find(b => b.querySelector('.nav-label')?.textContent?.trim() === ${q(label)})?.getAttribute('aria-pressed') === 'true'`,
  navCount: (label) =>
    `[...document.querySelectorAll('.nav-item')].find(b => b.querySelector('.nav-label')?.textContent?.trim() === ${q(label)})?.querySelector('.nav-count')?.textContent?.trim()`,
  wsPressed: (path) =>
    `[...document.querySelectorAll('.nav-item')].find(b => b.getAttribute('title') === ${q(path)})?.getAttribute('aria-pressed') === 'true'`,
  sourcePressed: (label) =>
    `[...document.querySelectorAll('.source-item')].find(b => b.getAttribute('aria-label')?.startsWith(${q(label)}))?.getAttribute('aria-pressed') === 'true'`,
  sourceCount: (label) =>
    `[...document.querySelectorAll('.source-item')].find(b => b.getAttribute('aria-label')?.startsWith(${q(label)}))?.querySelector('.nav-count')?.textContent?.trim()`,
  workspaceLabels: () =>
    `[...document.querySelectorAll('.sidebar-scroll > nav:nth-child(2) .nav-item[title]')].map(b => b.querySelector('.nav-label')?.textContent?.trim())`,
  titles: () =>
    `[...document.querySelectorAll('.flat-list .task .task-title')].map(t => t.textContent.trim())`,
  heading: () => `document.querySelector('.head-text h2')?.textContent?.trim()`,
  subheading: () => `document.querySelector('.head-text p')?.textContent?.trim()`,
  selectedTask: () =>
    `document.querySelector('.flat-list .task.selected .task-title')?.textContent?.trim() ?? null`,
  emptyState: () => `document.querySelector('.empty-state')?.textContent ?? null`,
  reportFrame: () => `Boolean(document.querySelector('.report-frame'))`,
  reportActive: () =>
    `document.querySelector('.sidebar-report')?.classList.contains('active') === true`,
};

async function click(page, expression) {
  await page.evaluate(`(() => { const el = (${expression}); if (!el) throw new Error('找不到元素'); el.click(); })()`);
}

const clickNav = (page, label) =>
  click(page, `[...document.querySelectorAll('.nav-item')].find(b => b.querySelector('.nav-label')?.textContent?.trim() === ${q(label)})`);
const clickWs = (page, path) =>
  click(page, `[...document.querySelectorAll('.nav-item')].find(b => b.getAttribute('title') === ${q(path)})`);
const clickSource = (page, label) =>
  click(page, `[...document.querySelectorAll('.source-item')].find(b => b.getAttribute('aria-label')?.startsWith(${q(label)}))`);

const WS0 = "/Users/demo/code/acme-storefront";
const WS3 = "/Users/demo/code/scratchpad";

const RECENT_ALL = [
  "重构结算页的价格计算逻辑",
  "更新 API 参考文档",
  "梳理 Cline 会话存储结构",
  "补齐指标看板图表",
  "为订单导出补充 CSV 单元测试",
  "修复移动端导航栏的层级问题",
  "翻译快速上手章节",
  "搭建日志聚合原型",
  "接入 OAuth 登录回调",
  "清理未使用的 Tailwind 插件",
  "生成 changelog 草稿",
  "批量重命名截图文件",
  "修复 Cline 配置迁移脚本",
];

const suite = new Suite("S1 三组单选");

async function run(page) {
  const s = suite;

  s.add("默认：最近高亮，两组未选，列表 11 条按序", async () => {
    await page.goto(URL);
    assertEqual(await page.evaluate(expr.navPressed("最近")), true, "最近应选中");
    assertEqual(await page.evaluate(expr.navPressed("更早")), false, "更早不应选中");
    assertEqual(await page.evaluate(expr.navPressed("已归档")), false, "已归档不应选中");
    assertEqual(await page.evaluate(expr.wsPressed(WS0)), false, "workspace 不应选中");
    assertEqual(await page.evaluate(expr.sourcePressed("Codex")), false, "来源不应选中");
    assertEqual(await page.evaluate(expr.navCount("最近")), "13", "最近计数");
    assertEqual(await page.evaluate(expr.navCount("更早")), "2", "更早计数");
    assertEqual(await page.evaluate(expr.navCount("已归档")), "4", "已归档计数");
    assertEqual(await page.evaluate(expr.titles()), RECENT_ALL, "默认列表");
  });

  s.add("门控正例-默认只剩任务组时点最近：保持选中（不出现三组全空）", async () => {
    await page.goto(URL);
    await clickNav(page, "最近");
    assertEqual(await page.evaluate(expr.navPressed("最近")), true, "最近应保持选中");
    assertEqual((await page.evaluate(expr.titles())).length, 13, "列表不变");
  });

  s.add("正例-点 workspace：任务组+workspace 两组高亮，列表按 workspace 过滤", async () => {
    await page.goto(URL);
    await clickWs(page, WS0);
    assertEqual(await page.evaluate(expr.navPressed("最近")), true, "最近应选中");
    assertEqual(await page.evaluate(expr.wsPressed(WS0)), true, "workspace 应选中");
    assertEqual(await page.evaluate(expr.titles()), [
      "重构结算页的价格计算逻辑",
      "为订单导出补充 CSV 单元测试",
      "修复移动端导航栏的层级问题",
      "接入 OAuth 登录回调",
      "清理未使用的 Tailwind 插件",
    ], "ws0 最近列表");
    assertEqual(await page.evaluate(expr.navCount("已归档")), "1", "已归档计数联动 ws0");
  });

  s.add("正例-再点来源：三组高亮，计数三组联动", async () => {
    await page.goto(URL);
    await clickWs(page, WS0);
    await clickSource(page, "Codex");
    assertEqual(await page.evaluate(expr.navPressed("最近")), true, "最近应选中");
    assertEqual(await page.evaluate(expr.wsPressed(WS0)), true, "workspace 应选中");
    assertEqual(await page.evaluate(expr.sourcePressed("Codex")), true, "来源应选中");
    assertEqual(await page.evaluate(expr.titles()), ["修复移动端导航栏的层级问题"], "三组交集列表");
    assertEqual(await page.evaluate(expr.navCount("最近")), "1", "最近计数联动");
    assertEqual(await page.evaluate(expr.navCount("更早")), "0", "更早计数联动");
    assertEqual(await page.evaluate(expr.navCount("已归档")), "1", "已归档计数联动");
    assertEqual(await page.evaluate(expr.sourceCount("Codex")), "1", "来源计数联动");
  });

  s.add("正例-组内点击另一项：选中转移", async () => {
    await page.goto(URL);
    await clickWs(page, WS3);
    await clickNav(page, "更早");
    assertEqual(await page.evaluate(expr.navPressed("最近")), false, "最近应取消");
    assertEqual(await page.evaluate(expr.navPressed("更早")), true, "更早应选中");
    assertEqual(await page.evaluate(expr.wsPressed(WS3)), true, "workspace 保留");
    assertEqual(await page.evaluate(expr.titles()), [
      "实验：多模态输入解析",
      "调研向量检索方案",
    ], "更早+ws3 列表");
  });

  s.add("正例-组数>1 时再点已选中任务项：取消该组回全部", async () => {
    await page.goto(URL);
    await clickWs(page, WS3);
    await clickSource(page, "Codex");
    await clickNav(page, "最近");
    assertEqual(await page.evaluate(expr.navPressed("最近")), false, "最近应取消");
    assertEqual(await page.evaluate(expr.wsPressed(WS3)), true, "workspace 保留");
    assertEqual(await page.evaluate(expr.sourcePressed("Codex")), true, "来源保留");
    assertEqual(await page.evaluate(expr.titles()), ["批量重命名截图文件"], "全部+ws+来源列表");
  });

  s.add("正例-继续取消来源组（组数仍>1）：回 workspace+全部", async () => {
    await page.goto(URL);
    await clickWs(page, WS3);
    await clickNav(page, "最近");
    await clickSource(page, "Codex");
    await clickSource(page, "Codex");
    assertEqual(await page.evaluate(expr.sourcePressed("Codex")), false, "来源应取消");
    assertEqual(await page.evaluate(expr.wsPressed(WS3)), true, "workspace 保留");
    assertEqual(await page.evaluate(expr.titles()), [
      "批量重命名截图文件",
      "修复 Cline 配置迁移脚本",
      "实验：多模态输入解析",
      "调研向量检索方案",
    ], "ws3 全部未归档 4 条");
  });

  s.add("门控反例-只剩 workspace 一组时再点它：保持选中", async () => {
    await page.goto(URL);
    await clickWs(page, WS3);
    await clickNav(page, "最近");
    await clickWs(page, WS3);
    assertEqual(await page.evaluate(expr.wsPressed(WS3)), true, "workspace 应保持选中");
    assertEqual((await page.evaluate(expr.titles())).length, 4, "列表不变");
  });

  s.add("门控反例-只剩任务组时点更早再点更早：保持选中", async () => {
    await page.goto(URL);
    await clickNav(page, "更早");
    await clickNav(page, "更早");
    assertEqual(await page.evaluate(expr.navPressed("更早")), true, "更早应保持选中");
    assertEqual((await page.evaluate(expr.titles())).length, 2, "更早列表不变");
  });

  s.add("全部口径-任务组取消+scratchpad：列表=全部未归档按活动倒序，无归档", async () => {
    await page.goto(URL);
    await clickWs(page, WS3);
    await clickNav(page, "最近");
    assertEqual(await page.evaluate(expr.navPressed("最近")), false, "最近应取消");
    assertEqual(await page.evaluate(expr.navPressed("更早")), false, "更早应取消");
    assertEqual(await page.evaluate(expr.wsPressed(WS3)), true, "scratchpad 应选中");
    assertEqual(await page.evaluate(expr.titles()), [
      "批量重命名截图文件",
      "修复 Cline 配置迁移脚本",
      "实验：多模态输入解析",
      "调研向量检索方案",
    ], "全部未归档列表");
    assertEqual(await page.evaluate(expr.navCount("最近")), "2", "最近计数联动");
    assertEqual(await page.evaluate(expr.navCount("更早")), "2", "更早计数联动");
    assertEqual(await page.evaluate(expr.navCount("已归档")), "1", "已归档计数联动");
    assertEqual(await page.evaluate(expr.workspaceLabels()), ["acme-storefront", "docs-portal", "data-pipeline", "scratchpad"], "workspace 计数按全部未归档口径");
  });

  s.add("已归档仍只显示归档", async () => {
    await page.goto(URL);
    await clickNav(page, "已归档");
    assertEqual(await page.evaluate(expr.titles()), [
      "验证图片压缩流程",
      "旧版落地页迁移",
      "临时排查构建失败",
      "整理会议纪要",
    ], "已归档列表");
    await clickWs(page, WS3);
    assertEqual(await page.evaluate(expr.titles()), ["整理会议纪要"], "已归档+ws 交集");
  });

  s.add("兜底正例-任务组为全部时清除作用域：任务组回落最近", async () => {
    await page.goto(URL);
    await clickWs(page, WS3);
    await clickNav(page, "最近");
    assertEqual(await page.evaluate(expr.navPressed("最近")), false, "前置：任务组应为全部");
    assertEqual(await page.evaluate(expr.wsPressed(WS3)), true, "前置：workspace 应选中");
    await clickNav(page, "搜索");
    await page.waitFor("Boolean(document.querySelector('.search-scope-clear'))");
    await click(page, `document.querySelector('.search-scope-clear')`);
    assertEqual(await page.evaluate(expr.navPressed("最近")), true, "任务组应回落最近");
    assertEqual(await page.evaluate(expr.wsPressed(WS3)), false, "workspace 应清除");
    await page.evaluate(`window.dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape', bubbles: true}))`);
    assertEqual((await page.evaluate(expr.titles())).length, 13, "回落到全部最近任务");
  });

  s.add("兜底反例-任务组已选中时清除作用域：不强制回落，保留原视图", async () => {
    await page.goto(URL);
    await clickNav(page, "已归档");
    await clickWs(page, WS3);
    await clickNav(page, "搜索");
    await page.waitFor("Boolean(document.querySelector('.search-scope-clear'))");
    await click(page, `document.querySelector('.search-scope-clear')`);
    assertEqual(await page.evaluate(expr.navPressed("已归档")), true, "已归档应保留");
    assertEqual(await page.evaluate(expr.navPressed("最近")), false, "不应回落最近");
    assertEqual(await page.evaluate(expr.wsPressed(WS3)), false, "workspace 应清除");
    await page.evaluate(`window.dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape', bubbles: true}))`);
    assertEqual((await page.evaluate(expr.titles())).length, 4, "全部归档任务");
  });

  s.add("报告正例-进入报告：清空任务组选中、筛选保留、报告渲染", async () => {
    await page.goto(URL);
    await clickWs(page, WS3);
    await clickNav(page, "更早");
    await click(page, `document.querySelector('.sidebar-report')`);
    await page.waitFor("Boolean(document.querySelector('.report-frame'))");
    assertEqual(await page.evaluate(expr.heading()), "用量报告", "报告标题");
    assertEqual(await page.evaluate(expr.navPressed("更早")), false, "任务组应清空");
    assertEqual(await page.evaluate(expr.navPressed("最近")), false, "最近应清空");
    assertEqual(await page.evaluate(expr.wsPressed(WS3)), true, "workspace 筛选保留");
    assertEqual(await page.evaluate(expr.reportActive()), true, "报告入口激活");
    assertEqual(await page.evaluate(expr.navCount("更早")), "2", "报告期间侧栏计数不跳变");
  });

  s.add("报告正例-返回任务：恢复原任务视图与筛选", async () => {
    await page.goto(URL);
    await clickWs(page, WS3);
    await clickNav(page, "更早");
    await click(page, `document.querySelector('.sidebar-report')`);
    await page.waitFor("Boolean(document.querySelector('.report-frame'))");
    await click(page, `[...document.querySelectorAll('.refresh-button')].find(b => b.textContent.includes('返回任务'))`);
    assertEqual(await page.evaluate(expr.navPressed("更早")), true, "更早应恢复");
    assertEqual(await page.evaluate(expr.wsPressed(WS3)), true, "workspace 应恢复");
    assertEqual(await page.evaluate(expr.titles()), [
      "实验：多模态输入解析",
      "调研向量检索方案",
    ], "恢复原列表");
    assertEqual(await page.evaluate(expr.heading()), "scratchpad", "恢复标题");
  });

  s.add("报告反例-报告期间点任务视图项：退出报告并选中", async () => {
    await page.goto(URL);
    await clickWs(page, WS3);
    await click(page, `document.querySelector('.sidebar-report')`);
    await page.waitFor("Boolean(document.querySelector('.report-frame'))");
    await clickNav(page, "最近");
    assertEqual(await page.evaluate(expr.reportFrame()), false, "报告应退出");
    assertEqual(await page.evaluate(expr.navPressed("最近")), true, "最近应选中");
    assertEqual(await page.evaluate(expr.wsPressed(WS3)), true, "workspace 保留");
    assertEqual((await page.evaluate(expr.titles())).length, 2, "ws3 最近 2 条");
  });

  s.add("报告反例-全部+筛选进入报告再返回：恢复全部口径", async () => {
    await page.goto(URL);
    await clickWs(page, WS0);
    await clickNav(page, "最近");
    await click(page, `document.querySelector('.sidebar-report')`);
    await page.waitFor("Boolean(document.querySelector('.report-frame'))");
    await click(page, `[...document.querySelectorAll('.refresh-button')].find(b => b.textContent.includes('返回任务'))`);
    assertEqual(await page.evaluate(expr.navPressed("最近")), false, "任务组保持全部");
    assertEqual(await page.evaluate(expr.wsPressed(WS0)), true, "workspace 恢复");
    assertEqual((await page.evaluate(expr.titles())).length, 5, "ws0 全部未归档");
  });

}

await runSuite("S1 结果", suite, run, { viewport: [1200, 780] });
