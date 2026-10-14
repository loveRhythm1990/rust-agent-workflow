# 第 3 章：任务、运行、结果的数据模型

状态：已实现任务、运行、结果的数据模型与 JSON 序列化往返。本章不执行编码、命令或实际验收；演示中的执行记录是手工构造的示例数据。

## 为什么需要正式数据模型

第 2 章的契约是文本：能通过门禁，但程序无法可靠地取用“验收操作”，比较两版契约的目标也只剩字符串比对。文本更无法区分三件经常被混为一谈的事：

- 运行是否成功结束（运行事实）；
- 质量是否通过（证据结论）；
- 为什么停下来（停止原因）。

`cargo run` 退出码为 0 只说明进程正常退出，不说明需求被满足。本章用类型把三个维度分开：运行结果永远写不进质量结论字段；UNKNOWN（证据不足）是与 PASS、FAIL 并列的合法结论，不是失败的委婉说法；预算耗尽之类的停止原因不会伪装成质量失败。

## 设计思路

模型的每处类型选择都在阻止一类常见混淆：

| 决策 | 理由 |
| --- | --- |
| 结论用枚举，而不是 bool 或自由字符串 | bool 表达不了 UNKNOWN；字符串会积累拼写变体。类型把非法结论挡在编译期 |
| `Option<QualityVerdict>` 区分两层含义 | None 是尚未评估，Some(Unknown) 是评估过但证据不足；合并成一个 Unknown 会让“没测”伪装成“测过但不够” |
| verdict 与 stop_reason 各占一个字段 | 预算耗尽时结论仍可以是 FAIL（已证实的失败 + 无法继续修复）；两个字段各说一件事，单一状态字段表达不了这种组合 |
| `exit_code: Option<i32>` | 运行记录在启动时创建：开始时间已知、退出码未知，None 是真实状态；这也是第 15 章崩溃后对账“哪些运行没有结果”的依据 |
| `runs: Vec<Run>` | 修复循环让同一任务天然有多条执行（第 13 章按 attempt 追加），不是固定槽位 |
| 标识只求稳定引用 | task_id 用 String、run_id 用 u64；生成策略（递增、UUID）属于编排与持久化层，第 6、14 章再定 |
| JSON 词汇对齐设计文档 | `rename_all = "SCREAMING_SNAKE_CASE"` 输出 `UNKNOWN`、`BUDGET_EXHAUSTED`，报告、日志与后续存储使用同一套词汇 |

## 模型一览

| 类型 | 职责 | JSON 形态 |
| --- | --- | --- |
| `Contract` | 第 2 章契约文本的类型化版本，8 个字段 | 对象 |
| `Task` | 任务身份：编号 + 契约，不携带质量状态 | 对象 |
| `Run` | 执行记录：命令、时间、退出码、第几次尝试 | 对象 |
| `TaskRecord` | 任务档案：聚合身份、运行与结论 | 对象 |
| `QualityVerdict` | 质量结论：PASS / FAIL / UNKNOWN | `"PASS"` 等字符串 |
| `StopReason` | 停止原因：4 种 | `"BUDGET_EXHAUSTED"` 等字符串 |

`verdict` 与 `stop_reason` 都是 `Option`：`None` 表示尚未评估 / 流程仍在继续，`Some(Unknown)` 表示已评估但证据不足——两者语义不同，JSON 中分别是 `null` 与 `"UNKNOWN"`。

## 运行实验

以下命令都从仓库根目录运行。无需模型账号。

```bash
cargo run -p chapter-03-task-model
```

不传路径时读取内嵌示例契约，解析为 `Task`，构造一条以退出码 0 结束的示例 `Run`，打印任务档案 JSON，然后原样读回并对比。预期输出：

