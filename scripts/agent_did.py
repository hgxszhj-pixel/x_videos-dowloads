#!/usr/bin/env python3
"""本地 Agent DID (Decentralized Identifier) + 信誉系统

模仿本体链 Ontology 的 ONT ID + Orange Protocol，本地化实现：
- DID: 基于机器指纹 + 用户名 + 时间戳的 SHA256 哈希
- 信誉: 记录每次 cargo test/clippy 的通过/失败，更新 Agent 信誉分
- 可验证凭证 (VC): 代码提交时携带 DID + 评估者投票

存储: ~/.claude/agent_did.json (DID 状态) + ~/.claude/agent_reputation.json (信誉历史)

用法:
    python3 scripts/agent_did.py issue        # 签发/获取当前 DID
    python3 scripts/agent_did.py did          # 打印当前 DID
    python3 scripts/agent_did.py record --event test_pass --detail "185 passed"
    python3 scripts/agent_did.py record --event test_fail --detail "..."
    python3 scripts/agent_did.py reputation   # 查看当前信誉分
    python3 scripts/agent_did.py vc --commit 6433dad --reviewers 'a:approve,b:reject'
"""

import argparse
import hashlib
import json
import os
import platform
import socket
import sys
import time
from pathlib import Path

DID_STORE = Path.home() / ".claude" / "agent_did.json"
REPUTATION_STORE = Path.home() / ".claude" / "agent_reputation.json"

# 事件权重（用于信誉分计算）
EVENT_WEIGHTS = {
    "test_pass": +2.0,
    "test_fail": -3.0,
    "clippy_pass": +1.0,
    "clippy_fail": -1.5,
    "review_approve": +1.5,
    "review_reject": -2.0,
    "commit_success": +0.5,
    "commit_failed": -1.0,
}


def _fingerprint() -> str:
    """机器指纹：hostname + username + platform — 用于 DID 生成"""
    raw = f"{socket.gethostname()}|{os.getenv('USER', 'unknown')}|{platform.machine()}"
    return hashlib.sha256(raw.encode()).hexdigest()[:16]


def _now() -> int:
    return int(time.time())


def load_did() -> dict | None:
    if not DID_STORE.exists():
        return None
    try:
        return json.loads(DID_STORE.read_text())
    except (json.JSONDecodeError, OSError):
        return None


def save_did(data: dict) -> None:
    DID_STORE.parent.mkdir(parents=True, exist_ok=True)
    DID_STORE.write_text(json.dumps(data, indent=2, ensure_ascii=False))


def load_reputation() -> dict:
    if not REPUTATION_STORE.exists():
        return {"events": [], "score": 50.0, "history": []}
    try:
        return json.loads(REPUTATION_STORE.read_text())
    except (json.JSONDecodeError, OSError):
        return {"events": [], "score": 50.0, "history": []}


def save_reputation(data: dict) -> None:
    REPUTATION_STORE.parent.mkdir(parents=True, exist_ok=True)
    REPUTATION_STORE.write_text(json.dumps(data, indent=2, ensure_ascii=False))


def issue_did() -> dict:
    """签发或复用现有 DID（24h 有效期）"""
    fp = _fingerprint()
    now = _now()

    existing = load_did()
    if existing and existing.get("fingerprint") == fp and existing.get("expires_at", 0) > now:
        return existing

    did_id = f"did:local:{fp}:{hashlib.sha256(f'{fp}{now}'.encode()).hexdigest()[:24]}"
    data = {
        "id": did_id,
        "fingerprint": fp,
        "issued_at": now,
        "expires_at": now + 86400,
        "capabilities": ["rust", "code-review", "hls"],
        "type": "AICodeAgent",
        "version": "1.0",
    }
    save_did(data)
    return data


