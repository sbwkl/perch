// 统一入口：npm run cdp —— 确保 demo 服务就绪，顺序执行全部验收套件。
// 依赖：Node 22（内置 fetch / WebSocket）、headless Chrome（CHROME_BIN 可覆盖）。
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const BASE = "http://localhost:1421/";
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

async function reachable() {
  try {
    return (await fetch(BASE)).ok;
  } catch {
    return false;
  }
}

async function ensureDemo() {
  if (await reachable()) return null;
  const proc = spawn("npm", ["run", "demo"], { stdio: "ignore", detached: true });
  proc.unref();
  const started = Date.now();
  while (Date.now() - started < 30_000) {
    if (await reachable()) return proc;
    await sleep(200);
  }
  throw new Error("demo 服务未在 30s 内就绪（npm run demo，端口 1421）");
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

const demo = await ensureDemo();
let failed = 0;
for (const suite of SUITES) {
  console.log(`\n== ${suite}`);
  const result = spawnSync(process.execPath, [join(here, suite)], { stdio: "inherit" });
  if (result.status !== 0) failed += 1;
}
if (demo) {
  try {
    process.kill(-demo.pid, "SIGTERM");
  } catch {
    // 进程已退出
  }
}
console.log(
  failed === 0
    ? `\n全部 ${SUITES.length} 个套件通过`
    : `\n${failed}/${SUITES.length} 个套件失败`,
);
process.exit(failed === 0 ? 0 : 1);