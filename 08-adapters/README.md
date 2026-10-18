# 第 8 章：Agent 接口与结构化输出校验

状态：已实现统一 Agent 接口、显式会话身份绑定与报告校验。本章的适配器都是模拟替身；真实 CLI 适配器在第 10 章接入时实现同一个 trait。

## 为什么需要统一接口

第 6 章定下了委派协议的骨架（delegation_id、task_id、no_tools），但要接的执行者不止一个：编码的、审查的，将来还可能是别的工具。如果每个工具各写一套调用代码，控制器很快会变成适配器的杂烩；更糟的是，校验逻辑会随工具漂移——pi 的报告过了这关，codex 的报告说不定走的是另一条没人检查的路。

本章用 trait 把“怎么调用”统一成一个接口，把“报什么”统一成一份 serde 结构（它也是 JSON 契约文档），把“信不信”统一在控制器侧的校验器里。空报告、截断、身份错配——这三种最常见的数据问题——都必须被拒绝，而不是“反正是 Agent 说的”。

## 设计思路

| 决策 | 理由 |
| --- | --- |
| 接口用 trait 定义，而不是每个工具一个函数 | 调用方（校验器、harness）只面对 `Agent`；换实现零改动。这是“模拟适配器可替换”的结构性保证，而不只是测试技巧 |
| 会话身份由控制器生成、显式注入请求，报告必须原样回带 | 会话 id 是“哪一次委派”的锚点：不显式绑定，就分不清报告属于哪轮执行；串了会话的报告对账时会错认 |
| 报告结构用 serde 类型定义 | `AgentReport` 的字段表就是 JSON 契约：缺字段、多字段、类型不对在解析时暴露，不靠文档口说 |
| 校验在控制器侧，Agent 自报 COMPLETED 不算数 | 空报告是“最便宜的成功”；校验器要求 COMPLETED 必须有内容、FAILED 必须有原因，拒绝不放行 |
| 截断与缺字段都落到 Malformed | 输出中途断流与字段缺失在解析层就失败了；区分“哪种坏”对排障有价值，但对控制器而言都是“不可采信” |
| 模拟适配器可注故障（Fault 枚举） | 证明校验器真的会拒绝：五种故障各对应一个测试；没有故障注入的校验器只在想象中工作 |

## 模型与校验

```text
控制器                      Agent（trait）
  │ AgentRequest
  │  delegation_id ─────┐
  │  task_id            │ 回带同一批身份字段
  │  session_id ◄───────┼────── AgentReport
  │  instruction        │       status / summary / findings
  ▼                     │            │
  bind_request ─────────┘             ▼
                            validate_report
                            （JSON 合法？身份回带一致？内容非空？）
                                    │
                            AcceptedReport / Err(拒绝理由)
```

校验顺序是有意的：先 JSON 合法性，再身份配对，最后内容规则——序列化失败的报文没有资格谈身份，身份错配的报文没有资格谈内容。

## 运行实验

从仓库根目录运行，无需账号。

```bash
cargo run -p chapter-08-adapters
```

预期输出：

```text
委派 del-101（任务 task-001），下面用统一接口调两类适配器。

== 合法报告 ==
pi 模拟适配器（会话 sess-pi-01）：模拟完成：为本地任务工具增加问候输出
codex 模拟适配器（会话 sess-codex-01）：模拟完成：为本地任务工具增加问候输出

== 故障报告都被拒绝 ==
截断：拒绝，报告不是合法 JSON：EOF while parsing a string at line 1 column 68
空报告：拒绝，空报告：COMPLETED 但没有任何内容
身份错配：拒绝，身份错配：session_id 应为 sess-01，报告回带 sess-elsewhere
缺字段：拒绝，报告不是合法 JSON：missing field `status` at line 1 column 152
无失败原因：拒绝，FAILED 报告缺少失败原因

校验在控制器侧：Agent 自报 COMPLETED 不算数，身份要回带、内容要非空。
会话身份显式绑定；第 10 章接真实 CLI 时，报告必须原样回带同一会话。
```

两个观察点：

- 两个适配器走的是完全相同的 `AgentBinding::run` 路径——换执行者不改调用方，这就是“可替换”的含义；
- 五种故障各自有明确的拒绝理由，没有一种被静默放过。

## 实现结构

- [src/lib.rs](src/lib.rs)：`Agent` trait、`AgentRequest` / `AgentReport` / `ReportStatus`、`validate_report`、`run_with`、`AgentBinding`，以及可注故障的 `ScriptedAgent`；全部自动化测试在此。
- [src/main.rs](src/main.rs)：合法路径与五种故障的演示。

核心接口三个：`bind_request`（委派 + 会话 → 请求）、`validate_report`（原始文本 → 校验后的报告或拒绝理由）、`run_with`（对任何实现 `Agent` 的对象执行“调用 + 校验”）。

## 边界的真实成本

本章的 Agent 都是内存里的模拟替身：没有进程、没有账号、没有网络。真实 CLI 的报告（pi 的 `--mode json` 事件流、编码工具的自然语言输出）要转成 `AgentReport`，那是第 10 章适配器的工作，届时“报告是模型生成的文本”这个风险才真正进场——驯服它的手段仍是这套身份绑定与非空校验，再加第 11 章的证据绑定。

另一个诚实的边界：校验器保证“报告结构可信”，不保证“报告内容为真”。Agent 说“我改好了”不等于改好了——那要靠独立验证（第 11 章）。校验只是把明显的假话挡在门外。

## 验证与完成标准

```bash
cargo test -p chapter-08-adapters
cargo check --workspace
cargo fmt --all --check
```

测试覆盖：合法报告被接受且身份回带、状态词汇为 COMPLETED / FAILED、截断被拒、空报告被拒、会话错配被拒、缺字段被拒、FAILED 无原因被拒、适配器互换零改动、运行时替换（Box<dyn Agent>）。

完成本章后，你应能说出：trait 在这里保证了什么可替换性、为什么会话 id 由控制器生成、以及“校验结构”与“相信内容”之间的边界。

## 动手练习

1. 给 `AgentReport` 加一个 `model: String` 字段（谁生成的报告），让 serde、校验器、模拟适配器、测试全部跟着更新——体会“契约改动会传染到哪里”。
2. 新增一种故障：报告多带了未知字段；先推演默认行为（serde 容忍还是拒绝），再运行验证。
3. 把 `AgentBinding` 的 `session_id` 改成每次 `run` 时生成新的，观察“每次委派绑定一个会话”在测试中断言了什么。

参见 [Rust 新手讲解](RUST.md)。

[上一章](../07-process/README.md) · [下一章：工作区隔离与快照冻结](../09-snapshots/README.md) · [返回学习路线](../README.md)
