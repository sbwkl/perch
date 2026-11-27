// U1：列表状态列对齐、继续工作按钮统一样式、Inspector 删除用量块与只读脚注
import { runSuite, Suite, assert, assertEqual, contrast, EFFECTIVE_BG_HELPER } from "./harness.mjs";

const URL = "http://localhost:1421/?transcript=1";

const rowExpr = (label) => `[...document.querySelectorAll('.flat-list .task-row')].find(r =>
  r.querySelector('.task-meta')?.textContent?.includes(${JSON.stringify(label)})) ?? null`;

const selectTask = (page, label) => page.evaluate(`(() => {
  const row = ${rowExpr(label)};
  row?.querySelector('.task')?.click();
})()`);

const suite = new Suite("U1 列表与详情体验修复");

async function run(page) {
  const s = suite;

  s.add("U1 Claude Code 与 Trae CN 状态胶囊右边界对齐", async () => {
    await page.goto(URL);
    const gaps = await page.evaluate(`(() => {
      const rows = [...document.querySelectorAll('.flat-list .task-row')];
      const right = (label) => {
        const row = rows.find(r => r.querySelector('.task-meta')?.textContent?.includes(label));
        return row?.querySelector('.status')?.getBoundingClientRect().right ?? null;
      };
      const claude = right("Claude Code");
      const trae = right("Trae CN");
      const codebuddy = right("CodeBuddy");
      return {
        claude,
        trae,
        codebuddy,
        iconClaude: Boolean(rows.find(r => r.querySelector('.task-meta')?.textContent?.includes("Claude Code"))?.querySelector('.task-transcript')),
        iconTrae: Boolean(rows.find(r => r.querySelector('.task-meta')?.textContent?.includes("Trae CN"))?.querySelector('.task-transcript')),
      };
    })()`);
    assert(gaps.claude !== null && gaps.trae !== null && gaps.codebuddy !== null, "三类任务行都存在状态胶囊");
    assert(gaps.iconClaude, "Claude Code 仍有对话图标");
    assertEqual(gaps.iconTrae, false, "Trae CN 仍无对话图标");
    assert(Math.abs(gaps.claude - gaps.trae) <= 1, `Claude Code / Trae CN 右边界对齐（偏差 ${(gaps.claude - gaps.trae).toFixed(1)}px）`);
    assert(Math.abs(gaps.claude - gaps.codebuddy) <= 1, `Claude Code / CodeBuddy 右边界对齐（偏差 ${(gaps.claude - gaps.codebuddy).toFixed(1)}px）`);
  });

  s.add("U1 继续工作按钮不同来源同款统一样式", async () => {
    await page.goto(URL);
    const styles = [];
    for (const label of ["Claude Code", "Trae CN", "Codex"]) {
      await selectTask(page, label);
      const style = await page.evaluate(`(() => {
        const button = document.querySelector('.inspector .continue-button');
        if (!button) return null;
        const cs = getComputedStyle(button);
        return {
          text: button.textContent.trim(),
          inlineStyle: button.getAttribute('style'),
          background: cs.backgroundColor,
          color: cs.color,
          boxShadow: cs.boxShadow,
          borderRadius: cs.borderRadius,
          fontSize: cs.fontSize,
          fontWeight: cs.fontWeight,
        };
      })()`);
      assert(style, `${label} 有继续工作按钮`);
      styles.push(style);
    }
    assert(styles[0].text.startsWith("激活 "), "Claude Code 文案保持激活");
    assert(styles[1].text.startsWith("激活 "), "Trae CN 文案保持激活");
    assert(styles[2].text.startsWith("打开 "), "Codex 文案保持打开");
    for (const key of ["inlineStyle", "background", "color", "boxShadow", "borderRadius", "fontSize", "fontWeight"]) {
      assertEqual(styles[1][key], styles[0][key], `Trae CN 与 Claude Code ${key} 一致`);
      assertEqual(styles[2][key], styles[0][key], `Codex 与 Claude Code ${key} 一致`);
    }
    assertEqual(styles[0].inlineStyle, null, "按钮无来源色内联样式");
    assertEqual(styles[0].boxShadow, "none", "按钮无品牌色描边");
  });

  s.add("U1 继续工作按钮浅色 / 深色主题均满足 WCAG AA", async () => {
    for (const dark of [false, true]) {
      await page.setDark(dark);
      try {
        await page.goto(URL);
        await selectTask(page, "Claude Code");
        const colors = await page.evaluate(`(() => {
          ${EFFECTIVE_BG_HELPER}
          const button = document.querySelector('.inspector .continue-button');
          if (!button) return null;
          const cs = getComputedStyle(button);
          return {
            foreground: cs.color,
            background: effectiveBg(button),
          };
        })()`);
        assert(colors, `${dark ? "深色" : "浅色"}主题有继续工作按钮`);
        const ratio = contrast(colors.foreground, colors.background);
        assert(
          ratio >= 4.5,
          `${dark ? "深色" : "浅色"}主题按钮对比度 ≥ 4.5（实际 ${ratio.toFixed(2)}:1）`,
        );
      } finally {
        await page.setDark(false);
      }
    }
  });

  s.add("U1 Inspector 无用量块与只读脚注", async () => {
    await page.goto(URL);
    await selectTask(page, "Claude Code");
    const text = await page.evaluate(`(() => {
      const inspector = document.querySelector('.inspector');
      return {
        text: inspector?.textContent ?? "",
        usageBlock: Boolean(inspector?.querySelector('.usage-block')),
        footer: Boolean(inspector?.querySelector('.inspector-foot')),
      };
    })()`);
    assertEqual(text.usageBlock, false, "无 usage-block");
    assertEqual(text.footer, false, "无 inspector-foot");
    assert(!text.text.includes("usage-ledger 中没有该会话的用量记录"), "无用量空态说明");
    assert(!text.text.includes("四轨口径不同"), "无四轨口径说明");
    assert(!text.text.includes("不会修改任何 agent 数据"), "无只读脚注");
  });
}

await runSuite("U1 结果", suite, run, { viewport: [1200, 780] });
