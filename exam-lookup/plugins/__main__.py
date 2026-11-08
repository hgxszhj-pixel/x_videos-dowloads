#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""插件命令行：python3 -m plugins <list|status|selftest|reload|due|review|enroll>

    python3 -m plugins list                 # 谁在、谁挂了
    python3 -m plugins selftest             # 全部插件 + 加载器自检（含热加载演练）
    python3 -m plugins due --limit 10       # 今天该复习哪些
    python3 -m plugins review --bank 自测-安全数字.json --qid GEN-J0001 --rating good
    python3 -m plugins enroll --limit 50    # 把答错过的题装进复习队列
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from plugins import load, status, selftest as loader_selftest          # noqa: E402


def _p(name):
    plugins, errors = load()
    if name not in plugins:
        sys.exit("插件 %s 不可用：%s" % (name, errors.get(name, "未安装")))
    return plugins[name]


def main():
    ap = argparse.ArgumentParser(prog="python3 -m plugins")
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("list")
    sub.add_parser("status")
    sub.add_parser("selftest")
    sub.add_parser("reload")

    d = sub.add_parser("due"); d.add_argument("--limit", type=int, default=10)
    d.add_argument("--bank")
    r = sub.add_parser("review")
    r.add_argument("--bank", required=True); r.add_argument("--qid", required=True)
    r.add_argument("--rating", default="good", choices=["again", "hard", "good", "easy"])
    e = sub.add_parser("enroll")
    e.add_argument("--limit", type=int, default=100); e.add_argument("--bank")
    e.add_argument("--all", action="store_true", help="不只装错题")
    a = ap.parse_args()

    if a.cmd in ("list", "status"):
        print(json.dumps(status(), ensure_ascii=False, indent=1))
        return 0
    if a.cmd == "reload":
        load(force=True)
        print("已强制重载：", json.dumps(status(), ensure_ascii=False))
        return 0
    if a.cmd == "selftest":
        for name, p in load()[0].items():
            fn = p.get("selftest")
            if fn:
                fn()
        loader_selftest()
        print("插件 selftest 全过")
        return 0
    if a.cmd == "due":
        print(json.dumps(_p("srs")["routes"]["/api/srs/due"](
            {"limit": a.limit, "bank": a.bank}), ensure_ascii=False, indent=1))
        return 0
    if a.cmd == "review":
        card = _p("srs")
        from plugins.srs import review as srs_review
        print(json.dumps(srs_review(a.bank, a.qid, a.rating), ensure_ascii=False, indent=1))
        return 0
    if a.cmd == "enroll":
        from plugins.srs import enroll as srs_enroll
        print(json.dumps(srs_enroll(limit=a.limit, only_wrong=not a.all, bank=a.bank),
                         ensure_ascii=False, indent=1))
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main() or 0)
