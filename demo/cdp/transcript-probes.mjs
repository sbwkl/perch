// T2 / T3 共用的对话视图探针与数值工具：两个套件断言同一套几何与滚动口径，
// 共用一份实现，避免各自维护后悄悄漂移。

export const near = (a, b, tolerance = 2) => Math.abs(a - b) <= tolerance;
export const percent = (part, whole) => `${((part / whole) * 100).toFixed(1)}%`;

// 消息列已滚到底的统一口径：首开定位与 T5 基线必须用同一个阈值，
// 否则「定位最新」与「阅读位置保持」会各说各话。
export function atBottomExpr() {
  return `(() => {
    const el = document.querySelector('.transcript-pane .transcript-messages');
    return el && el.scrollHeight > el.clientHeight && el.scrollHeight - el.scrollTop - el.clientHeight < 8;
  })()`;
}

// 以消息滚动容器的内容盒为基准（而非消息元素自身），所以读数与当前滚动位置无关：
// T2 断言对话内容口径、T3 断言小窗口布局时都能直接复用同一份几何事实，无需各写一套。
export function bubbleProbeExpr() {
  return `(() => {
    const el = document.querySelector('.transcript-pane .transcript-messages');
    const cs = getComputedStyle(el);
    const padL = parseFloat(cs.paddingLeft);
    const padR = parseFloat(cs.paddingRight);
    const left = el.getBoundingClientRect().left + el.clientLeft + padL;
    const contentWidth = el.clientWidth - padL - padR;
    const rect = (m) => {
      const r = m.getBoundingClientRect();
      const style = getComputedStyle(m);
      return {
        left: r.left,
        right: r.right,
        width: r.width,
        textAlign: style.textAlign,
        background: style.backgroundColor,
      };
    };
    const pick = (role) => [...el.querySelectorAll('.transcript-msg')]
      .filter((m) => m.dataset.role === role)
      .map(rect);
    return {
      left,
      right: left + contentWidth,
      contentWidth,
      overflowX: el.scrollWidth > el.clientWidth,
      user: pick('user'),
      assistant: pick('assistant'),
      tool: pick('tool_call'),
    };
  })()`;
}
