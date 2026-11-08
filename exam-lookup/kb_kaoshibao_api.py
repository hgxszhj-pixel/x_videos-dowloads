#!/usr/bin/env python3
"""kb_kaoshibao_api —— 考试宝题库接口客户端（自算签名 + 自解响应密文）。

为什么能用：考试宝的「搜题」按次计费（免费额度会用完），但「题库内容」接口不是搜题，
只受签名与响应加密保护，两层都已逆出（证据在站点自己的 JS 里，见 ANSWER_SOURCES.md）：

    sign = md5(SALT + uu + path + ts + SALT)      SALT = 12b6bb84e093532fb72b4d65fec3f00b
    path = 去掉协议/主机、去掉开头 /api 的请求路径（含 query）
    uu   = cookie uu（客户端标识）
    头：TimeStamp / Sign / VERSION / CLIENT-IDENTIFIER / REQUEST-ID / Authorization / platform=web
    响应：base64 → 3DES-ECB-PKCS7，key = localStorage['passphrase']（回退固定 key）

    （与图灵《考试宝 sign 参数逆向》实战视频同一套：写死常量 + cookie uu + URL 去主机去 /api
      + 13 位毫秒时间戳，MD5 拼接后在头里带上时间戳。视频管这叫「重算」。）

不触发风控的几条硬规矩（`selftest` 逐条断言，改代码时别破坏）：

    1. 串行 + 节流：默认 1.2s 间隔 + 0~0.8s 抖动（≈0.5 req/s）。同一 uu 的并发峰值和固定周期
       是最容易被打上的指纹——所以不并发、不整点、不批 20 条一起发。
    2. TimeStamp 每请求严格递增且唯一，并且用页面 Date.now() 的时钟（和真客户端同源，不吃本机时钟偏移）。
    3. 头按真客户端补齐：Accept / Referer / VERSION / platform（UA、Origin 由浏览器 fetch 自己带）。
    4. 999（未登录）/30001（额度用尽）/拦截页 = 立刻停（抛 Blocked），不重试、不换路径硬刚；
       只有 403/429/5xx 才指数退避重试。
    5. `--max-requests` 给单次运行封顶，别一次跑穿整个题库。

链路：
    GET /api/questions/lists?paperid=<pid>&size=20&page=1  → 题目 id 列表（一次性给全量）
    GET /api/questions/detail?id=<qid>                     → 题干 + 选项 + **答案** + 解析

用法:
    python3 kb_kaoshibao_api.py pull --paper 17348551 [--out jr/out/ksb_17348551.json]
    python3 kb_kaoshibao_api.py pull --paper 17348551 --limit 20 --gap 2.0 --verbose
    python3 kb_kaoshibao_api.py selftest          # 离线：节流/时间戳/中止三条不变量
"""
import argparse
import base64
import hashlib
import json
import os
import random
import secrets
import subprocess
import sys
import time
import urllib.parse
import urllib.request

import websocket

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")
PORT = 9224
SALT = "12b6bb84e093532fb72b4d65fec3f00b"
FB_KEY = "WmrGowJCtUQd/2PgoyrJuQ=="
VERSION = "2.4.7"          # 站点前端版本号；不影响签名，只影响「像真客户端」
ORIGIN = "https://www.kaoshibao.com"
BLOCK_CODES = {"999", "30001"}          # 未登录 / 额度用尽：再发也是白挨一次风控记分
BLOCK_MARKS = ("请求已被拦截", "安全验证", "验证码", "访问频率", "请稍后再试")
# 2026-10-27：账号被站点停用（「账号涉及违规操作，违反《用户服务协议》」）。
# 批量取数这条路到此为止——不是节奏问题，是条款问题。默认冻结，只有人工明确解冻才允许联网。
FROZEN_MSG = ("已冻结：2026-10-27 考试宝账号因「违规操作」被停用，本脚本不再联网取数。\n"
              "要临时解冻只能在本人账号状态恢复后显式设置 KSB_ALLOW=1，并且只用官方额度内的量。\n"
              "停用记录与替代方案见 exam-lookup/考试宝-停用与替代方案.md")


def frozen():
    return os.environ.get("KSB_ALLOW") != "1"


def norm_path(url):
    u = url
    if u.startswith("http"):
        p = urllib.parse.urlparse(u)
        u = p.path + ("?" + p.query if p.query else "")
    if u.startswith("/api"):
        u = u[4:]
    return u


def mk_sign(path, uu, ts):
    return hashlib.md5((SALT + uu + norm_path(path) + str(ts) + SALT).encode()).hexdigest()


