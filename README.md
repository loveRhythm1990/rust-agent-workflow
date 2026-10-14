# rust-agent-workflow

用 Rust 构建可验证的多 Agent 协作工具：从 Cargo 入门，到 Codex 编码、Claude Code 审查，再到可靠的流程控制与恢复。

本仓库有两个学习目标：掌握 Rust；理解并实现需求契约、证据验收、审查核实、有限修复和故障恢复。它是逐章演进的教学项目，当前仅第一章已实现，不代表完整系统已可用。

## 快速开始

安装 Rust 工具链（[官方安装说明](https://www.rust-lang.org/tools/install)），然后运行：

```bash
git clone https://github.com/loveRhythm1990/rust-agent-workflow.git
cd rust-agent-workflow
cargo run -p chapter-01-cargo
```

预期输出：

```text
你好，Rust！
项目：rust-agent-workflow
第 1 章：Cargo 与第一个程序
学习目标：用 Rust 构建可验证的多 Agent 协作工具
```

第一章使用 Rust 2024 edition，无第三方依赖，无需模型账号或 API Key。后续接入真实 CLI 时再核对版本、账号权限与运行参数。

## 学习路线

| 章 | 主题 | 核心内容 | 状态 |
| --- | --- | --- | --- |
| 01 | [Cargo 与第一个程序](01-cargo/README.md) | 项目结构、变量、函数 | 已实现 |
| 02 | [任务输入与字符串处理](02-ownership/README.md) | 所有权、借用、生命周期入门 | 规划中 |
| 03 | [任务与消息建模](03-task-model/README.md) | struct、enum、match、集合 | 规划中 |
| 04 | [JSON 与文件操作](04-json-files/README.md) | 模块、Serde、配置与日志 | 规划中 |
| 05 | [受控子进程执行](05-process/README.md) | Result、错误处理、参数数组 | 规划中 |
| 06 | [Agent 适配器](06-adapters/README.md) | trait、泛型、模拟 CLI | 规划中 |
| 07 | [接入 Codex 与 Claude Code](07-dual-cli/README.md) | 结构化输出、显式会话 ID | 规划中 |
| 08 | [需求契约与验收矩阵](08-contracts/README.md) | 需求充分性、版本冻结、独立判题依据 | 规划中 |
| 09 | [工作区隔离与代码快照](09-snapshots/README.md) | worktree、固定 SHA、只读边界 | 规划中 |
| 10 | [状态机与决策规则](10-state-machine/README.md) | 运行状态、质量结论、停止原因分离 | 规划中 |
| 11 | [独立验证与证据账本](11-evidence/README.md) | 原问题复现、证据有效性与失效 | 规划中 |
| 12 | [审查发现核实](12-findings/README.md) | 确认、反驳、未决、去重与误报 | 规划中 |
| 13 | [有限修复与收敛判断](13-convergence/README.md) | 轮次、停滞、振荡、回归 | 规划中 |
| 14 | [异步执行与资源预算](14-async-budget/README.md) | Tokio、超时、取消、进程组清理 | 规划中 |
| 15 | [持久化与崩溃恢复](15-recovery/README.md) | SQLite、事务、租约、过期结果拒绝 | 规划中 |
| 16 | [人工介入与安全边界](16-human-security/README.md) | 决策版本绑定、提示注入、权限限制 | 规划中 |
| 17 | [故障注入与效果评估](17-evaluation/README.md) | 恢复测试、查准率、召回率、成本对比 | 规划中 |
| 18 | [接入 Hermes 与完整交付](18-hermes/README.md) | 受控协调接口、端到端案例、交付包 | 规划中 |

## 学习方式

按章节阅读 README，运行示例，修改代码，再完成练习。每章解释 Rust 概念与工程选择；涉及可靠性时，提供失败场景、实验与明确验收标准。后续章节代码的复用关系会在对应章节中注明。

目前根目录使用 Cargo workspace，仅注册已实现章节。后续每章实现时再加入 workspace，避免目录占位被误认为可运行功能。

```bash
cargo check --workspace
cargo fmt --all --check
cargo test --workspace
```

## 最终目标与设计边界

Rust 控制器通过受控 CLI 子进程传递任务、快照和结构化结果；首先验证单仓库、单任务的双 CLI 闭环，再增加并发及 Hermes 协调入口。

- 编码、测试与审查绑定同一候选版本，旧报告不能推进新版本。
- 运行状态、PASS / FAIL / UNKNOWN 与停止原因分别记录。
- Review 发现先核实再修复；空报告和成功退出不能直接证明代码正确。
- 预算覆盖修复和核实；停滞、振荡、超时与取消应可靠停止并保留现场。
- worktree 不是安全沙箱；只读和权限隔离需要实际机制验证。
- 模型漏报、业务歧义和精确用量限制不能凭机制宣称消除，实验应披露剩余限制。

## 参考与公开内容

章节组织参考 [mini-pi](https://github.com/loveRhythm1990/mini-pi) 的渐进教程形式。架构学习依据用户提供的《Hermes + 双 CLI 编排架构与证据验收》v2 方案（2026-10-09）；本仓库不附带该原始 PDF，相关设计将用原创讲解与实验展开。

公开仓库不提交凭据、真实会话日志或未经检查的模型产物。当前未选择开源许可证；公开可见不等同于授予开源许可。
