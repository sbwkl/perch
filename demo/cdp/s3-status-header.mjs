// S3：扫描状态栏整条消失 / 副标题只剩「N 个任务 · 快照 X」/ usage-strip 只留三数值 pill
import { readFileSync } from "node:fs";
import { launchChrome, Page, Suite, assert, assertEqual } from "./harness.mjs";

const URL = "http://localhost:1421/";
const APP = "/Users/wayne/workspace/perch-wt-t25/src/App.svelte";

const q = (s) => JSON.stringify(s);

const clickSource = (page, label) =>
  page.evaluate(`[...document.querySelectorAll('.source-item')].find(b => b.getAttribute('aria-label')?.startsWith(${q(label)}))?.click()`);

export const suite = new Suite("S3 状态栏与头部提示");

export async function run(page) {
  const s = suite;

  s.add("扫描状态栏整条不存在，无暂停/恢复实时入口", async () => {
    await page.goto(URL);
    assertEqual(await page.evaluate("Boolean(document.querySelector('.scan-status'))"), false, ".scan-status 存在");
    assertEqual(await page.evaluate("Boolean(document.querySelector('.scan-toggle'))"), false, ".scan-toggle 存在");
    const text = await page.evaluate("document.body.innerText");
    assert(!text.includes("暂停实时"), "不应出现「暂停实时」");
    assert(!text.includes("恢复实时"), "不应出现「恢复实时」");
    assert(!text.includes("后台更新"), "不应出现「后台更新」状态文案");
  });

  s.add("副标题只剩「N 个任务 · 快照 X」", async () => {
    await page.goto(URL);
    const sub = await page.evaluate("document.querySelector('.head-text p')?.textContent?.trim()");
    assert(
      /^(\d+ \/ \d+|\d+) 个任务 · 快照 /.test(sub),
      `副标题口径不符：${sub}`,
    );
    assert(!sub.includes("扫描"), "副标题不应含扫描耗时");
    assert(!sub.includes("ms"), "副标题不应含毫秒数");
    assert(!sub.includes("用量"), "副标题不应含用量信息");
    assert(!sub.includes("匹配"), "副标题不应含匹配数");
  });

  s.add("选中 Codex：usage-strip 只含 token 数值 pill，无过期/快照/匹配", async () => {
    await page.goto(URL);
    await clickSource(page, "Codex");
    await page.waitFor("Boolean(document.querySelector('.usage-strip'))");
    const pills = await page.evaluate(
      `[...document.querySelectorAll('.usage-strip .usage-pill')].map(p => p.textContent.trim())`,
    );
    assertEqual(pills.length, 1, "Codex 只有 token pill");
    assert(pills[0].includes("tokens"), "token pill 文案");
    const strip = await page.evaluate("document.querySelector('.usage-strip')?.textContent ?? ''");
    assert(!strip.includes("过期"), "不应含「用量可能已过期」");
    assert(!strip.includes("用量快照"), "不应含「用量快照」");
    assert(!strip.includes("匹配"), "不应含「匹配 N 个会话」");
  });

  s.add("选中 Qoder：credits + est 两个数值 pill", async () => {
    await page.goto(URL);
    await clickSource(page, "Qoder");
    await page.waitFor("Boolean(document.querySelector('.usage-strip'))");
    const pills = await page.evaluate(
      `[...document.querySelectorAll('.usage-strip .usage-pill')].map(p => p.textContent.trim())`,
    );
    assertEqual(pills.length, 2, "Qoder 是 credits + est");
    assert(pills.some((p) => p.includes("credits")), "credits pill");
    assert(pills.some((p) => p.includes("est")), "est pill");
  });

  s.add("选中无用量数据的来源：不显示 usage-strip", async () => {
    await page.goto(URL);
    await clickSource(page, "Trae CN");
    await new Promise((r) => setTimeout(r, 300));
    assertEqual(await page.evaluate("Boolean(document.querySelector('.usage-strip'))"), false, "无用量来源不应有条");
  });

  s.add("刷新按钮以「扫描中」+ spinner 表达手动扫描", async () => {
    await page.goto(URL);
    const during = await page.evaluate(`(async () => {
      document.querySelector('.refresh-button').click();
      await new Promise((resolve) => setTimeout(resolve, 30));
      const button = document.querySelector('.refresh-button');
      return {
        label: button.textContent.trim(),
        spinning: Boolean(document.querySelector('.refresh-icon.spinning')),
      };
    })()`);
    assert(during.label.includes("扫描中"), `扫描期间按钮应显示扫描中：${during.label}`);
    assert(during.spinning, "扫描期间应有 spinner");
    await page.waitFor("!document.querySelector('.refresh-icon.spinning')");
    const after = await page.evaluate("document.querySelector('.refresh-button span')?.textContent?.trim()");
    assertEqual(after, "刷新", "扫描结束后恢复「刷新」");
  });

  s.add("错误 banner 与「N 条数据源提示」保留（代码级）", async () => {
    const src = readFileSync(APP, "utf8");
    assert(src.includes('class="banner error"'), "错误 banner 保留");
    assert(src.includes("条数据源提示"), "数据源提示折叠条保留");
    assert(src.includes("scanNotices"), "数据源提示状态保留");
  });

  s.add("⌘R 仍触发手动刷新", async () => {
    await page.goto(URL);
    const during = await page.evaluate(`(async () => {
      window.dispatchEvent(new KeyboardEvent('keydown', {key: 'r', metaKey: true, bubbles: true}));
      await new Promise((resolve) => setTimeout(resolve, 30));
      return document.querySelector('.refresh-button span')?.textContent?.trim();
    })()`);
    assertEqual(during, "扫描中", "⌘R 触发扫描");
    await page.waitFor("!document.querySelector('.refresh-icon.spinning')");
  });

  const { pass, failures } = await suite.run(process.argv[2]);
  console.log(`\nS3 结果：${pass} 通过，${failures.length} 失败`);
  process.exitCode = failures.length === 0 ? 0 : 1;
}

export async function main() {
  const chrome = await launchChrome();
  const page = await Page.connect(chrome.port);
  await page.setViewport(1200, 780);
  try {
    await run(page);
  } finally {
    chrome.proc.kill();
  }
}

await main();
