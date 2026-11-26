// S2：三组选中样式——柔色底 / 加粗 / 墨色文字与计数 / 无色条 / 对比度 / aria-pressed
import { runSuite, Suite, assert, assertEqual, contrast, EFFECTIVE_BG_HELPER } from "./harness.mjs";

const URL = "http://localhost:1421/";
const WS3 = "/Users/demo/code/scratchpad";

const LIGHT = { bg: "rgba(185, 106, 53, 0.12)", ink: "rgb(143, 79, 36)" };
const DARK = { bg: "rgba(224, 150, 92, 0.16)", ink: "rgb(224, 150, 92)" };

const q = (s) => JSON.stringify(s);

const clickNav = (page, label) =>
  page.evaluate(`[...document.querySelectorAll('.nav-item')].find(b => b.querySelector('.nav-label')?.textContent?.trim() === ${q(label)})?.click()`);
const clickWs = (page, path) =>
  page.evaluate(`[...document.querySelectorAll('.nav-item')].find(b => b.getAttribute('title') === ${q(path)})?.click()`);
const clickSource = (page, label) =>
  page.evaluate(`[...document.querySelectorAll('.source-item')].find(b => b.getAttribute('aria-label')?.startsWith(${q(label)}))?.click()`);

// 读取选中项的样式事实：背景 / 标签字重与颜色 / 计数颜色 / ::before / 有效底色
const activeStyleExpr = (finder) => `(() => {
  ${EFFECTIVE_BG_HELPER}
  const el = ${finder};
  if (!el) return null;
  const cs = getComputedStyle(el);
  const label = el.querySelector('.nav-label');
  const count = el.querySelector('.nav-count');
  return {
    bg: cs.backgroundColor,
    labelWeight: label ? Number(getComputedStyle(label).fontWeight) : null,
    labelColor: label ? getComputedStyle(label).color : null,
    countColor: count ? getComputedStyle(count).color : null,
    barContent: getComputedStyle(el, '::before').content,
    ariaPressed: el.getAttribute('aria-pressed'),
    effectiveBg: effectiveBg(el),
  };
})()`;

const navFinder = (label) =>
  `[...document.querySelectorAll('.nav-item')].find(b => b.querySelector('.nav-label')?.textContent?.trim() === ${q(label)} && b.classList.contains('active'))`;
const wsFinder = (path) =>
  `[...document.querySelectorAll('.nav-item')].find(b => b.getAttribute('title') === ${q(path)} && b.classList.contains('active'))`;
const srcFinder = (label) =>
  `[...document.querySelectorAll('.source-item')].find(b => b.getAttribute('aria-label')?.startsWith(${q(label)}) && b.classList.contains('active'))`;

const navInactiveFinder = (label) =>
  `[...document.querySelectorAll('.nav-item[aria-pressed="false"]')].find(b => b.querySelector('.nav-label')?.textContent?.trim() === ${q(label)})`;
const wsInactiveFinder = () =>
  `[...document.querySelectorAll('.nav-item:not(.search-entry)[aria-pressed="false"][title]')].find(b => b.getAttribute('title'))`;
const srcInactiveFinder = () =>
  `document.querySelector('.source-item[aria-pressed="false"]')`;

function assertSoftActive(style, expected, context) {
  assert(style, `${context}：存在选中项`);
  assertEqual(style.bg, expected.bg, `${context}：选中态为柔色底`);
  assert(style.labelWeight >= 600, `${context}：标签加粗（实际 ${style.labelWeight}）`);
  assertEqual(style.labelColor, expected.ink, `${context}：标签为选中墨色`);
  assertEqual(style.countColor, expected.ink, `${context}：计数与标签同墨色`);
  assertEqual(style.barContent, "none", `${context}：无左侧色条`);
  assertEqual(style.ariaPressed, "true", `${context}：保留 aria-pressed`);
  const labelContrast = contrast(style.labelColor, style.effectiveBg);
  const countContrast = contrast(style.countColor, style.effectiveBg);
  assert(labelContrast >= 4.5, `${context}：标签对比度 ≥ 4.5（实际 ${labelContrast.toFixed(2)}）`);
  assert(countContrast >= 4.5, `${context}：计数对比度 ≥ 4.5（实际 ${countContrast.toFixed(2)}）`);
  return style;
}

