#!/usr/bin/env node
/**
 * 常驻浏览器驱动：启动有头反检测浏览器 + 文件队列控制 + 网络抓包
 *
 * 为什么这么写：MCP 是 stdio 协议，没法跨终端调用。用文件队列当控制通道，
 * 浏览器进程常驻，我每次只需读写 /tmp/jr_cmd.json 和 /tmp/jr_out.json。
 *
 * 命令（写进 /tmp/jr_cmd.json）：
 *   {"cmd":"info"}                  当前 URL / 标题 / 尺寸
 *   {"cmd":"text"}                  正文纯文本（可带 limit）
 *   {"cmd":"html","path":"x.html"}  整页 HTML 存文件（给 ingest_saved_page.py 用）
 *   {"cmd":"goto","url":"..."}      跳转
 *   {"cmd":"links"}                 页面上所有链接（找导航入口）
 *   {"cmd":"shot","path":"x.png"}   截图
 *   {"cmd":"net"}                   抓到的所有请求（去重，找 API）
 *   {"cmd":"netclean"}              清空抓包缓冲
 *   {"cmd":"eval","js":"..."}       执行 JS 取值
 *   {"cmd":"cookies"}               导出 cookies 域名清单
 *   {"cmd":"handoff"}               交给人手动操作：暂停自动化，等你点完
 *   {"cmd":"resume"}                恢复自动化
 *   {"cmd":"quit"}                  退出
 */
const fs = require('fs');
const path = require('path');
const { chromium } = require('/Users/hgxszhj/develop_software/CloakBrowser/js-reverse-mcp-local-cloak/node_modules/patchright');

const CMD = '/tmp/jr_cmd.json';
const OUT = '/tmp/jr_out.json';
const PROFILE = '/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr/profile';
const OUTDIR = '/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr/out';
fs.mkdirSync(OUTDIR, { recursive: true });

const netlog = [];          // 抓到的请求
let paused = false;         // handoff 状态

function write(obj) { fs.writeFileSync(OUT, JSON.stringify(obj, null, 1)); }
function log(...a) { console.error('[' + new Date().toTimeString().slice(0, 8) + ']', ...a); }