```text
任务档案 JSON：
{
  "task": {
    "task_id": "task-001",
    "contract": {
      "version": 1,
      "goal": "为本地任务工具增加问候输出",
      "scope": "仅修改 01-cargo/src/main.rs",
      "non_goals": "不接入模型，不修改依赖",
      "constraints": "保持现有输出，新增内容使用中文",
      "acceptance_command": "在仓库根目录运行 cargo run -p chapter-01-cargo",
      "expected_result": "原有输出保留，最后一行精确为“任务已接收”",
      "counter_example": "若缺少该行或删除原有输出，验收不通过"
    }
  },
  "runs": [
    {
      "run_id": 1,
      "task_id": "task-001",
      "attempt": 1,
      "command": "在仓库根目录运行 cargo run -p chapter-01-cargo",
      "started_at": 1770000000,
      "ended_at": 1770000005,
      "exit_code": 0
    }
  ],
  "verdict": "UNKNOWN",
  "stop_reason": null
}
序列化后原样读回，与原档案相等：true

这条运行以退出码 0 成功结束，质量结论仍是 UNKNOWN。
运行成功只是运行事实；PASS 只能来自有效证据，本章不产生证据。
```

同一份 JSON 里，`exit_code: 0` 与 `verdict: "UNKNOWN"` 并存——这就是本章要确立的事实。

### 实验一：解析第 2 章的示例契约

```bash
cargo run -p chapter-03-task-model -- 02-contracts/examples/task.txt
```

契约文本在两章之间通用：第 2 章负责校验，第 3 章负责变成类型。

### 实验二：拒绝未通过门禁的契约

```bash
cargo run -p chapter-03-task-model -- 02-contracts/examples/missing.txt
```

预期：标准错误输出包含 `字段“预期结果”不能为空`，程序退出码为 1。解析必须先过门禁，类型化模型不接受残缺文本。

### 实验三：读取失败

```bash
cargo run -p chapter-03-task-model -- 03-task-model/examples/not-found.txt
```

预期：显示读取失败，退出码为 2；与第 2 章相同的退出码约定，读取错误不伪装成契约问题。

## 实现结构

- [src/main.rs](src/main.rs)：读取契约，构造示例档案，演示 JSON 往返与结论说明。
- [src/lib.rs](src/lib.rs)：模型定义与 `parse_contract`，包含自动化测试。
- `examples/task.txt`：与第 2 章相同的示例契约。

本章第一次引入第三方依赖：serde 提供派生宏，serde_json 负责读写 JSON；同时通过 workspace path 依赖复用第 2 章的 `validate_contract`——校验规则只有一份，本章不复制。

核心接口 `parse_contract(&str) -> Result<Contract, String>`：门禁通过则返回类型化契约，否则携带全部问题返回。`TaskRecord` 为四个维度各留一个字段，`runs` 是 `Vec<Run>`，同一任务可以有多条执行记录。

## 门禁的真实边界

通过解析只说明字段齐全、类型正确，不代表目标合理或验收充分。JSON 可读也可手改，本章不校验语义一致性。演示中的 `Run` 是构造的示例：本章没有任何命令真正执行过，`exit_code: Some(0)` 是假设的运行事实。

真实证据需要绑定候选版本、环境与产物哈希，并区分证据类型与失效条件，这些在第 9、11 章实现。状态转换规则（哪个状态可以到达哪个状态）属于第 4 章，本章只提供结论与原因的载体。

## 验证与完成标准

```bash
cargo test -p chapter-03-task-model
cargo check --workspace
cargo fmt --all --check
```

测试覆盖契约解析与门禁拒绝、全类型 JSON 往返等值、`None` 与 `UNKNOWN` 的序列化差异、枚举 JSON 名与设计词汇一致。

完成本章后，你应能说出质量结论三态的含义、`None` 与 `UNKNOWN` 的区别，并解释为什么一条退出码 0 的运行不能把结论变成 PASS。

## 动手练习

1. 修改示例契约的目标字段并升版，先用第 2 章比较两版，再用本章解析新版本。
2. 在 main.rs 里给 `runs` 再追加一条尚未结束的运行（`ended_at` 与 `exit_code` 都是 `None`），观察 JSON 变化。
3. 把 `verdict` 改成 `Some(Fail)` 并配上一个 `stop_reason`，说明这种组合表达什么处境。

参见 [Rust 新手讲解](RUST.md)。

[上一章](../02-contracts/README.md) · [下一章：状态机与确定性决策](../04-state-machine/README.md) · [返回学习路线](../README.md)
