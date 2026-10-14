# 第 4 章：状态机与确定性决策

状态：已实现状态机与决策规则。本章不执行编码、命令或实际验收；演示中的验收清单是手工构造的示例。

## 为什么需要状态机和决策

第 3 章的模型只能“记录”，不能“裁判”。一个任务走到哪一步、下一步做什么，如果没有显式规则，就只能靠各处代码即兴判断——这正是“Agent 自行宣布任务完成”的老路。

本章确立两条分离：

- **质量是否通过**：只由验收清单计算（`decide`）；
- **动作是否继续**：由结论加预算单独决定（`plan_next`）。

STOPPED 不等于 FAIL：预算耗尽时已证实的失败保留 FAIL；证据缺失时保留 UNKNOWN，不冒充失败。非法的状态跳转被拒绝，而不是被静默容忍。

## 设计思路

| 决策 | 理由 |
| --- | --- |
| 转换用显式允许表，而不是散布的 if/switch | `ALLOWED_TRANSITIONS` 就是状态机全图：表外一律 Err。规则可以逐一审查，新增转换必须改表，不会“隐式可达” |
| `decide` 只看清单，不看状态或预算 | 同一规则适用于 DECIDING 及未来的任何环节；混入流程状态后规则就只能在单点使用 |
| FAIL 优先于 UNKNOWN | 已证实的失败不能被“还有没测的”掩盖；未验证项留在清单里随结论一起交付 |
| 空清单得出 UNKNOWN，不是 PASS | 没有任何验收记录通过时，无法证明通过；“空报告和成功退出不能证明正确” |
| 终态（DONE / STOPPED / CANCELLED）无出边 | 旧终态记录不可改写，恢复 = 创建新 attempt；这是第 15 章崩溃恢复与幂等地基 |
| 本章只数“剩余修复次数”一个预算维度 | 完整预算（轮次、时间、用量）是第 13 章；本章规则只回答“还有没有机会修” |
| 不建模 NEW / CLARIFYING | 契约确认发生在系统入口之前（第 2 章门禁 + 人工确认），系统内首状态是 PLANNED |
| WAITING_HUMAN 有转换、暂无自动触发 | 人工澄清的触发条件后续章节接入；先占位避免日后改枚举波及已有记录 |

## 规则一览

状态机（来源：设计文档第 13 节，按当前范围裁剪）：

```text
PLANNED → IMPLEMENTING → VERIFYING → REVIEWING → DECIDING
DECIDING → FIXING → VERIFYING          （失败进入修复环）
DECIDING → READY → DONE
DECIDING → WAITING_HUMAN → DECIDING    （人工再确认后返回）
DECIDING → STOPPED
所有活跃状态 → CANCELLED
DONE / STOPPED / CANCELLED 为终态，无出边
```

质量结论由验收清单计算：

| 清单状态 | 结论 |
| --- | --- |
| 任一项 FAIL | FAIL（未验证项照旧保留） |
| 无 FAIL，但有未验证项，或 passed 为空 | UNKNOWN |
| 其余（全部必需项通过） | PASS |

动作由结论加剩余修复次数决定：

| 结论 | 条件 | 动作 |
| --- | --- | --- |
| PASS | — | 进入 READY，候选可交付 |
| FAIL | 还有修复次数 | 进入 FIXING |
| FAIL | 修复次数为 0 | STOPPED，原因 BUDGET_EXHAUSTED，结论保留 FAIL |
| UNKNOWN | 无论剩余多少 | STOPPED，原因 EVIDENCE_MISSING，结论保留 UNKNOWN |

## 运行实验

以下命令从仓库根目录运行。无需模型账号。

```bash
cargo run -p chapter-04-state-machine
```

预期输出（四条决策路径 + 两次非法转换尝试）：