(async () => {
  log('启动浏览器…');
  const ctx = await chromium.launchPersistentContext(PROFILE, {
    channel: 'chrome',
    headless: false,
    viewport: { width: 1440, height: 900 },
    locale: 'zh-CN',
    timezoneId: 'Asia/Shanghai',
    args: ['--no-first-run', '--no-default-browser-check', '--disable-blink-features=AutomationControlled'],
  });

  // 抓包：所有请求（含 XHR/Fetch 的响应片段，看得出是不是返回 JSON 数据）
  ctx.on('request', (r) => {
    const u = r.url();
    if (/\.(png|jpg|jpeg|gif|woff2?|ttf|css|svg)(\?|$)/i.test(u)) return;
    const rec = { t: Date.now(), m: r.method(), url: u, type: r.resourceType() };
    // API 请求额外记下请求头（找 token / 签名头；只留 auth 相关，避免噪声）
    if (/\/(api|oauth)\//.test(u)) {
      const h = {};
      try {
        const hs = r.headers();
        for (const k of Object.keys(hs)) {
          if (/auth|token|sign|trace|client|company|companyid|tenant|version|accept/i.test(k)) h[k] = hs[k];
        }
      } catch (e) { }
      if (Object.keys(h).length) rec.headers = h;
    }
    netlog.push(rec);
  });
  ctx.on('response', async (resp) => {
    const ct = (resp.headers()['content-type'] || '');
    if (!/json/i.test(ct)) return;
    const rec = netlog.filter(x => x.url === resp.url()).pop();
    if (!rec || rec.body) return;
    try {
      const b = await resp.text();
      if (b.length < 400000) rec.body = b.slice(0, 200000);
    } catch (e) { }
  });

  const page = ctx.pages()[0] || await ctx.newPage();
  page.setDefaultTimeout(45000);
  await page.goto('https://ctps.mylearning.cn/p1/index.html', { waitUntil: 'domcontentloaded', timeout: 60000 })
    .catch(e => log('首次导航:', e.message.slice(0, 90)));

  write({ ok: true, msg: '浏览器已启动，等待指令', url: page.url(), title: await page.title().catch(() => '') });
  log('就绪。控制文件:', CMD);

  // 文件队列循环
  setInterval(async () => {
    if (!fs.existsSync(CMD)) return;
    let req;
    try { req = JSON.parse(fs.readFileSync(CMD, 'utf8')); } catch (e) { return; }
    if (!req || !req.cmd) return;
    fs.unlinkSync(CMD);                       // 先删，避免重复执行
    const c = req.cmd;
    log('命令:', c);
    try {
      let r;
      switch (c) {
        case 'info':
          r = { url: page.url(), title: await page.title(), pages: ctx.pages().length };
          break;
        case 'text': {
          const t = await page.evaluate(() => document.body ? document.body.innerText : '');
          r = { len: t.length, text: req.limit ? t.slice(0, req.limit) : t };
          break;
        }
        case 'html': {
          const p = path.join(OUTDIR, req.path || 'page.html');
          fs.writeFileSync(p, await page.content(), 'utf8');
          r = { path: p, bytes: fs.statSync(p).size, url: page.url() };
          break;
        }
        case 'goto':
          await page.goto(req.url, { waitUntil: 'domcontentloaded', timeout: 60000 });
          r = { url: page.url(), title: await page.title() };
          break;
        case 'links': {
          const ls = await page.evaluate(() => [...document.querySelectorAll('a[href]')]
            .map(a => ({ href: a.href, text: (a.innerText || '').trim().slice(0, 40) }))
            .filter(x => x.href && !x.href.startsWith('javascript')));
          r = { count: ls.length, links: ls.slice(0, Number(req.limit) || 200) };
          break;
        }
        case 'shot': {
          const p = path.join(OUTDIR, req.path || 'shot.png');
          await page.screenshot({ path: p, fullPage: !!req.full });
          r = { path: p, bytes: fs.statSync(p).size };
          break;
        }
        case 'net': {
          const seen = new Map();
          for (const x of netlog) if (!seen.has(x.url)) seen.set(x.url, x);
          r = { count: seen.size, reqs: [...seen.values()].slice(-Number(req.limit) || -150) };
          break;
        }
        case 'netclean': netlog.length = 0; r = { msg: '已清空' }; break;
        case 'eval': r = { value: await page.evaluate(req.js) }; break;
        // 真实鼠标事件 —— 合成 el.click() 打不穿这个前端的 isTrusted 校验，必须走这个
        case 'click': {
          const loc = page.locator(req.sel).first();
          await loc.waitFor({ state: 'visible', timeout: req.timeout || 15000 });
          await loc.click({ timeout: req.timeout || 15000 });
          r = { msg: '已真实点击', sel: req.sel, url: page.url() };
          break;
        }
        case 'clickxy': {
          await page.mouse.click(Number(req.x), Number(req.y));
          r = { msg: '已点击坐标', x: req.x, y: req.y, url: page.url() };
          break;
        }
        // 点某个文字所在的元素（先滚动到可见再真点）
        case 'clicktext': {
          const el = page.getByText(req.text, { exact: !!req.exact }).first();
          await el.waitFor({ state: 'visible', timeout: req.timeout || 15000 });
          await el.click({ timeout: req.timeout || 15000 });
          r = { msg: '已点击文字', text: req.text, url: page.url() };
          break;
        }
        case 'pages': {
          r = { count: ctx.pages().length, list: ctx.pages().map(p => ({ url: p.url() })) };
          break;
        }
        case 'cookies': {
          const cs = await ctx.cookies();
          const by = {};
          for (const k of cs) by[k.domain] = (by[k.domain] || 0) + 1;
          r = { domains: by, total: cs.length };
          break;
        }
        case 'handoff': paused = true; r = { msg: '已暂停自动化，浏览器窗口归你操作；完成后发 resume' }; break;
        case 'resume': paused = false; r = { msg: '已恢复自动化', url: page.url() }; break;
        case 'quit':
          write({ ok: true, msg: '退出' });
          await ctx.close(); process.exit(0);
        default: r = { err: '未知命令: ' + c };
      }
      write({ ok: true, cmd: c, paused, result: r });
    } catch (e) {
      write({ ok: false, cmd: c, err: String(e).slice(0, 600) });
    }
  }, 800);
})();
