// CDP 验收驱动：连接 headless Chrome，驱动 demo（端口 1421，合成数据）。
// 统一入口 npm run cdp（自动起 demo 与浏览器）；单独运行某脚本需先 npm run demo。
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const CHROME =
  process.env.CHROME_BIN ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

export async function launchChrome() {
  const dir = mkdtempSync(join(tmpdir(), "perch-cdp-"));
  // 套件进程结束时删掉本次的临时 profile，避免每跑一轮就在 /tmp 留一份。
  process.on("exit", () => {
    try {
      rmSync(dir, { recursive: true, force: true });
    } catch {
      // profile 已被清理或仍被占用，留给系统临时目录
    }
  });
  const proc = spawn(
    CHROME,
    [
      "--headless=new",
      "--remote-debugging-port=0",
      `--user-data-dir=${dir}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-gpu",
      "about:blank",
    ],
    { stdio: "ignore" },
  );
  const port = await waitForPortFile(dir);
  return { proc, port, dir };
}

async function waitForPortFile(dir, timeoutMs = 15_000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    try {
      const content = readFileSync(join(dir, "DevToolsActivePort"), "utf8");
      const port = Number(content.split("\n")[0]);
      if (port > 0) return port;
    } catch {
      // file not ready yet
    }
    await sleep(100);
  }
  throw new Error("Chrome DevToolsActivePort 未出现");
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export class Page {
  constructor(ws) {
    this.ws = ws;
    this.seq = 0;
    this.pending = new Map();
    ws.addEventListener("message", (event) => {
      const msg = JSON.parse(event.data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(`${msg.error.message} (${msg.error.data ?? ""})`));
        else resolve(msg.result);
      }
    });
  }

  static async connect(port) {
    const list = await fetch(`http://127.0.0.1:${port}/json/list`).then((r) => r.json());
    const target = list.find((t) => t.type === "page");
    if (!target) throw new Error("没有 page target");
    const ws = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve, { once: true });
      ws.addEventListener("error", reject, { once: true });
    });
    return new Page(ws);
  }

  send(method, params = {}) {
    const id = ++this.seq;
    this.ws.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new Error(`CDP 超时：${method}`));
        }
      }, 20_000);
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
    });
  }

  async evaluate(expression) {
    const result = await this.send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (result.exceptionDetails) {
      const detail = result.exceptionDetails.exception?.description ?? JSON.stringify(result.exceptionDetails);
      throw new Error(`页面内异常：${detail}`);
    }
    return result.result.value;
  }

  async goto(url) {
    await this.send("Page.enable");
    await this.send("Page.navigate", { url });
    // 侧栏在 payload 到达前就会渲染，等待列表真正出现、挂载时的自动刷新结束
    await this.waitFor("document.querySelectorAll('.flat-list .task').length > 0");
    await this.waitFor("!document.querySelector('.refresh-icon.spinning')");
  }

  async waitFor(expression, timeoutMs = 8_000) {
    const started = Date.now();
    let last;
    while (Date.now() - started < timeoutMs) {
      last = await this.evaluate(expression);
      if (last) return last;
      await sleep(60);
    }
    throw new Error(`等待超时：${expression}（最后值 ${JSON.stringify(last)}）`);
  }

  async setViewport(width, height) {
    await this.send("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor: 1,
      mobile: false,
    });
  }

  async setDark(enabled) {
    await this.send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: enabled ? "dark" : "light" }],
    });
  }
}

// ---- 断言运行器 ----

export class Suite {
  constructor(name) {
    this.name = name;
    this.cases = [];
  }

  add(title, fn) {
    this.cases.push({ title, fn });
  }

  async run(filter) {
    let pass = 0;
    const failures = [];
    for (const { title, fn } of this.cases) {
      if (filter && !title.includes(filter)) continue;
      try {
        await fn();
        pass += 1;
        console.log(`  ✓ ${title}`);
      } catch (error) {
        failures.push({ title, error });
        console.log(`  ✗ ${title}\n      ${error.message.split("\n")[0]}`);
      }
    }
    return { pass, failures };
  }
}

// 套件统一引导：起 Chrome → 连页面 →（可选）设视口 → 跑用例 → 关 Chrome → 汇总退出码。
// 套件文件只登记用例，末尾一行 `await runSuite("S4 结果", suite, run, { viewport: [1200, 780] });`。
export async function runSuite(label, suite, run, { viewport } = {}) {
  const chrome = await launchChrome();
  const page = await Page.connect(chrome.port);
  if (viewport) await page.setViewport(...viewport);
  try {
    await run(page);
    const { pass, failures } = await suite.run(process.argv[2]);
    console.log(`\n${label}：${pass} 通过，${failures.length} 失败`);
    process.exitCode = failures.length === 0 ? 0 : 1;
  } finally {
    chrome.proc.kill();
  }
}

export function assert(condition, message) {
  if (!condition) throw new Error(message ?? "断言失败");
}

function deepEqual(a, b) {
  if (a === b) return true;
  if (typeof a !== typeof b || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) {
    return a.length === b.length && a.every((v, i) => deepEqual(v, b[i]));
  }
  if (typeof a === "object") {
    const ka = Object.keys(a).sort();
    const kb = Object.keys(b).sort();
    return (
      ka.length === kb.length &&
      ka.every((k) => deepEqual(a[k], b[k]))
    );
  }
  return false;
}

export function assertEqual(actual, expected, message) {
  if (!deepEqual(actual, expected)) {
    throw new Error(
      `${message ?? "值不相等"}：期望 ${JSON.stringify(expected)}，实际 ${JSON.stringify(actual)}`,
    );
  }
}

// WCAG 对比度
export function contrast(fg, bg) {
  const lum = (cssColor) => {
    const m = cssColor.match(/rgba?\(([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:[,\s/]+([\d.]+%?))?/);
    if (!m) throw new Error(`无法解析颜色：${cssColor}`);
    const alpha = m[4] === undefined ? 1 : m[4].endsWith("%") ? Number(m[4]) / 100 : Number(m[4]);
    const base = [m[1], m[2], m[3]].map((v) => {
      const c = Number(v) / 255;
      return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    });
    const l = 0.2126 * base[0] + 0.7152 * base[1] + 0.0722 * base[2];
    return { l, alpha };
  };
  const front = lum(fg);
  const back = lum(bg);
  // 前景透明时按背景合成
  const mix = front.alpha < 1 ? front.l * front.alpha + back.l * (1 - front.alpha) : front.l;
  const lighter = Math.max(mix, back.l);
  const darker = Math.min(mix, back.l);
  return (lighter + 0.05) / (darker + 0.05);
}