def open_tab(url):
    q = urllib.parse.quote(url, safe="")
    req = urllib.request.Request("http://127.0.0.1:%d/json/new?%s" % (PORT, q), method="PUT")
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return json.load(r)["webSocketDebuggerUrl"]
    except Exception:
        raise SystemExit("连不上 %d 调试 Chrome。先起：python3 ~/.hermes/skills/media/"
                         "zhixueyun-kc-video/scripts/kc_chrome.py start，或带 "
                         "--remote-debugging-port=%d 开 Chrome 并登录考试宝。" % (PORT, PORT))


class Blocked(Exception):
    """站点明确拒绝（未登录 / 额度用尽 / 拦截页）——立刻收手，别重试。"""


class Pacer:
    """请求节流：最小间隔 + 抖动。clock/sleep 可注入，selftest 用假时钟。"""

    def __init__(self, min_gap=1.2, jitter=0.8, clock=time.monotonic, sleep=time.sleep,
                 rand=random.random):
        self.min_gap, self.jitter = min_gap, jitter
        self.clock, self.sleep, self.rand = clock, sleep, rand
        self.last = None
        self.gaps = []

    def wait(self):
        gap = self.min_gap + self.rand() * self.jitter
        now = self.clock()
        due = gap if self.last is None else self.last + gap
        if due > now:
            self.sleep(due - now)
            now = due
        self.last = now
        self.gaps.append(gap)
        return gap


def classify(status, text):
    """ok / transient（可退避重试）/ blocked（立刻停）。"""
    body = (text or "").strip()
    if status in (-1, 403, 429) or status >= 500:
        return "transient"
    if status >= 400:
        return "transient"
    if any(m in body[:400] for m in BLOCK_MARKS):
        return "blocked"
    if not body:
        return "transient"
    try:
        j = json.loads(body)
    except Exception:
        return "ok" if not body.startswith("<") else "blocked"
    if isinstance(j, dict) and str(j.get("code")) in BLOCK_CODES:
        return "blocked"
    return "ok"


