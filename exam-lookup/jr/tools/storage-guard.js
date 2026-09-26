/* 浏览器验证脚本的存储护栏 —— 跑任何会碰 localStorage 的测试前，先拿它包一层。

   为什么必须有：刷题页的错题本/字号/主题都存在 http://127.0.0.1:8765 这个 origin 的
   localStorage 里，而自动化脚本是在**用户自己的浏览器 profile** 里跑同一个 origin 的。
   2026-09-22 我为了验证「错题本导出 json」直接调了 `localStorage.clear()`，
   把该窗口里真实的错题本一起清掉了（只存了题号 id，没有备份就找不回来）。
   用户的日常窗口是另一个浏览器所以没丢，但这是运气，不是设计。

   用法：
     await withStorage(page, async () => { ... 随便改 localStorage ... });
   退出时按快照恢复：脚本里多出来的键删掉、被改的写回原值。
   想真的从空开始测，就在回调里自己清，护栏在结束后仍会恢复成原样。 */

async function withStorage(page, fn){
  const snap = await page.evaluate(`(() => {
    const o = {};
    for (let i = 0; i < localStorage.length; i++) { const k = localStorage.key(i); o[k] = localStorage.getItem(k); }
    return JSON.stringify(o);
  })()`);
  try {
    return await fn();
  } finally {
    await page.evaluate(`(() => {
      const snap = ${snap};
      Object.keys(localStorage).forEach(k => { if(!(k in snap)) localStorage.removeItem(k); });
      Object.entries(snap).forEach(([k, v]) => localStorage.setItem(k, v));
      return Object.keys(snap).length;
    })()`);
  }
}

// 只读地看一眼当前 origin 存了什么（排障用，不改动任何键）
const dumpStorage = (page) => page.evaluate(`(() => {
  const o = {};
  for (let i = 0; i < localStorage.length; i++) { const k = localStorage.key(i); o[k] = localStorage.getItem(k); }
  return JSON.stringify(o).slice(0, 400);
})()`);
