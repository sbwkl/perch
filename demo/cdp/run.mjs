// 统一入口：npm run cdp —— 确保 demo 服务就绪，顺序执行全部验收套件。
// 依赖：Node 22（内置 fetch / WebSocket）、headless Chrome（CHROME_BIN 可覆盖）。
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const BASE = "http://localhost:1421/";
const DEMO_MARKER = "Perch · Demo";
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

const demo = await ensureDemo();
let failed = 0;
try {
  for (const suite of SUITES) {
    console.log(`\n== ${suite}`);
    const result = spawnSync(process.execPath, [join(here, suite)], {
      stdio: "inherit",
      // 单套件挂死不该拖住整轮：超时按失败计。
      timeout: 180_000,
      killSignal: "SIGKILL",
    });
    if (result.status !== 0) {
      failed += 1;
      if (result.signal) console.log(`  （套件被 ${result.signal} 终止）`);
    }
  }
} finally {
  // 任何路径退出前都收拾自己启动的 demo，别把 1421 留给下一个进程。
  if (demo) await stopDemo(demo);
}
console.log(
  failed === 0
    ? `\n全部 ${SUITES.length} 个套件通过`
    : `\n${failed}/${SUITES.length} 个套件失败`,
);
process.exit(failed === 0 ? 0 : 1);