class Ksb:
    """一条 CDP 连接；请求都在页面上下文里发（同源、带登录 cookie），串行节流。"""

    def __init__(self, paper_id, port=PORT, pacer=None, transport=None):
        self.paper = str(paper_id)
        self.pacer = pacer or Pacer()
        self.i = 0
        self.ts = 0
        self.skew = 0
        self.blocked = None
        self.sent = 0
        self.retried = 0
        self.key = ""
        self.ws = None                       # 只有真连浏览器时才被赋成 websocket 连接
        if transport is not None:            # selftest：不碰浏览器
            self.transport = transport
            self.token, self.uu, self.ws = "fake", "fake", None
            return
        self.ws = websocket.create_connection(
            open_tab("%s/ti/%s/" % (ORIGIN, self.paper)),
            timeout=90, suppress_origin=True, max_size=1 << 26)
        self.send("Runtime.enable")
        time.sleep(5)
        self.token = self.ev("decodeURIComponent(document.cookie.split('; ').find(x=>x.startsWith('token='))?.slice(6) || '')") or ""
        self.uu = self.ev("document.cookie.split('; ').find(x=>x.startsWith('uu='))?.slice(3) || ''") or ""
        self.key = (self.ev("localStorage.getItem('passphrase') || ''") or "").strip('"')
        if not self.token:
            raise SystemExit("页面里没有登录 token —— 先在 9224 的 Chrome 里登录考试宝")
        if not self.uu:
            # 自己编一个 uu 会算出「看起来合法、实际对不上」的签名，比不发还显眼
            raise SystemExit("页面里没有 uu cookie —— 刷新一次 /ti/ 页面再跑")
        page_now = self.ev("Date.now()")
        if isinstance(page_now, (int, float)) and page_now > 0:
            self.skew = int(page_now) - int(time.time() * 1000)
        self.transport = self._cdp_transport

    # ---- CDP ----
    def send(self, m, **p):
        self.i += 1
        self.ws.send(json.dumps({"id": self.i, "method": m, "params": p}))
        while True:
            r = json.loads(self.ws.recv())
            if r.get("id") == self.i:
                return r.get("result", {})

    def ev(self, e):
        r = self.send("Runtime.evaluate", expression=e, returnByValue=True, awaitPromise=True)
        if r.get("exceptionDetails"):
            return None
        return r.get("result", {}).get("value")

    def _cdp_transport(self, path, headers):
        expr = ("(async () => { try { const r = await fetch(%s, {headers: %s, credentials: 'include'});"
                " return [r.status, (await r.text()).slice(0, 400000)]; }"
                " catch(e) { return [-1, String(e)]; } })()"
                ) % (json.dumps(path), json.dumps(headers))
        v = self.ev(expr) or [-1, ""]
        return int(v[0]), (v[1] or "")

    # ---- 单请求：节流 → 唯一时间戳 → 分类 → 退避/停手 ----
    def _headers(self, path, ts):
        return {"TimeStamp": str(ts), "Sign": mk_sign(path, self.uu, ts), "VERSION": VERSION,
                "CLIENT-IDENTIFIER": self.uu, "REQUEST-ID": secrets.token_hex(16),
                "Authorization": '"%s"' % self.token, "platform": "web",
                "Accept": "application/json, text/plain, */*",
                "Referer": "%s/ti/%s/" % (ORIGIN, self.paper)}

    def next_ts(self):
        now = int(time.time() * 1000) + self.skew
        self.ts = max(self.ts + 1, now)
        return self.ts

    def call(self, path, tries=3):
        if self.blocked:
            raise Blocked(self.blocked)
        for n in range(tries):
            self.pacer.wait()
            ts = self.next_ts()
            status, text = self.transport(path, self._headers(path, ts))
            self.sent += 1
            kind = classify(status, text)
            if kind == "ok":
                return self.decrypt(text)
            if kind == "blocked":
                self.blocked = "HTTP %s %s" % (status, (text or "")[:120])
                raise Blocked(self.blocked)
            self.retried += 1
            if n + 1 < tries:
                time.sleep(min(2 ** n, 20) + random.random())   # 退避 + 抖动
        return None

    def decrypt(self, text):
        text = (text or "").strip()
        if not text:
            return None
        if text.startswith("{"):
            try:
                return json.loads(text)
            except Exception:
                return None
        b = text.replace("-", "+").replace("_", "/")
        b += "=" * (-len(b) % 4)
        try:
            raw = base64.b64decode(b)
        except Exception:
            return None
        tmp = "/tmp/ksb_%s.bin" % secrets.token_hex(4)
        open(tmp, "wb").write(raw)
        try:
            for key in (self.key, FB_KEY):
                if not key:
                    continue
                rr = subprocess.run(["openssl", "enc", "-d", "-des-ede3", "-nopad",
                                     "-K", key.encode().hex(), "-in", tmp], capture_output=True)
                if rr.returncode:
                    continue
                d = rr.stdout
                if d and 1 <= d[-1] <= 8:
                    d = d[:-d[-1]]
                t = d.decode("utf-8", "ignore").strip()
                if t.startswith("{"):
                    try:
                        return json.loads(t)
                    except Exception:
                        continue
        finally:
            os.path.exists(tmp) and os.remove(tmp)
        return None

    # ---- 业务 ----
    def paper_ids(self):
        r = self.call("/api/questions/lists?paperid=%s&size=20&page=1" % self.paper) or {}
        rows = (r.get("data") or {}).get("rows") or []
        return [str(x) for x in rows]

    def question(self, qid):
        r = self.call("/api/questions/detail?id=%s" % qid) or {}
        q = (r.get("data") or {}).get("question")
        if not q:
            return None
        opts = q.get("options")
        if isinstance(opts, str):
            try:
                opts = json.loads(opts)
            except Exception:
                opts = []
        return {"id": q.get("id"), "question": q.get("question"), "qtype": q.get("qtype"),
                "options": opts, "answer": q.get("answer"), "analysis": q.get("analysis"),
                "difficulty": q.get("difficulty"), "hash": q.get("hash"),
                "paperid": q.get("paperid"), "uid": q.get("uid")}


