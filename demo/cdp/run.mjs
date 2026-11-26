// 统一入口：npm run cdp —— 确保 demo 服务就绪，顺序执行全部验收套件。
// 依赖：Node 22（内置 fetch / WebSocket）、headless Chrome（CHROME_BIN 可覆盖）。
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const BASE = "http://localhost:1421/";
const DEMO_MARKER = "Perch · Demo";
const SUITE_TIMEOUT_MS = 180_000;
const SUITES = [
  "s1-selection.mjs",
  "s2-style.mjs",
  "s3-status-header.mjs",
  "s4-icons.mjs",
  "s5-viewport.mjs",
  "t1-entry.mjs",
  "t2-content.mjs",
  "t3-visual.mjs",
  "t8-filter-close.mjs",
  "t8-subsession-keep.mjs",
];

// 本轮所有套件的 Chrome profile 都落在这个目录下：套件被 SIGKILL 时退出钩子跑不到，
// 由这里在收尾时整目录回收。
const tmpRoot = mkdtempSync(join(tmpdir(), "perch-cdp-run-"));

// 1421 的三种状态：demo 就绪 / 端口被别的进程占着 / 没有服务。
async function demoState() {
  try {
    const response = await fetch(BASE);
    if (!response.ok) return "foreign";
    return (await response.text()).includes(DEMO_MARKER) ? "ready" : "foreign";
  } catch {
    return "down";
  }
}

async function ensureDemo() {
  const state = await demoState();
  if (state === "ready") return null;
  if (state === "foreign") {
    throw new Error("端口 1421 被其它进程占用（不是 Perch demo），请先释放端口");
  }
  const proc = spawn("npm", ["run", "demo"], { stdio: "ignore", detached: true });
  proc.unref();
  const started = Date.now();
  while (Date.now() - started < 30_000) {
    if ((await demoState()) === "ready") return proc;
    await sleep(200);
  }
  await stopDemo(proc);
  throw new Error("demo 服务未在 30s 内就绪（npm run demo，端口 1421）");
}

// 只收拾自己启动的 demo：SIGTERM 整个进程组，5s 内不退出再 SIGKILL，
// 避免遗留 vite 进程占着 1421 影响下一次运行。
async function stopDemo(proc) {
  // demo 可能已经自己退出；这时再按 pid 发信号会误伤复用的进程组。
  if (proc.exitCode !== null || proc.signalCode !== null) return;
  let exited = false;
  proc.on("exit", () => {
    exited = true;
  });
  try {
    process.kill(-proc.pid, "SIGTERM");
  } catch {
    return;
  }
  for (let i = 0; i < 25 && !exited; i += 1) await sleep(200);
  if (exited) return;
  try {
    process.kill(-proc.pid, "SIGKILL");
  } catch {
    // 进程已退出
  }
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// 每个套件跑在自己的进程组里：挂死时连同它起的 headless Chrome 一起 SIGKILL，
// 否则只杀套件进程会留下孤儿浏览器。
function runSuiteProcess(file) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [file], {
      stdio: "inherit",
      detached: true,
      env: { ...process.env, PERCH_CDP_TMP: tmpRoot },
    });
    const timer = setTimeout(() => {
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {
        // 进程组已退出
      }
    }, SUITE_TIMEOUT_MS);
    child.on("exit", (code, signal) => {
      clearTimeout(timer);
      resolve({ code, signal });
    });
    child.on("error", (error) => {
      clearTimeout(timer);
      console.error(`  套件启动失败：${error.message}`);
      resolve({ code: 1, signal: null });
    });
  });
}

const demo = await ensureDemo();
let failed = 0;
try {
  for (const suite of SUITES) {
    console.log(`\n== ${suite}`);
    const result = await runSuiteProcess(join(here, suite));
    if (result.code !== 0) {
      failed += 1;
      if (result.signal) console.log(`  （套件被 ${result.signal} 终止）`);
    }
  }
} finally {
  // 任何路径退出前都收拾自己启动的 demo，别把 1421 留给下一个进程；
  // 再回收本轮 profile（含被超时杀掉、来不及自清的套件目录）。
  if (demo) await stopDemo(demo);
  rmSync(tmpRoot, { recursive: true, force: true });
}
console.log(
  failed === 0
    ? `\n全部 ${SUITES.length} 个套件通过`
    : `\n${failed}/${SUITES.length} 个套件失败`,
);
process.exit(failed === 0 ? 0 : 1);