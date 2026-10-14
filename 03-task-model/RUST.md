# 第 3 章 Rust 配套：结构体、枚举与序列化

本章从文本处理进入数据建模。先看懂六个类型定义，再读解析函数和 JSON 往返。新语法都围绕本章代码出现的位置展开。

## 1. struct：把相关数据放在一起

```rust
struct Task {
    task_id: String,
    contract: Contract,
}
```

字段标记 `pub` 才能被包外使用；本章所有模型字段公开，main 与测试直接用字面量构造：

```rust
let task = Task {
    task_id: "task-001".to_string(),
    contract,
};
println!("{}", task.task_id);
```

构造时每个字段都要给出值；`contract` 是变量名与字段名相同时的简写。Task 里放一个 Contract、TaskRecord 里放一个 Task，只是字段类型恰好是另一个结构体——嵌套不需要额外语法。

## 2. enum：一个值，多种可能

```rust
enum QualityVerdict {
    Pass,
    Fail,
    Unknown,
}
```

`QualityVerdict` 只有这三种取值，不可能出现“大概率通过”。`StopReason` 同理：`BudgetExhausted`、`NoProgress`、`EvidenceMissing`、`Cancelled`。枚举值通过 `::` 使用：`QualityVerdict::Unknown`。

带数据的枚举（每个变体携带字段）下一章才会用到；本章两个枚举都只有名字。

## 3. Option：没有值也是一种值

```rust
let ended_at: Option<u64> = None;
let exit_code: Option<i32> = Some(0);
```

`Option<T>` 要么是 `Some(值)`，要么是 `None`。`Run` 用它表示“尚未结束的运行”：`ended_at`、`exit_code` 为 `None` 不是错误，是真实状态。

最值得注意的是 `verdict: Option<QualityVerdict>`：`None` 是尚未评估，`Some(QualityVerdict::Unknown)` 是评估过但证据不足。类型系统能保住这层区别——这是本章最重要的设计点。

比较 Option 不需要打开它：`assert_eq!(run.exit_code, Some(0))`。本章的库代码不用 `unwrap` 强制取值；测试里的 `unwrap` 表示“失败即测试失败”。

## 4. Vec：值的列表

```rust
let runs: Vec<Run> = Vec::new();
let runs = vec![run1, run2];
println!("共 {} 条运行", runs.len());
let first = &runs[0];
```

`Vec<T>` 是可增长列表；`vec!` 按顺序列出初始值。`runs[0]` 借用第一个元素，`len()` 返回数量。任务档案用 `Vec<Run>` 保存任意多条执行记录，测试里第二条运行的 `ended_at`、`exit_code` 都是 `None`，表示仍在执行。

## 5. derive：让编译器生成常规代码

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRecord { /* 字段 */ }
```

`#[derive(...)]` 让编译器按固定规则实现这些能力：

- `Debug`：可用 `{:?}` 打印，测试失败信息也靠它；
- `Clone`：`.clone()` 复制整个值；
- `PartialEq`：支持 `==` 与 `assert_eq!`；
- `Serialize` / `Deserialize` 来自 serde，见下一节。

结构体 derive 这些能力的前提是字段类型也拥有它们；`String`、`u32`、`Option`、`Vec` 和本章枚举都满足，所以 `TaskRecord` 可以一路派生。

## 6. serde 与 JSON

serde 的 `#[derive(Serialize, Deserialize)]` 让类型与 JSON 互转：

```rust
let json = serde_json::to_string(&record);        // 值 → JSON 文本
let restored: TaskRecord = serde_json::from_str(&json); // JSON 文本 → 值
```

两个函数都返回 `Result`，本章 main 用 match 处理，与第 2 章读取文件的写法相同。`from_str` 的返回类型由目标决定，所以 `: TaskRecord` 标注必不可少——去掉它，编译器会报告无法推断类型。

序列化规则：结构体字段名成为 JSON 键；`None` 变 `null`，`Some(x)` 变 `x`；`Vec` 变数组；枚举变字符串。`#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` 把 `BudgetExhausted` 写成 `BUDGET_EXHAUSTED`，让 JSON 词汇与设计文档一致（PASS / FAIL / UNKNOWN）。

往返（round-trip）指：值 → JSON → 值之后与原值相等。`assert_eq!(restored, record)` 依赖 `PartialEq`，这正是本章验收之一。

## 7. parse_contract 返回 Result

```rust
pub fn parse_contract(text: &str) -> Result<Contract, String>
```

`Result<成功类型, 失败类型>`：成功携带 `Contract`，失败携带 `String`（全部问题用 `；` 连接）。构造用 `Ok(value)` 与 `Err(message)`；读取用 match，main.rs 是完整示例。库里的 `?` 运算符（版本解析处）在出错时提前返回错误，第 5 章再系统学习错误传播。

## 8. 常见错误

- 忘记 derive `Serialize` 就调用 `to_string`：编译错误，提示类型没有实现 `Serialize`；
- 忘记 derive `PartialEq` 就写 `assert_eq!`：同样在编译期报错；
- `from_str` 缺少目标类型标注：编译器无法推断要生成什么类型；
- 手写 JSON 把 verdict 写成小写 `"pass"`：反序列化失败，serde 默认区分大小写。

## 9. 小练习

1. 给 `Run` 加一个 `producer` 字段（谁执行的：Codex / Claude Code / 用户），让往返测试继续通过。
2. 构造 `verdict` 为 `None` 的档案，序列化后找到 `"verdict": null`。
3. 故意删掉 derive 列表里的 `Deserialize`，运行 `cargo check` 读错误信息。
4. 把 `stop_reason` 设为 `Some(StopReason::BudgetExhausted)`，观察 JSON 变化。

[返回本章](README.md)
