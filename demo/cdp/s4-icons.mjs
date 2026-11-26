// S4：图标统一——品牌位 / 详情空态 / demo 社交预览全部使用 app icon 图形，线描鸟移除
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { launchChrome, Page, Suite, assert, assertEqual } from "./harness.mjs";

const URL = "http://localhost:1421/";
// 验收不依赖当前工作目录：所有路径相对套件文件解析到仓库根。
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const APP = join(ROOT, "src/App.svelte");
const SOCIAL = join(ROOT, "demo/social.html");
const ICON = join(ROOT, "src-tauri/icons/icon.svg");

// app icon 的可识别特征
const SIGNATURES = [
  { name: "渐变起点色", value: "#e2a56f" },
  { name: "渐变终点色", value: "#a35626" },
  { name: "插画鸟身体", value: 'cx="512" cy="566"' },
  { name: "栖木横杆", value: 'x="188" y="706"' },
];
const LINE_BIRD_PATH = "M7 20c0-5.5";

export const suite = new Suite("S4 图标统一");

export async function run(page) {
  const s = suite;

  const iconSvg = readFileSync(ICON, "utf8");
  for (const sig of SIGNATURES) {
    assert(iconSvg.includes(sig.value), `app icon 本体应含 ${sig.name}`);
  }

  s.add("侧栏品牌位使用 app icon 图形", async () => {
    await page.goto(URL);
    const brand = await page.evaluate(`(() => {
      const mark = document.querySelector('.brand-mark');
      if (!mark) return null;
      const svg = mark.querySelector('svg');
      return {
        html: mark.innerHTML,
        width: svg?.getBoundingClientRect().width,
        bg: getComputedStyle(mark).backgroundColor,
        bgImage: getComputedStyle(mark).backgroundImage,
      };
    })()`);
    assert(brand, "品牌位存在");
    for (const sig of SIGNATURES) {
      assert(brand.html.includes(sig.value), `品牌位应含 ${sig.name}`);
    }
    assertEqual(brand.bg, "rgba(0, 0, 0, 0)", "品牌位不再有渐变底色（由图形自身提供）");
    assertEqual(brand.bgImage, "none", "品牌位不再有渐变背景图");
    assert(brand.width > 20 && brand.width <= 34, `品牌位约 30px（实际 ${brand.width}）`);
  });

  s.add("详情空态使用 app icon 图形", async () => {
    await page.goto(URL);
    const empty = await page.evaluate(`(() => {
      const box = document.querySelector('.inspector-empty');
      if (!box) return null;
      return { html: box.querySelector('svg')?.outerHTML ?? "" };
    })()`);
    assert(empty, "详情空态存在（未选中任务）");
    for (const sig of SIGNATURES) {
      assert(empty.html.includes(sig.value), `空态应含 ${sig.name}`);
    }
  });

  s.add("线描鸟图标全应用移除", async () => {
    await page.goto(URL);
    const count = await page.evaluate(
      `document.querySelectorAll('path[d^="${LINE_BIRD_PATH}"]').length`,
    );
    assertEqual(count, 0, "线描鸟 path 数量");
    const appSrc = readFileSync(APP, "utf8");
    assert(!appSrc.includes(LINE_BIRD_PATH), "App.svelte 不含线描鸟");
    assert(!appSrc.includes("bird:"), "ICONS.bird 已删除");
    assert(!appSrc.includes("ICONS.bird"), "无 ICONS.bird 引用");
  });

  s.add("demo 社交预览使用 app icon 图形", async () => {
    const social = readFileSync(SOCIAL, "utf8");
    for (const sig of SIGNATURES) {
      assert(social.includes(sig.value), `社交预览应含 ${sig.name}`);
    }
    assert(!social.includes(LINE_BIRD_PATH), "社交预览不含线描鸟");
    assert(!social.includes("linear-gradient(135deg, #e0965c, #b96a35)"), "社交预览 .mark 不再自带渐变底");
  });

  s.add("品牌位在 24px 下仍可辨识（矢量缩放）", async () => {
    await page.setViewport(900, 560);
    try {
      await page.goto(URL);
      const box = await page.evaluate(
        `document.querySelector('.brand-mark svg')?.getBoundingClientRect().width`,
      );
      assert(box > 15 && box <= 34, `小窗口下品牌图形仍渲染（实际 ${box}）`);
    } finally {
      await page.setViewport(1200, 780);
    }
  });

  const { pass, failures } = await suite.run(process.argv[2]);
  console.log(`\nS4 结果：${pass} 通过，${failures.length} 失败`);
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
