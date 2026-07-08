# 本体链集成：Claude Code 信任层

> **核心思想:** 将本体链 (Ontology) 的信任协作架构映射到 Claude Code 工作流,实现可审计、可验证、可信的多 Agent 开发系统。
> **实现策略:** 本地化优先 (Local-first) — 不依赖真实区块链,用 `~/.claude/*.json` 模拟 ONT ID + Orange Protocol 语义。

---

## 1. 架构映射

| Ontology 公链层 | 本项目实现 | 文件 |
|----------------|------------|------|
| **ONT ID (DID)** | `did:local:<机器指纹>:<哈希>` | `scripts/agent_did.py:issue_did()` |
| **可验证凭证 (VC)** | W3C 标准 JSON + SHA256 证明 | `scripts/agent_did.py:build_vc()` |
| **Orange Protocol (信誉)** | 事件加权得分 (0-100) | `scripts/agent_did.py:record_event()` |
| **VBFT 共识** | 2/3 权重阈值多 Evaluator | `.claude/commands/ontology/review.md` |
| **跨链协议** | Git hooks + MCP 跨工具 | `scripts/git_hooks/*` |
| **多 VM 智能合约** | Skill 模块 (Markdown + Python) | `.claude/commands/ontology/*` |
| **P2P 加密 (可选)** | 本地 JSON,无需网络 | N/A (单机模式) |

---

## 2. 快速开始

### 2.1 签发你的 Agent DID

```bash
python3 scripts/agent_did.py issue
```

输出示例:
```json
{
  "id": "did:local:280b7ffda52a6076:33e4d05f542d933308566413",
  "fingerprint": "280b7ffda52a6076",
  "issued_at": 1783469598,
  "expires_at": 1783555998,
  "capabilities": ["rust", "code-review", "hls"]
}
```

**DID 格式解读:**
- `did:local` — 协议 (模仿 `did:ont`)
- `280b7ffda52a6076` — 机器指纹 (hostname + username + platform 的 SHA256 前 16 位)
- `33e4d05f542d933308566413` — 时间戳哈希 (24h 有效期)

### 2.2 安装 Git Hooks

```bash
bash scripts/install_git_hooks.sh
```

效果:
- **commit-msg** — 自动在 commit message 末尾追加 `[DID: ...] [VC: <hash>] rep=<score>`
- **post-commit** — 自动 `record_event commit_success` 到信誉库

### 2.3 运行质量追踪

```bash
bash scripts/track_quality.sh all   # cargo test + clippy,自动记分
bash scripts/track_quality.sh test  # 仅测试
bash scripts/track_quality.sh clippy
```

### 2.4 查看状态

在 Claude Code 中:
```
/ontology:status
```

或直接:
```bash
python3 scripts/agent_did.py reputation
```

---

## 3. 工作流集成

### 3.1 修改前 (默认流程)

```
edit code → cargo test → cargo clippy → git commit
   ↓           ↓            ↓             ↓
   无凭证     单次验证     单次验证      无溯源
```

### 3.2 修改后 (本体链风格)

```
edit code → /ontology:review → git commit
   ↓             ↓                 ↓
   无凭证    4 Evaluator 投票    自动注入 DID + VC
              ↓
         自动 record_event
         信誉分 ±delta
```

---

## 4. 信誉系统详解

### 4.1 事件权重表

| 事件 | delta | 触发场景 |
|------|-------|----------|
| `test_pass` | +2.0 | cargo test 全过 |
| `test_fail` | -3.0 | cargo test 失败 |
| `clippy_pass` | +1.0 | clippy 无警告 |
| `clippy_fail` | -1.5 | clippy 有警告 |
| `review_approve` | +1.5 | 多 Evaluator 通过 |
| `review_reject` | -2.0 | 多 Evaluator 否决 |
| `commit_success` | +0.5 | git commit 成功 |
| `commit_failed` | -1.0 | git commit 失败 |

**分数范围:** 0-100 (50 为初始中位)
**历史保留:** 最近 500 条事件 + 每 10 条快照

### 4.2 信誉等级

| 分数 | 等级 | 含义 |
|------|------|------|
| 80+ | ⭐ Trusted | 高可信 Agent |
| 60-79 | ✅ Reliable | 正常可信 |
| 40-59 | ⚠️ Cautious | 关键操作需复核 |
| 20-39 | 🛑 Restricted | 禁止危险操作 |
| 0-19 | ❌ Banned | 强制重新校准 |

### 4.3 当前会话信誉 (示例)

```json
{
  "score": 54.0,
  "total_events": 3,
  "events": [
    {"event": "test_pass", "detail": "185 passed", "delta": +2.0},
    {"event": "test_pass", "detail": "185 passed", "delta": +2.0},
    {"event": "clippy_pass", "detail": "0 warnings", "delta": +1.0}
  ]
}
```

---

## 5. 多 Evaluator 投票 (VBFT)

### 5.1 与本体链 VBFT 的对应

| Ontology VBFT | 本项目 /ontology:review |
|---------------|-------------------------|
| 共识节点 | 4 个 Evaluator Agent |
| 提案 | 代码变更 / PR |
| 投票 | approve / reject + 权重 |
| 阈值 | 2/3 (≈ 0.667) |
| 出块 | git commit |
| 区块验证 | cargo test + clippy |

### 5.2 Evaluator 权重设计

