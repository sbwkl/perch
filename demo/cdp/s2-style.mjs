// S2：三组选中样式——实底 / 加粗 / 反白计数 / 左侧色条 / 对比度 / aria-pressed
import { runSuite, Suite, assert, assertEqual, contrast } from "./harness.mjs";

const URL = "http://localhost:1421/";
const WS3 = "/Users/demo/code/scratchpad";
const CODEX_RGB = "rgb(255, 255, 255)";

const q = (s) => JSON.stringify(s);

const clickNav = (page, label) =>
  page.evaluate(`[...document.querySelectorAll('.nav-item')].find(b => b.querySelector('.nav-label')?.textContent?.trim() === ${q(label)})?.click()`);
const clickWs = (page, path) =>
  page.evaluate(`[...document.querySelectorAll('.nav-item')].find(b => b.getAttribute('title') === ${q(path)})?.click()`);
const clickSource = (page, label) =>
  page.evaluate(`[...document.querySelectorAll('.source-item')].find(b => b.getAttribute('aria-label')?.startsWith(${q(label)}))?.click()`);

// 读取选中项的样式事实：背景 / 标签字重与颜色 / 计数颜色 / 左侧色条
const activeStyleExpr = (finder) => `(() => {
  const el = ${finder};
  if (!el) return null;
  const cs = getComputedStyle(el);
  const bar = getComputedStyle(el, '::before');
  const label = el.querySelector('.nav-label');
  const count = el.querySelector('.nav-count');
  return {
    bg: cs.backgroundColor,
    color: cs.color,
    labelWeight: label ? Number(getComputedStyle(label).fontWeight) : null,
    labelColor: label ? getComputedStyle(label).color : null,
    countColor: count ? getComputedStyle(count).color : null,
    barWidth: bar.width,
    barColor: bar.backgroundColor,
    position: cs.position,
    ariaPressed: el.getAttribute('aria-pressed'),
  };
})()`;

const navFinder = (label) =>
  `[...document.querySelectorAll('.nav-item')].find(b => b.querySelector('.nav-label')?.textContent?.trim() === ${q(label)} && b.classList.contains('active'))`;
const wsFinder = (path) =>
  `[...document.querySelectorAll('.nav-item')].find(b => b.getAttribute('title') === ${q(path)} && b.classList.contains('active'))`;
const srcFinder = (label) =>
  `[...document.querySelectorAll('.source-item')].find(b => b.getAttribute('aria-label')?.startsWith(${q(label)}) && b.classList.contains('active'))`;

function assertSolidActive(style, expectedInk, context) {
  assert(style, `${context}：存在选中项`);
  const alphaMatch = style.bg.match(/rgba?\(([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:[,\s/]+\s*([\d.]+))?\)/);
  assert(alphaMatch, `${context}：可解析背景色 ${style.bg}`);
  const alpha = alphaMatch[4] === undefined ? 1 : Number(alphaMatch[4]);
  assertEqual(alpha, 1, `${context}：选中态为实底（不透明）`);
  assert(style.labelWeight >= 600, `${context}：标签加粗（实际 ${style.labelWeight}）`);
  assertEqual(style.countColor, expectedInk, `${context}：计数反白`);
  const barW = Number.parseFloat(style.barWidth);
  assert(barW >= 3 && barW <= 4, `${context}：左侧色条宽 3–4px（实际 ${style.barWidth}）`);
  const barAlpha = style.barColor.includes("rgba") ? Number(style.barColor.split(",")[3]?.replace(")", "")) : 1;
  assert(barAlpha > 0, `${context}：色条非透明`);
  assert(style.position === "relative", `${context}：色条定位基准`);
  assertEqual(style.ariaPressed, "true", `${context}：保留 aria-pressed`);
  const labelContrast = contrast(style.labelColor, style.bg);
  const countContrast = contrast(style.countColor, style.bg);
  assert(labelContrast >= 4.5, `${context}：标签对比度 ≥ 4.5（实际 ${labelContrast.toFixed(2)}）`);
  assert(countContrast >= 4.5, `${context}：计数对比度 ≥ 4.5（实际 ${countContrast.toFixed(2)}）`);
  return style;
}

const suite = new Suite("S2 选中样式");

async function run(page) {
  const s = suite;

  s.add("浅色-任务组选中：实底/加粗/反白计数/色条/对比度", async () => {
    await page.goto(URL);
    const style = await page.evaluate(activeStyleExpr(navFinder("最近")));
    assertSolidActive(style, "rgb(255, 255, 255)", "浅色任务组");
  });

  s.add("浅色-workspace 选中：同规格 + 中性色条", async () => {
    await page.goto(URL);
    await clickWs(page, WS3);
    const style = await page.evaluate(activeStyleExpr(wsFinder(WS3)));
    assertSolidActive(style, "rgb(255, 255, 255)", "浅色 workspace");
  });

  s.add("浅色-来源选中：色条取品牌色（Codex）", async () => {
    await page.goto(URL);
    await clickSource(page, "Codex");
    const style = await page.evaluate(activeStyleExpr(srcFinder("Codex")));
    assertSolidActive(style, "rgb(255, 255, 255)", "浅色来源");
    assertEqual(style.barColor, CODEX_RGB, "来源色条颜色");
  });

  s.add("浅色-来源选中：色条取品牌色（Qoder）", async () => {
    await page.goto(URL);
    await clickSource(page, "Qoder");
    const style = await page.evaluate(activeStyleExpr(srcFinder("Qoder")));
    assertSolidActive(style, "rgb(255, 255, 255)", "浅色来源");
    assertEqual(style.barColor, "rgb(42, 219, 92)", "Qoder 品牌色色条");
  });

  s.add("深色-三组选中：实底/加粗/反白计数/对比度达标", async () => {
    await page.setDark(true);
    try {
      await page.goto(URL);
      await clickWs(page, WS3);
      await clickSource(page, "Codex");
      assertSolidActive(
        await page.evaluate(activeStyleExpr(navFinder("最近"))),
        "rgb(29, 29, 31)",
        "深色任务组",
      );
      assertSolidActive(
        await page.evaluate(activeStyleExpr(wsFinder(WS3))),
        "rgb(29, 29, 31)",
        "深色 workspace",
      );
      const srcStyle = assertSolidActive(
        await page.evaluate(activeStyleExpr(srcFinder("Codex"))),
        "rgb(29, 29, 31)",
        "深色来源",
      );
      assertEqual(srcStyle.barColor, CODEX_RGB, "深色来源色条颜色");
    } finally {
      await page.setDark(false);
    }
  });

  s.add("分组标题不变：三组标题文案原样", async () => {
    await page.goto(URL);
    const titles = await page.evaluate(
      `[...document.querySelectorAll('.section-title')].map(t => t.textContent.trim())`,
    );
    assertEqual(titles, ["任务", "Workspaces", "来源"], "分组标题");
  });

}

await runSuite("S2 结果", suite, run, { viewport: [1200, 780] });

