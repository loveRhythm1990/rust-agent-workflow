# 第 5 章：模拟执行器与完整流程

状态：已实现模拟执行器，四条路径（PASS、FAIL→修复→PASS、FAIL→停止、UNKNOWN→停止）可重复运行。本章不用模型账号，也不执行真实命令；验收结果来自脚本化替身。

## 为什么先模拟再真实

前面四章各自解决一个局部问题：契约、模型、状态机、决策。这一章把它们串成闭环，但此时接手还轮不到真实 Agent。原因很简单：状态机没有跑通闭环之前，接一个昂贵、不稳定、不可审查的模型 CLI 既难调试也难证明规则正确。

本章用脚本化替身替代双 CLI：你要验收的每一步——实现、验收、审查——都由一个不变的脚本来扮演。脚本定下来，结果是确定的：四条路径任意重复运行，输出完全一致。只有闭环规则对了、错了都知道哪一步错，才谈得上把 Codex / Claude Code 接进来。

## 实现思路：先证明闭环，再接执行者

本章的实现顺序和第 9 章一样：先想清楚"替身要证明什么"，再倒推代码。

**第一步：把"模拟闭环"翻译成三个证据需求。**

| 证据需求 | 反问题 | 对应机制 |
| --- | --- | --- |
| 规则走在状态机里 | 脚本会不会偷偷主导流程？ | `transition`：脚本给的只是验收结果，走哪条边只看允许表 |
| 结论由独立规则算出来 | 替身写"PASS"就信吗？ | `decide` + `plan_next`：清单 → 结论 → 动作，由控制器侧计算 |
| 档案与后续章节同形 | 模拟编的格式将来要不要重写？ | 复用第 3 章 `TaskRecord`：`runs` / `verdict` / `stop_reason`，不发明新词汇 |

**第二步：把三个机制串成一条数据流。** 脚本进、规则过、档案出，每一步都能单独拒绝（重复登记、脚本耗尽、非法转移），拒绝不会伪装成成功。

**第三步：用路径清单推导测试。** 四条路径（PASS / FAIL→修复→PASS / FAIL→停止 / UNKNOWN→停止）加上三个拒绝场景，正好覆盖状态机的每条边与每个错误变体——测试不是按"代码行"送的，是按"流程图"送的。

**第四步：被否掉的方案。**

| 方案 | 否掉的原因 |
| 先接真实 CLI 把闭环跑通 | 规则 bug 与进程/账号 bug 会混在同一次运行里，定位成本成倍；先让状态机每条边可重复跑通，真实执行只引入新失败 |
| 让脚本给出最终状态或判定 | 等于让替身决定流程，控制器又要回去信它；路线与结论只能由 `transition` / `decide` / `plan_next` 算出来 |
| 为模拟发明一份新 JSON 格式 | 模拟与真实共用第 3 章 `TaskRecord`；两套词汇到持久化时会打架 |
| 用 `exit_code` 携带质量结论 | 第 3 章已定退出码是运行事实；模拟里的 `exit_code: 0` 恰好用来证明运行成功 ≠ 质量通过 |

## 架构总览

```text
Script（脚本化替身，只给验收结果）
  │ attempts: Vec<AttemptPlan> → CheckOutcome[]
  ▼
run_simulation
  ├─ transition（第 4 章状态机：每一步只走允许表里的边）
  ├─ build_summary（验收结果 → VerificationSummary，重复登记拒收）
  ├─ decide（清单 → PASS / FAIL / UNKNOWN）
  ├─ plan_next（结论 + 剩余修复次数 → READY / FIXING / STOPPED）
  ├─ fixes_remaining 计数（预算不减到负数）
  └─ 任一环节出错 → Err(Transition / DuplicateCheck / ScriptExhausted)
  ▼
SimulationReport
  = TaskRecord（第 3 章模型：runs + verdict + stop_reason）
  + final_status + attempts_used + fixes_used + decisions[]
```

注意流向是单向的：脚本在入口进、档案在出口出，中间的判断没有一个字段由脚本决定。这正是"Agent 不能自己宣布完成"。

## 设计思路

