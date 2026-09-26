#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""给常驻浏览器驱动发命令 / 在登录态里调 API 的小中转。

用法：
  jr.py eval  '<js表达式>'
  jr.py api   '<url>'              # 用页面登录态 fetch，返回 JSON（自动截断）
  jr.py text  [limit]
  jr.py goto  <url>
  jr.py html  <文件名>
  jr.py shot  <文件名>
  jr.py links [limit]
  jr.py net   [limit]
  jr.py info
  jr.py save  <输出文件>  '<js表达式>'   # 结果写文件（大结构用）
"""
import json, subprocess, sys, time, os

CMD = "/tmp/jr_cmd.json"
OUT = "/tmp/jr_out.json"


def send(obj, wait=4.0):
    # 先清掉上一次的输出，避免竞态读到旧结果（踩过：连发两命令拿到同一份 value）
    if os.path.exists(OUT):
        os.remove(OUT)
    if os.path.exists(CMD):
        os.remove(CMD)
    with open(CMD, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False)
    t0 = time.time()
    while time.time() - t0 < 90:
        time.sleep(0.6)
        if not os.path.exists(OUT):
            continue
        if os.path.getmtime(OUT) < t0 - 0.5:
            continue
        try:
            d = json.load(open(OUT, encoding="utf-8"))
        except Exception:
            continue
        if d.get("cmd") == obj.get("cmd"):
            return d
    return {"ok": False, "err": "超时（驱动没响应）"}


def main():
    if len(sys.argv) < 2:
        print(__doc__); return
    a = sys.argv[1]
    if a == "eval":
        r = send({"cmd": "eval", "js": sys.argv[2]})
    elif a == "api":
        url = sys.argv[2]
        lim = sys.argv[3] if len(sys.argv) > 3 else "180000"
        # 该平台的 API 需要在请求头带 Authorization（localStorage.token 里的 access_token）
        # 实测该平台的自定义方案：authorization 头是 "Bearer__<token>"（双下划线，不是空格），
        # 另需 version 头；只带 cookie 或 "Bearer <token>" 都会被 401 Access is Denied。
        js = ("(async()=>{"
              "let tk='';try{const t=JSON.parse(localStorage.getItem('token')||'{}');"
              "tk=(t.access_token||t.accessToken||'');}catch(e){}"
              "const h={'version':'12.1.1'};if(tk)h.authorization='Bearer__'+tk;"
              "const r=await fetch(%s,{credentials:'include',headers:h});"
              "const t=await r.text();return JSON.stringify({status:r.status,body:t.slice(0,Number(%s)||180000)});})()"
              % (json.dumps(url), json.dumps(lim)))
        r = send({"cmd": "eval", "js": js}, wait=8)
    elif a == "apisave":
        url, out = sys.argv[2], sys.argv[3]
        js = ("(async()=>{"
              "let tk='';try{const t=JSON.parse(localStorage.getItem('token')||'{}');"
              "tk=(t.access_token||t.accessToken||'');}catch(e){}"
              "const h={'version':'12.1.1'};if(tk)h.authorization='Bearer__'+tk;"
              "const r=await fetch(%s,{credentials:'include',headers:h});"
              "return JSON.stringify({status:r.status,body:(await r.text())});})()"
              % json.dumps(url))
        r = send({"cmd": "eval", "js": js}, wait=8)
        v = (r.get("result") or {}).get("value")
        if v is None:
            print("取回失败:", json.dumps(r, ensure_ascii=False)[:400]); return
        o = json.loads(v)
        st, body = o.get("status"), o.get("body") or ""
        open(out, "w", encoding="utf-8").write(body)
        print("HTTP %s  -> %s  (%d 字节)" % (st, out, len(body)))
        try:
            j = json.loads(body)
            dd = j.get("data")
            if isinstance(dd, dict): print("  data 键:", list(dd.keys())[:24])
            elif isinstance(dd, list): print("  data 列表长度:", len(dd))
            if j.get("message"): print("  message:", str(j["message"])[:120])
        except Exception:
            print("  非 JSON，前 200 字:", body[:200].replace("\n", " "))
        return
    elif a == "click":
        r = send({"cmd": "click", "sel": sys.argv[2]}, wait=10)
    elif a == "clicktext":
        r = send({"cmd": "clicktext", "text": sys.argv[2], "exact": "--exact" in sys.argv}, wait=10)
    elif a == "clickxy":
        r = send({"cmd": "clickxy", "x": int(sys.argv[2]), "y": int(sys.argv[3])}, wait=6)
    elif a == "pages":
        r = send({"cmd": "pages"})
    elif a == "netclean":
        r = send({"cmd": "netclean"})
    elif a == "resume":
        r = send({"cmd": "resume"})
    elif a == "text":
        r = send({"cmd": "text", "limit": int(sys.argv[2]) if len(sys.argv) > 2 else 3000})
    elif a == "goto":
        r = send({"cmd": "goto", "url": sys.argv[2]}, wait=8)
    elif a == "html":
        r = send({"cmd": "html", "path": sys.argv[2]})
    elif a == "shot":
        r = send({"cmd": "shot", "path": sys.argv[2], "full": "--full" in sys.argv})
    elif a == "links":
        r = send({"cmd": "links", "limit": int(sys.argv[2]) if len(sys.argv) > 2 else 200})
    elif a == "net":
        r = send({"cmd": "net", "limit": int(sys.argv[2]) if len(sys.argv) > 2 else 100})
    elif a == "info":
        r = send({"cmd": "info"})
    elif a == "save":
        out, js = sys.argv[2], sys.argv[3]
        r = send({"cmd": "eval", "js": "(async()=>{const v=await (%s);"
                                     "return JSON.stringify(v);})()" % js}, wait=10)
        v = (r.get("result") or {}).get("value")
        if v is None:
            print(json.dumps(r, ensure_ascii=False)[:800]); return
        open(out, "w", encoding="utf-8").write(v)
        print("-> %s  (%d 字节)" % (out, len(v)))
        return
    else:
        print("未知子命令:", a); return
    res = r.get("result", r)
    print(json.dumps(res, ensure_ascii=False, indent=1)[:2500])


if __name__ == "__main__":
    main()