def record_event(event: str, detail: str = "") -> dict:
    """记录事件并更新信誉分"""
    if event not in EVENT_WEIGHTS:
        print(f"未知事件类型: {event}", file=sys.stderr)
        sys.exit(1)

    rep = load_reputation()
    delta = EVENT_WEIGHTS[event]
    rep["score"] = max(0.0, min(100.0, rep["score"] + delta))

    entry = {
        "ts": _now(),
        "event": event,
        "detail": detail[:200],
        "delta": delta,
        "score_after": rep["score"],
    }
    rep["events"].append(entry)
    # 只保留最近 500 条
    rep["events"] = rep["events"][-500:]

    # 每 10 个事件生成一个历史快照
    if len(rep["events"]) % 10 == 0:
        rep["history"].append({
            "ts": _now(),
            "score": rep["score"],
            "total_events": len(rep["events"]),
        })

    save_reputation(rep)
    return entry


def get_did() -> str:
    did = issue_did()
    return did["id"]


def get_reputation() -> dict:
    rep = load_reputation()
    did = load_did()
    rep["did"] = did["id"] if did else "未签发"
    rep["total_events"] = len(rep["events"])
    return rep


def build_vc(commit: str, reviewers: str) -> dict:
    """构建可验证凭证 (Verifiable Credential) — 模拟 Ontology VC 标准"""
    did = issue_did()
    rep = load_reputation()

    # 解析 reviewers: 'reviewer1:vote1,reviewer2:vote2'
    parsed = []
    for r in reviewers.split(","):
        if ":" not in r:
            continue
        name, vote = r.split(":", 1)
        weight = 1.0
        if "security" in name.lower():
            weight = 1.5
        elif "tdd" in name.lower() or "qa" in name.lower():
            weight = 1.2
        parsed.append({"reviewer": name.strip(), "vote": vote.strip(), "weight": weight})

    total_weight = sum(r["weight"] for r in parsed)
    approve_weight = sum(r["weight"] for r in parsed if r["vote"] == "approve")
    consensus = approve_weight / total_weight if total_weight > 0 else 0.0

    vc = {
        "@context": "https://www.w3.org/2018/credentials/v1",
        "type": ["VerifiableCredential", "CodeReviewCredential"],
        "issuer": did["id"],
        "issuanceDate": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "credentialSubject": {
            "commit": commit,
            "agent_reputation": rep["score"],
            "evaluators": parsed,
            "consensus_ratio": round(consensus, 3),
            "threshold": 0.667,
            "verdict": "approved" if consensus >= 0.667 else "rejected",
        },
        "proof": {
            "type": "Sha256Hash",
            "created": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "proofValue": hashlib.sha256(
                f"{commit}{did['id']}{consensus}".encode()
            ).hexdigest(),
        },
    }
    return vc


def main() -> int:
    parser = argparse.ArgumentParser(description="本地 Agent DID + 信誉系统")
    sub = parser.add_subparsers(dest="cmd", required=True)

    sub.add_parser("issue", help="签发或复用 DID")
    sub.add_parser("did", help="打印当前 DID")
    sub.add_parser("reputation", help="查看当前信誉分")

    rec = sub.add_parser("record", help="记录事件")
    rec.add_argument("--event", required=True, choices=list(EVENT_WEIGHTS.keys()))
    rec.add_argument("--detail", default="")

    vc = sub.add_parser("vc", help="生成可验证凭证")
    vc.add_argument("--commit", required=True)
    vc.add_argument("--reviewers", default="", help="格式: name1:vote1,name2:vote2")

    args = parser.parse_args()

    if args.cmd == "issue":
        did = issue_did()
        print(json.dumps(did, indent=2, ensure_ascii=False))
    elif args.cmd == "did":
        print(get_did())
    elif args.cmd == "reputation":
        print(json.dumps(get_reputation(), indent=2, ensure_ascii=False))
    elif args.cmd == "record":
        entry = record_event(args.event, args.detail)
        print(json.dumps(entry, ensure_ascii=False))
    elif args.cmd == "vc":
        vc_data = build_vc(args.commit, args.reviewers)
        print(json.dumps(vc_data, indent=2, ensure_ascii=False))

    return 0


if __name__ == "__main__":
    sys.exit(main())