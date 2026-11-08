---
name: download-agent-loop
description: 在规划或实现下载策略、Resolver 选择、下载、muxer、文件输出，或需要独立端到端验收时使用。
---

## 1. Multi-Agent 开发循环 (Anthropic 模式)

**来源**: Anthropic 工程博客 — 三 Agent 架构：Planner / Generator / Evaluator

### 三角色分工

| 角色 | 职责 | 你的项目中的应用 |
|------|------|-----------------|
| **Planner** | 产品层面高层次规划，输出规格书（不含技术细节） | 规划下载策略、Resolver 选择逻辑 |
| **Generator** | 按 sprint 增量开发，自测后交棒 | 执行下载、muxer 拼接、文件输出 |
| **Evaluator** | **独立分离**，真实验证（不只是读代码） | 校验文件完整性、实际跑下载测例 |

### 核心原则

- **Evaluator 必须独立**：同一个 agent 评自己代码一定会给好评，必须分离
- **sprint contract**：Generator 和 Evaluator 事先协商"完成定义"（如：下载成功率 ≥ 95%，文件完整可播放）
- **文件系统通信**：Agent 间通过文件传递状态，无花哨消息队列
- **主动验证**：Evaluator 不做静态审代码，直接跑实际下载流程验证

### 你的项目适配

```text
URL → Planner(规划下载策略) → Generator(执行下载) → Evaluator(验证文件完整性)
                                    ↑                    ↓
                                    ←←← 循环直到通过 ←←←←
```

### 不适用场景

视频下载是**确定性任务**（有正确答案的执行），不是探索性产品开发。Planner 不碰技术细节这条**不适用**——视频下载涉及协议解析、平台差异，必须含技术上下文。

### 参考文献

- Anthropic 工程博客 (2026-03-24): Building Effective AI Agents
- Boris Cherny (Claude Code 负责人): "I don't prompt Claude anymore. I have loops running that prompt Claude. My job is to write loops."