| Evaluator | 权重 | 职责 |
|-----------|------|------|
| `code-reviewer` | 1.0 | 代码风格 / 命名 / 重复 |
| `security-reviewer` | 1.5 | 安全 / unsafe / 注入 |
| `tdd-guide` | 1.2 | 测试覆盖 / 边界 |
| `qa_engineer` | 1.2 | 编译 / Lint / 依赖 |

**权重设计理念:**
- **security 1.5** — 与本体链对验证人节点惩罚更重的设计思路一致
- **tdd 1.2 / qa 1.2** — 测试与质量门禁是 release 前的关键闸口,失败代价高
- **code-review 1.0** — 风格/命名问题可在后续 PR 中迭代修复

### 5.3 投票失败处理

**不阻塞,而是记录:**

```bash
# 假设 clippy 失败
python3 scripts/agent_did.py record --event clippy_fail --detail "5 warnings"
# 信誉分: 54.0 → 52.5

# 下次 commit 时 commit-msg hook 会附加警告:
# [DID: ...] [VC: ...] rep=52.5 ⚠️ Agent reputation low
```

---

## 6. 可验证凭证 (VC) 示例

执行 `/ontology:review` 后生成的 W3C 格式 VC:

```json
{
  "@context": "https://www.w3.org/2018/credentials/v1",
  "type": ["VerifiableCredential", "CodeReviewCredential"],
  "issuer": "did:local:280b7ffda52a6076:33e4d05f542d933308566413",
  "issuanceDate": "2026-07-08T00:13:31Z",
  "credentialSubject": {
    "commit": "6433dad",
    "agent_reputation": 54.0,
    "evaluators": [
      {"reviewer": "code-reviewer", "vote": "approve", "weight": 1.0},
      {"reviewer": "security-reviewer", "vote": "approve", "weight": 1.5},
      {"reviewer": "tdd-guide", "vote": "approve", "weight": 1.2},
      {"reviewer": "qa_engineer", "vote": "approve", "weight": 1.2}
    ],
    "consensus_ratio": 1.0,
    "threshold": 0.667,
    "verdict": "approved"
  },
  "proof": {
    "type": "Sha256Hash",
    "proofValue": "36338f89c1733a064917f816fb654c44da510deb7db80da16d5fd4dc11f89ed6"
  }
}
```

---

## 7. 文件清单

```
scripts/
├── agent_did.py                    # DID + 信誉 + VC 核心 (~200 行)
├── track_quality.sh                # cargo test/clippy + 自动记分
├── install_git_hooks.sh            # 安装 commit-msg + post-commit hooks
└── git_hooks/
    ├── commit-msg                  # 自动注入 DID + VC 到 commit message
    └── post-commit                 # 自动记录 commit_success

.claude/commands/ontology/
├── review.md                       # /ontology:review — 多 Evaluator 投票
└── status.md                       # /ontology:status — 信誉仪表盘

~/.claude/                          # 本地存储 (运行时生成)
├── agent_did.json                  # 当前 DID (24h 有效)
└── agent_reputation.json           # 信誉历史 (最近 500 事件)
```

---

## 8. 适用场景与边界

### ✅ 适合用

| 场景 | 价值 |
|------|------|
| 多人协作 PR 审查 | 多 Agent 投票代替单 reviewer 偏见 |
| 高风险代码路径 (加密 / 网络) | security reviewer 权重 1.5 |
| 长期项目信誉追踪 | 分数随时间累积,反映 Agent 表现 |
| CI/CD 流水线审计 | commit message 自动注入 VC,可追溯 |

### ❌ 不适合用

| 场景 | 原因 |
|------|------|
| 一次性脚本 | 增加复杂度无收益 |
| 实验性代码 | 信誉追踪反而干扰探索 |
| 极小修改 (typo) | 4 Evaluator 投票过于重量级 |
| 学习阶段 | 信任机制会限制探索 |

---

## 9. 已知限制

1. **DID 不跨机器同步** — `did:local` 只在当前机器有效 (模拟)
2. **信誉库本地存储** — 重装系统会丢失 (符合"Local-first"原则)
3. **VC 证明是 SHA256 哈希** — 非真正密码学签名 (足够防篡改,但不能防密钥伪造)
4. **Evaluators 是逻辑角色** — 实际是 Claude Code 子 Agent,非独立进程

如需真链级别安全,需集成:
- `ontio/ontology` Go 节点 (RPC 调用)
- ONT ID SDK (Rust 客户端)
- ONG 钱包 (Gas 支付)

---

## 10. 未来扩展 (Roadmap)

- [ ] **模式 4: Skill 上链市场** — WASM 合约封装第三方 Skill,按调用付费 (ONG)
- [ ] **模式 6: 跨系统协调** — Claude Code ↔ Cursor ↔ Aider 状态同步
- [ ] **真链集成** — 接入 Ontology 测试网,DID 上链 (需要 ONG gas)
- [ ] **信誉可视化** — Web 仪表盘 (`docs/agent_reputation.html`)
- [ ] **多 Evaluator 并行** — 真正并行调用 4 个 Claude 子 Agent

---

## 11. 参考资料

- **本体链项目:** https://github.com/ontio/ontology
- **本体链官网:** https://ont.io
- **ONT ID 规范:** W3C DID Core (https://www.w3.org/TR/did-core/)
- **VC 规范:** W3C Verifiable Credentials (https://www.w3.org/TR/vc-data-model/)
- **VBFT 共识:** Ontology 共识算法白皮书

---

**版本:** v1.0 (2026-07-08)
**维护者:** Claude Code × 本体链架构研究