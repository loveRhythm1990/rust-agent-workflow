# 第 6 章：编排层选型与接入

状态：已实现选型实验记录、委派模型、模拟适配器与配置加载。本章不使用真实模型调用；真实 CLI 由第 7 章起接入。

## 为什么先选型再编排

前面五章把“自己干完全流程”走通了：契约、门禁、模型、状态机、模拟。问题是这不太现实——真正被委派的执行者是独立的命令行工具（Codex、Claude Code、pi、Hermes…），它们有自己的账号、会话、输出格式和失败方式。选择哪个、用它们做什么、不用它们做什么，这不是小决定：选错一个，后面所有受控执行、快照、取消都要为其重新定界。

我们的约束是清楚的：

- 控制器保留验收、预算、状态机与停止原因；执行者只提交候选，不能自行宣布通过。
- 优先复用现成工具，不自建通用编排引擎；每一次接入都需要真实实验验证。

## 设计思路

| 决策 | 理由 |
| --- | --- |
| 在本机实测两个候选的能力后再定选型 | 候选的真实能力（会话、输出格式、取消、JSON 接口）必须基于可验证的命令，而非二手期望 |
| 只完成模拟委派，不接真实执行 | 本章交付清单与边界；真实 CLI 的会话、超时、取消留给第 7 章开始 |
| 委派与结果都记录 delegation_id 与 task_id | 结果必须能关联回原任务；关联不上的结果（张冠李戴）与空结果一样被拒绝 |
| `no_tools: true` 进入委派契约 | 源码写入的隔离落在类型上：模拟委派不授权；将来逐个授权也要逐个验证 |
| 适配器配置来源是 JSON 文件而不是常量 | 候选、模型、授权开关集中可见，也可以随选型结论 diff 和审查 |

## 选型结论与依据

本章测试了两个本机可用的命令行工具：**pi** 与 **Hermes**。以下信息都已通过命令验证，不来自品牌说法。

| 维度 | pi | Hermes |
| --- | --- | --- |
| 已验证版本 | `1.2.0`（`pi --version`） | `Hermes Agent v0.21.6+675.g79b45d1`（`hermes --version`） |
| 非交互单次运行 | `pi -p "..." --no-session --no-tools` 单行输出，退出码 0 | `hermes -z "..."` 单行输出，退出码 0 |
| 结构化输出 | `--mode json` 产生逐事件 JSON 流（会话、回合、消息、用量） | `-z` 为纯文本；未找到等价的 JSON 流接口 |
| 会话身份 | `--session-id` 指定 uuid，警告明确“creating new session with that id” | `--resume SESSION` 可接续，但不承担显式会话 id 生成 |
| 源码写入隔离 | `--no-tools` 单行关闭工具 | `-z` 默认工具全开；不存在单行“无工具”验证模式 |
| 账号与验票 | `pi auth check` 提供 provider 与 model 注册信息 | `hermes status` 显示 DeepSeek 账号与 gateway 运行状态 |

基于上表，本章把 **pi** 作为已验证委派接口的候选：一行委派、可校验的会话 id、JSON 会话流、工具准入开关都可以在文档中看到。Hermes 不被忽略：它提供更丰富的会话与工作区能力，但迈向前的一体化调度（gateway、cron、webhook）超出首版单任务范围；后续接续时再评估。

选型结论不保护未来——第 7 章还要继续证明 pi 的会话、超时、取消都可行。第 6 章只适模拟委派这一步，这是已有实验支撑的部分。

## 模型与接管

本章把外部执行者抽象成一次有来有回的委派：

```text
Delegation ──────────────────────────────────────────────
  delegation_id ── SimulatedAdapter ──▶ DelegationResult
  task_id            （回传同源 id）         │
  instruction                              ▼
  no_tools                            accept_result
                                    （校验配对非空再允收）
```