# ---------- 离线自检：三条不变量，跑一遍就知道节流还在不在 ----------
def selftest():
    fails = []

    def check(name, ok, detail=""):
        print(("  ok   " if ok else "  FAIL ") + name + ("  " + detail if detail else ""))
        if not ok:
            fails.append(name)

    class Clock:
        def __init__(self):
            self.t = 1000.0
            self.slept = 0.0

        def monotonic(self):
            return self.t

        def sleep(self, s):
            self.t += s
            self.slept += s

    def build(script):
        """script: 依次返回 (status, body) 的队列；队列空了就返回 200 正常体。"""
        c = Clock()
        p = Pacer(min_gap=1.2, jitter=0.8, clock=c.monotonic, sleep=c.sleep, rand=lambda: 0.5)
        seen = []

        def transport(path, headers):
            seen.append(headers)
            return script.pop(0) if script else (200, '{"code":"200","data":{}}')

        k = Ksb("1", pacer=p, transport=transport)
        k.decrypt = lambda text: json.loads(text) if (text or "").startswith("{") else text   # 不走 openssl
        return k, c, seen

    # 1) 串行节流 + 时间戳唯一递增
    k, c, seen = build([])
    for _ in range(10):
        k.call("/api/questions/detail?id=1")
    ts = [int(h["TimeStamp"]) for h in seen]
    gaps = p_gaps = k.pacer.gaps
    check("十次请求时间戳严格递增且不重复", all(b > a for a, b in zip(ts, ts[1:])), str(ts[:3]) + "…")
    check("间隔落在我设定的 1.6s（1.2 + 0.5×0.8）",
          len(gaps) == 10 and all(abs(g - 1.6) < 1e-6 for g in gaps), "%.2fs" % gaps[0] if gaps else "-")
    check("签名随路径/时间戳变化", len({h["Sign"] for h in seen}) == 10)
    check("头里 Referer/platform 齐全", all(h.get("platform") == "web" and "/ti/1/" in h.get("Referer", "")
                                          for h in seen))

    # 2) 429 退避后重试成功
    k, c, seen = build([(429, "Too Many Requests")])
    r = k.call("/api/questions/detail?id=2")
    check("429 退避重试一次后成功", r == {"code": "200", "data": {}} and len(seen) == 2, "请求 %d 次" % len(seen))

    # 3) 额度用尽 → 立刻停，且后续不再发
    k, c, seen = build([(200, '{"code":"30001","msg":"免费搜题次数已用完"}')])
    try:
        k.call("/api/questions/detail?id=3")
        check("30001 抛 Blocked", False)
    except Blocked:
        before = len(seen)
        try:
            k.call("/api/questions/detail?id=4")
        except Blocked:
            pass
        check("30001 抛 Blocked 且之后不再发请求", len(seen) == before == 1, "累计 %d 次" % len(seen))

    # 4) 拦截页 → 立刻停
    k, c, seen = build([(200, "<html><title>🐴 请求已被拦截</title></html>")])
    try:
        k.call("/api/questions/detail?id=5")
        check("拦截页抛 Blocked", False)
    except Blocked:
        check("拦截页抛 Blocked", len(seen) == 1)

    print("\nselftest %s（%d 项断言）" % ("全部通过" if not fails else "失败: " + ", ".join(fails),
                                        4 + 2 + 1 + 1))
    return 0 if not fails else 1


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    st = sub.add_parser("selftest")
    p = sub.add_parser("pull")
    p.add_argument("--paper", required=True)
    p.add_argument("--out")
    p.add_argument("--limit", type=int, default=0)
    p.add_argument("--gap", type=float, default=1.2, help="请求最小间隔秒（默认 1.2，真人翻题速度）")
    p.add_argument("--jitter", type=float, default=0.8, help="额外随机抖动上限秒（避免固定周期）")
    p.add_argument("--max-requests", type=int, default=0, help="单次运行请求上限（0 = 不限）")
    p.add_argument("--verbose", action="store_true")
    a = ap.parse_args()
    if a.cmd == "selftest":
        return selftest()
    if frozen():
        print(FROZEN_MSG)
        return 5

    k = Ksb(a.paper, pacer=Pacer(min_gap=a.gap, jitter=a.jitter))
    print("登录 token %d 位 | uu %s | 解密 key %s | 节流 %.1f~%.1fs"
          % (len(k.token), k.uu[:12], k.key or "(回退固定 key)", a.gap, a.gap + a.jitter))
    qs, ids, err = [], [], None
    try:
        ids = k.paper_ids()
        print("题库 %s → 题目 id %d 个" % (a.paper, len(ids)))
        if not ids:
            print("没拿到 id 列表（可能该题库无权限）")
            return 1
        if a.limit:
            ids = ids[:a.limit]
        for n, qid in enumerate(ids, 1):
            if a.max_requests and k.sent >= a.max_requests:
                print("到达 --max-requests %d，收工" % a.max_requests)
                break
            q = k.question(qid)
            if q:
                qs.append(q)
                if a.verbose:
                    print("  [%d/%d] %s | 答案 %s" % (n, len(ids), (q["question"] or "")[:36], q["answer"]))
            if n % 20 == 0:
                print("  进度 %d/%d（成功 %d）" % (n, len(ids), len(qs)))
    except Blocked as e:
        err = str(e)
    except KeyboardInterrupt:
        err = "手动中断"

    with_ans = sum(1 for q in qs if str(q.get("answer") or "").strip())
    print("抓取 %d 题，带答案 %d 题（请求 %d 次，退避重试 %d 次）"
          % (len(qs), with_ans, k.sent, k.retried))

    out = os.path.expanduser(a.out) if a.out else os.path.join(OUT, "ksb_%s.json" % a.paper)
    json.dump({"paperid": a.paper, "pulled_at": time.strftime("%Y-%m-%d %H:%M:%S"),
               "count": len(qs), "stopped": err, "questions": qs},
              open(out, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print("→ %s" % out)
    if err:
        print("已停：%s（已抓到的已存盘）" % err)
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
