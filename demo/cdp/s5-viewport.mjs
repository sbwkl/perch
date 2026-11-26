// S5：900×560 无横向溢出 / 侧栏可滚动 / 页脚可达
import { runSuite, Suite, assert, assertEqual } from "./harness.mjs";

const URL = "http://localhost:1421/";

const suite = new Suite("S5 小窗口布局");

async function run(page) {
  const s = suite;

  s.add("900×560 默认数据：无横向溢出，页脚报告图标可达", async () => {
    await page.setViewport(900, 560);
    try {
      await page.goto(URL);
      const layout = await page.evaluate(`(() => {
        const doc = document.documentElement;
        const foot = document.querySelector('.sidebar-report');
        const rect = foot?.getBoundingClientRect();
        const hit = rect ? document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2) : null;
        return {
          overflowX: Math.max(doc.scrollWidth, document.body.scrollWidth) > 900,
          footVisible: Boolean(rect && rect.bottom <= innerHeight && rect.top >= 0),
          footClickable: Boolean(hit && (hit === foot || foot.contains(hit))),
        };
      })()`);
      assertEqual(layout.overflowX, false, "无横向溢出");
      assertEqual(layout.footVisible, true, "页脚报告图标在视口内");
      assertEqual(layout.footClickable, true, "页脚报告图标可点击");
    } finally {
      await page.setViewport(1200, 780);
    }
  });

  s.add("900×560 + stress=520：侧栏可滚动、页脚可点、无横向溢出", async () => {
    await page.setViewport(900, 560);
    try {
      await page.goto(`${URL}?stress=520`);
      const layout = await page.evaluate(`(() => {
        const doc = document.documentElement;
        const scroll = document.querySelector('.sidebar-scroll');
        const foot = document.querySelector('.sidebar-report');
        const rect = foot?.getBoundingClientRect();
        const hit = rect ? document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2) : null;
        return {
          overflowX: Math.max(doc.scrollWidth, document.body.scrollWidth) > 900,
          scrollable: scroll ? scroll.scrollHeight > scroll.clientHeight : false,
          footClickable: Boolean(hit && (hit === foot || foot.contains(hit))),
        };
      })()`);
      assertEqual(layout.overflowX, false, "无横向溢出");
      assertEqual(layout.scrollable, true, "侧栏内部可滚动");
      assertEqual(layout.footClickable, true, "页脚报告图标可点击");
    } finally {
      await page.setViewport(1200, 780);
    }
  });

  s.add("900×560 三组全选中 + 报告视图：无横向溢出", async () => {
    await page.setViewport(900, 560);
    try {
      await page.goto(URL);
      await page.evaluate(`(() => {
        const ws = [...document.querySelectorAll('.nav-item')].find(b => b.getAttribute('title') === '/Users/demo/code/acme-storefront');
        ws?.click();
        const src = [...document.querySelectorAll('.source-item')].find(b => b.getAttribute('aria-label')?.startsWith('Codex'));
        src?.click();
      })()`);
      const layout = await page.evaluate(`(() => {
        const doc = document.documentElement;
        return Math.max(doc.scrollWidth, document.body.scrollWidth) > 900;
      })()`);
      assertEqual(layout, false, "三组选中下无横向溢出");
      await page.evaluate(`document.querySelector('.sidebar-report')?.click()`);
      await page.waitFor("Boolean(document.querySelector('.report-frame'))");
      const reportOverflow = await page.evaluate(
        `Math.max(document.documentElement.scrollWidth, document.body.scrollWidth) > 900`,
      );
      assertEqual(reportOverflow, false, "报告视图无横向溢出");
    } finally {
      await page.setViewport(1200, 780);
    }
  });

}

await runSuite("S5 结果", suite, run, { viewport: [900, 560] });