- `encode_delegation(&Task, id)` 从任务契约生成编码委派；
- `SimulatedAdapter::delegate(&Delegation)` 返回可预测的、能关联的结果；
- `accept_result(&Delegation, &DelegationResult)` 校验 id 一致、结果非空；这两关都过才能算成功。

## 运行实验

以下命令都从仓库根目录运行，无需网络账号。

```bash
cargo run -p chapter-06-orchestration
```

预期输出：

```text
任务 task-001 的目标：为本地任务工具增加问候输出
委派 del-001 交给模拟执行者。
执行者回送：模拟执行：已收到「为本地任务工具增加问候输出」的说明，尚未实现真实编码
结果已接收并关联回任务 task-001
这真是模拟；控制器尚未交托真实 Agent（第 7 章起）。
边界是固定的：控制器仍持，由它决定任务是否通过；委派只交付候选。
```

### 实验一：加载适配器配置

```bash
cargo run -p chapter-06-orchestration -- 06-orchestration/examples/adapter-config.json
```

预期：读取成功并提示模拟适配器已接入。把 adapter 改成任何未知名字，会得到“未知适配器”——名字不合法就不假装接通。

### 实验二：结果与请求不匹配

在 [src/lib.rs](src/lib.rs) 的测试里把 `wrong` 的 delegation_id 和 task_id 改成与请求一致的字符串，测试会从成功转成失败：MismatchedIds 拒绝任何关联不上的结果。id 对接是“结果不是成功”的标志位。

### 实验三：空结果不算成功

把 DelegationResult 的 summary 改成空白字符串，`accept_result` 返回 EmptyResult。Demo 打印一段局面也好，空结果都不能充当成功。

## 实现结构

- [src/lib.rs](src/lib.rs)：Delegation、DelegationResult、AdapterConfig、accept_result、SimulatedAdapter，以及全部自动化测试。
- [src/main.rs](src/main.rs)：模拟委派演示与配置加载。
- `examples/task.txt` 与 `adapter-config.json`：demo 用的契约与配置。

## 边界的真实成本

本章的委派是模拟的，不代表真实执行的行为。真实接入时必须继续提供证据：工具版本本轮验证的是 `pi 1.2.0`、账号检查 `pi auth check` 能指出已注册的 provider、取消与超时仍待第 7 章验证。选型结论已经记录下来；后续章节仍须逐项给出证据。

此外：本章不能直接证明“执行者 不会 乱写源码”。信任来自不授予工具。第 7 章一旦出现真实的硬盘操作，就必须配合快照与产物目录来证明执行者行为被限制住了。

## 验证与完成标准

```bash
cargo test -p chapter-06-orchestration
cargo check --workspace
cargo fmt --all --check
```

测试覆盖：委派含 delegation_id 与 task_id、模拟结果被接收后可以关联回原任务、id 不匹配被拒绝、空结果被拒绝、JSON 配置往返、说明为空的模拟委派被拒绝。

完成本章后，你应能：

- 说明选型已验证的部分、哪些能力是推迟到下一章继续验证；
- 解释为什么 id 不匹配与空结果都不被视为成功；
- 指出第 8 章之后控制器与执行者各自的责任边界。

## 动手练习

1. 修改 examples/adapter-config.json 把 adapter 设为一个不认识的值，运行并读错误信息。
2. 在 lib.rs 里让模拟适配器回送的 delegation_id 与请求不同，跑一次确认 MismatchedIds 的输出。
3. 对照设计 PDF 第 3 节，记下控制器与编排器各自的授权边界，预备第 7 章进入受控执行。

参考 [原始设计 PDF](../docs/design/Hermes_Codex_Claude_Code_编排架构与证据验收_v2.pdf) 里的 Hermes 选型背景，但本 README 依据的是本章记录的实际命令证明。

参见 [Rust 新手讲解](RUST.md)。

[上一章](../05-simulation/README.md) · [下一章：受控执行适配、超时与取消](../07-process/README.md) · [返回学习路线](../README.md)