```text
决策分两步：先独立计算质量结论，再决定动作。清单为手工构造的示例。

== 场景一：全部验收通过 ==
质量结论 PASS，剩余修复次数 2，动作：READY（候选可交付）
Planned -> Implementing -> Verifying -> Reviewing -> Deciding -> Ready -> Done

== 场景二：FAIL 且还有修复机会 ==
质量结论 FAIL，剩余修复次数 2，动作：FIXING（进入修复）
Deciding -> Fixing -> Verifying -> Reviewing -> Deciding
质量结论 PASS，剩余修复次数 1，动作：READY（候选可交付）
Deciding -> Ready -> Done

== 场景三：FAIL 且修复次数耗尽 ==
质量结论 FAIL，剩余修复次数 0，动作：STOPPED（BUDGET_EXHAUSTED）
Deciding -> Stopped

== 场景四：证据缺失 ==
质量结论 UNKNOWN，剩余修复次数 2，动作：STOPPED（EVIDENCE_MISSING）
Deciding -> Stopped

== 非法转换 ==
Planned
  拒绝：非法状态转换：Planned 不可直接转到 Done
Done
  拒绝：Done 是终态，不可再转换；恢复请创建新 attempt

STOPPED 不是 FAIL：场景三保留 FAIL，场景四保留 UNKNOWN。
运行状态与质量结论分开记录，停止原因说明为什么停。
```

场景二值得细看：第一轮 FAIL 与“预期结果未复跑”共存，结论是 FAIL 而不是 UNKNOWN；修复后清单翻新、修复次数从 2 减到 1，第二轮才 PASS。

### 实验：观察非法转换被拒绝

在 [src/main.rs](src/main.rs) 的 `== 非法转换 ==` 段落里再添加一条路径，例如 `walk(&[TaskStatus::Verifying, TaskStatus::Done])`，运行确认被拒绝；也可以先猜哪些跳转会通过，再用输出验证——预期是：除非在允许表里，否则全部拒绝。

## 实现结构

- [src/status.rs](src/status.rs)：`TaskStatus`、`ALLOWED_TRANSITIONS` 允许表与 `transition`，含状态机测试。
- [src/decision.rs](src/decision.rs)：`VerificationSummary`、`decide` 与 `plan_next`，含决策测试。
- [src/lib.rs](src/lib.rs)：模块声明与 re-export。
- [src/main.rs](src/main.rs)：叙述性演示四条路径与两次拒绝。

核心接口三个纯函数：`decide(&VerificationSummary) -> QualityVerdict`（借用清单算结论）、`plan_next(&QualityVerdict, u32) -> NextAction`（结论加预算定动作）、`transition(TaskStatus, TaskStatus) -> Result<TaskStatus, String>`（按允许表转换）。质量结论与停止原因复用第 3 章类型，词汇不另造。

## 规则的真实边界

本章不执行任何东西：验收清单是构造的示例，真正的执行在第 5 章（模拟）与第 7 章起（受控执行）接入，第 11 章再谈证据的有效性。修复环只受“次数”约束，时间与用量预算在第 13 章。WAITING_HUMAN 的触发、SYSTEM_ERROR 的建模、状态持久化（第 14 章）都不在本章。规则本身不判断验收标准写得是否合理——那是契约（第 2 章）的事。

## 验证与完成标准

```bash
cargo test -p chapter-04-state-machine
cargo check --workspace
cargo fmt --all --check
```

测试覆盖全通过路径、修复环与等待环、每个活跃状态可取消、三个终态全部出边被拒、非法跳转被拒、FAIL 与缺失证据共存、空清单 UNKNOWN、结论到动作的四种映射。

完成本章后，你应能指出一条“退出码 0 但清单有未验证项”的运行会得到什么结论（UNKNOWN），并解释为什么 STOPPED 不改变质量结论。

## 动手练习

1. 在允许表里删掉 `(Deciding, Stopped)`，运行测试，看哪些场景失败——体会“表即全部规则”。
2. 给 `decide` 写一个新测试：passed 与 failed 同时非空（候选部分通过部分失败），先推演结论再验证。
3. 场景二里把第二轮的清单再留一个未验证项，推演 `plan_next` 的动作后运行确认。

参见 [Rust 新手讲解](RUST.md)。

[上一章](../03-task-model/README.md) · [下一章：模拟执行器与完整流程](../05-simulation/README.md) · [返回学习路线](../README.md)