| 决策 | 理由 |
| --- | --- |
| 模拟与真实执行分开，先建“脚本 → 状态机”闭环 | 状态机的每条边在真实执行之前都已被确定的路径验证过；真实执行在第 7 章接入时才需要处理进程、超时与取消这些新失败 |
| 脚本只描述验收结果，不描述状态 | 状态走向交给 `transition`、`decide`、`plan_next` 计算：脚本说“这次验收哪些过、哪些败、哪些未定”，流程怎么走结论怎么给由规则决定；脚本写不出状态，状态也不会被脚本改写 |
| `exit_code: 0` 的本义保持在第 3 章的原意 | 运行记录里 `exit_code` 只是“模拟步骤完成”，与 `verdict` 与 `stop_reason` 是两个不同字段；本章用代码体现这一点 |
| 流程失败返回 `Err`，不伪造成成功 | 脚本耗尽、重复登记、状态机拒绝任一发生，函数就返回 `Err`；失败先暴露，不靠 printf 结尾的“通过”字样圆场 |
| 每条 attempt 保留决策记录与运行事实 | `decisions` 让你看到每一步的结论与动作；这是第 14 章事件日志的最小起点，崩溃恢复的第 15 章才能据此对账 |
| `--json` 输出第 3 章模型，不给模拟新增格式 | 任务档案只用第 3 章定义的 JSON，模拟不发明词汇；将来第 7、8 章扩展的是该档案的内容，不是本章自己的格式 |

## 运行实验

以下命令都从仓库根目录运行。无需模型账号，无需网络。

```bash
cargo run -p chapter-05-simulation
```

输出四个独立场景。每个场景先交代脚本写的验收结果，再按状态机推演结论与动作。预期输出：

```text
== 场景一：全部验收通过（最多 2 次修复） ==
脚本 attempt 1：原有输出保留：通过、新增行为符合预期：通过、反例未触发：通过
  结论 PASS，动作 READY
最终状态 Done，结论 PASS，停止原因 无
```

### 实验一：看完整记录 JSON

```bash
cargo run -p chapter-05-simulation -- --json 3
```

打印场景三的 `TaskRecord`：每次 attempt 一条运行记录、`verdict: "FAIL"`、`stop_reason: "BUDGET_EXHAUSTED"`。同样的字段也被后续持久化章节使用；模拟只是用脚本构造，格式与真实流程共用。

### 实验二：脚本耗尽

在 [src/main.rs](src/main.rs) 中把场景三的 `attempts` 改成一个，让第二次尝试缺失，运行会看到：

```text
流程失败：脚本没有第 2 次尝试的记录（不当作成功）
```

脚本先写两次失败的机会，再想强行用第二次失败顶替，结果不会通过伪装成成功——值得注意的是：脚本也得是真材实料，不骗人。

### 实验三：存疑的退出码 / 运行成功

任一场景 JSON 中 `exit_code: 0` 仅表示模拟程序自身完成，不代表任务通过。在场景三中它旁边同时写着 `verdict: "FAIL"` 与 `stop_reason: "BUDGET_EXHAUSTED"`：这就是第 3 章说过的“运行成功不等于质量通过”，现在是闭环里的具体一条记录。

## 实现结构

- [src/main.rs](src/main.rs)：四个场景、控制台演示与 `--json` 输出。
- [src/lib.rs](src/lib.rs)：脚本类型、模拟执行与错误；含自动化测试。

核心实现是 `run_simulation(&Task, &Script, u32) -> Result<SimulationReport, SimulationError>`：按 attempt 推进状态机、用脚本构造验收清单、调用第 4 章的 `decide` 与 `plan_next`、维护修复预算并返回最终档案。`build_summary` 用 `HashSet` 拦截重复登记的验收项。

## 模拟的事实边界

本章不调用模型、不执行契约中的命令、不读真实文件；验收项目全部来自脚本。脚本说“通过”不等于真实通过；它只用来推演流程。模拟报告同样不能直接代替证据——真正的验收会记录命令、环境、时间戳等真实产物，第 11 章才把它们绑在一起。这里也不会把审查环节真正模拟成有发现的逻辑；本篇的 REVIEWING 只是状态走空，发现动作在第 12 章展开。

## 验证与完成标准

```bash
cargo test -p chapter-05-simulation
cargo check --workspace
cargo fmt --all --check
```

测试覆盖：正常路径 DONE、修复成功仅用一次预算、预算耗尽保留 FAIL、UNKNOWN 不升级为 FAIL、重复验收被拒绝、脚本耗尽被拒绝、JSON 往返后的 FAIL 与 BUDGET_EXHAUSTED 词汇一致。

完成本章后，你应能用脚本分别造出 PASS、修复成功和 UNKNOWN 三份模拟报告，并说明为什么 `exit_code: 0` 可出现在 verdict 是 FAIL 的记录里。

## 动手练习

1. 把场景二改成“第一次 FAIL、第二次仍 FAIL”，跑到 BUDGET_EXHAUSTED，记录 `attempts_used`。
2. 在同一次 attempt 里写两个同名验收项，观察 DuplicateCheck 的输出。
3. 把 `max_fixes` 和脚本各改一遍，确认四条路径的规则分工互不混淆：定路线的是状态机，定结果的是验收。

参见 [Rust 新手讲解](RUST.md)。

[上一章](../04-state-machine/README.md) · [下一章：编排层选型与接入](../06-orchestration/README.md) · [返回学习路线](../README.md)
