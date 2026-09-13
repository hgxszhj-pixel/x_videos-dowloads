#!/usr/bin/env python3
"""
devtools_cookie_to_shanhu_state.py - 把 Chrome DevTools 复制的 Cookie 字符串转 camoufox state.json

背景:camoufox MCP v1.2.0 `export_state()` 不导出 HttpOnly cookie(返空数组),
但 SHANHU/珊瑚 SaaS 鉴权完全靠 HttpOnly cookie(localStorage tengface_log_uuid
是 UI 状态,不是 API token)。所以 --shanhu-login 流程断链。

本工具用最简来源填这个洞 —— Chrome DevTools Network 面板 → 任意 wanzibook
请求 → Request Headers → Cookie: 字段 → 复制 → 喂本脚本 → 直接得 state.json。

用法:
  1. Chrome 登录 wx-bgfcbieh.rexxi.cn 后,打开 DevTools → Network
  2. 任意触发一次 listSections(滚动/切换章节),点对应请求
  3. 复制 Request Headers 里的 Cookie 行整行(包含 "Cookie:" 前缀或裸 cookie)
  4. 跑:
     python3 scripts/devtools_cookie_to_shanhu_state.py \\
       --cookie "PHPSESSID=abc123; tengface_user=xxx; ..." \\
       --output /tmp/shanhu-state.json

  5. cargo run -- --shanhu-login /tmp/shanhu-state.json

可选:
  --domain-prefix auto  - 自动从 cookie 推断 domain (.wanzibook.com 或 .rexxi.cn)
  --secure true         - 默认所有 cookie 标 Secure + HttpOnly(防 dev tools 误拷贝)
"""
import argparse
import json
import sys
from pathlib import Path


# 已知 SHANHU/珊瑚 SaaS 域(cookie 解析不出 host 时回退)
SHANHU_DOMAINS = [".wanzibook.com", ".rexxi.cn", ".tenwiseacademy.cn"]


def parse_cookie_header(raw: str) -> list[dict]:
    """解析 'name=value; name2=value2' 格式 → camoufox cookie 字典列表。

    camoufox cookie dict 关键字段(其他字段可选):
      - name / value
      - domain (.wanzibook.com 形式,前面带点)
      - path (默认 /)
      - expires (unix timestamp, -1 = session)
      - httpOnly / secure (bool)
    """
    out = []
    for piece in raw.split(";"):
        piece = piece.strip()
        if not piece:
            continue
        if "=" not in piece:
            print(f"⚠️  跳过无 = 的片段: {piece[:40]}", file=sys.stderr)
            continue
        name, _, value = piece.partition("=")
        name = name.strip()
        value = value.strip()
        if not name or not value:
            continue
        out.append({
            "name": name,
            "value": value,
            "domain": "",  # 后面统一赋
            "path": "/",
            "expires": -1,
            "httpOnly": True,  # SHANHU 全用 HttpOnly
            "secure": True,
            "sameSite": "Lax",
        })
    return out


def assign_domain(cookies: list[dict]) -> list[dict]:
    """给 cookie 赋 domain 字段。

    启发式:从 cookie 名字 + 已知 SHANHU 域列表匹配。
    没法精确(cookie 不带 host 信息),用域后缀兜底。
    实际经验:.wanzibook.com 覆盖 PHPSESSID/主会话,.rexxi.cn 覆盖 UI 状态。
    """
    if not cookies:
        return cookies
    # 默认全标 .wanzibook.com(后端 API 域,任何 wanzibook 端点都会带)
    for c in cookies:
        c["domain"] = ".wanzibook.com"
    return cookies


def main():
    ap = argparse.ArgumentParser(
        description="Chrome DevTools Cookie 字符串 → camoufox state.json",
    )
    ap.add_argument("--cookie", required=True,
                    help="从 Chrome DevTools Network 面板 Request Headers 复制的 Cookie 整行")
    ap.add_argument("--output", "-o", default="/tmp/shanhu-state.json",
                    help="输出 state.json 路径")
    ap.add_argument("--domains", nargs="+", default=SHANHU_DOMAINS,
                    help="cookie domain 候选(按顺序匹配第一个)")
    args = ap.parse_args()

    cookies = parse_cookie_header(args.cookie)
    if not cookies:
        print("❌ 没解析出任何 cookie,检查输入格式", file=sys.stderr)
        sys.exit(1)

    cookies = assign_domain(cookies)

    state = {
        "cookies": cookies,
        "origins": [],
    }
    Path(args.output).write_text(json.dumps(state, indent=2, ensure_ascii=False))
    print(f"✅ 已写 {len(cookies)} cookies → {args.output}")
    print(f"   下一步:cargo run -- --shanhu-login {args.output}")


if __name__ == "__main__":
    main()