function assertInactive(style, context) {
  assert(style, `${context}：存在未选中项`);
  assertEqual(style.ariaPressed, "false", `${context}：未选中 aria-pressed 为 false`);
  assertEqual(style.bg, "rgba(0, 0, 0, 0)", `${context}：未选中无选中底色`);
  assert(style.labelWeight < 600, `${context}：未选中标签不加粗（实际 ${style.labelWeight}）`);
  return style;
}

const suite = new Suite("S2 选中样式");

async function run(page) {
  const s = suite;

  s.add("浅色-任务组选中：柔底/加粗/墨色/无色条/对比度", async () => {
    await page.goto(URL);
    const style = await page.evaluate(activeStyleExpr(navFinder("最近")));
    assertSoftActive(style, LIGHT, "浅色任务组");
  });

  s.add("浅色-workspace 选中：同规格", async () => {
    await page.goto(URL);
    await clickWs(page, WS3);
    const style = await page.evaluate(activeStyleExpr(wsFinder(WS3)));
    assertSoftActive(style, LIGHT, "浅色 workspace");
    const taskStyle = await page.evaluate(activeStyleExpr(navFinder("最近")));
    assertEqual(style.labelColor, taskStyle.labelColor, "浅色 workspace 墨色与任务组一致");
  });

  s.add("浅色-来源选中：同规格（不用品牌色）", async () => {
    await page.goto(URL);
    await clickSource(page, "Codex");
    const style = await page.evaluate(activeStyleExpr(srcFinder("Codex")));
    assertSoftActive(style, LIGHT, "浅色来源");
    const taskStyle = await page.evaluate(activeStyleExpr(navFinder("最近")));
    assertEqual(style.labelColor, taskStyle.labelColor, "来源组选中墨色与任务组一致");
  });

  s.add("浅色-三组未选中：aria-pressed=false 且无底色", async () => {
    await page.goto(URL);
    await clickWs(page, WS3);
    await clickSource(page, "Codex");
    assertInactive(await page.evaluate(activeStyleExpr(navInactiveFinder("更早"))), "浅色任务组未选中");
    assertInactive(await page.evaluate(activeStyleExpr(wsInactiveFinder())), "浅色 workspace 未选中");
    assertInactive(await page.evaluate(activeStyleExpr(srcInactiveFinder())), "浅色来源未选中");
  });

  s.add("深色-三组选中：柔底/加粗/墨色/无色条/对比度达标", async () => {
    await page.setDark(true);
    try {
      await page.goto(URL);
      await clickWs(page, WS3);
      await clickSource(page, "Codex");
      const taskStyle = assertSoftActive(
        await page.evaluate(activeStyleExpr(navFinder("最近"))),
        DARK,
        "深色任务组",
      );
      assertSoftActive(await page.evaluate(activeStyleExpr(wsFinder(WS3))), DARK, "深色 workspace");
      const srcStyle = assertSoftActive(
        await page.evaluate(activeStyleExpr(srcFinder("Codex"))),
        DARK,
        "深色来源",
      );
      assertEqual(srcStyle.labelColor, taskStyle.labelColor, "深色来源组选中墨色与任务组一致");
    } finally {
      await page.setDark(false);
    }
  });

  s.add("深色-三组未选中：aria-pressed=false 且无底色", async () => {
    await page.setDark(true);
    try {
      await page.goto(URL);
      await clickWs(page, WS3);
      await clickSource(page, "Codex");
      assertInactive(await page.evaluate(activeStyleExpr(navInactiveFinder("更早"))), "深色任务组未选中");
      assertInactive(await page.evaluate(activeStyleExpr(wsInactiveFinder())), "深色 workspace 未选中");
      assertInactive(await page.evaluate(activeStyleExpr(srcInactiveFinder())), "深色来源未选中");
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
