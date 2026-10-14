# 第 4 章 Rust 配套：match、模块与纯函数

本章代码分在两个模块文件里，用 match 写决策规则。先看 `status.rs`（状态与允许表），再看 `decision.rs`（结论与动作），最后看 main 的三个 match 示例。

## 1. 模块：一个文件，一个职责

```rust
mod decision;
mod status;

pub use decision::{decide, plan_next, NextAction, VerificationSummary};
pub use status::{transition, TaskStatus, ALLOWED_TRANSITIONS};
```

`src/status.rs` 就是模块 `status`，文件名即模块名；`decision.rs` 同理。`mod` 声明后，模块内项需要 `pub` 才能对外可见。`pub use` 把模块里的项重新导出到包根部，因此外面可以写 `chapter_04_state_machine::TaskStatus`，而不必写 `chapter_04_state_machine::status::TaskStatus`。

测试也随之分组：`cargo test` 输出里的 `status::tests::...` 与 `decision::tests::...` 直接对应文件。

## 2. match：穷尽性是免费的检查

```rust
pub fn plan_next(verdict: &QualityVerdict, fixes_remaining: u32) -> NextAction {
    match verdict {
        QualityVerdict::Pass => NextAction::Ready,
        QualityVerdict::Fail if fixes_remaining > 0 => NextAction::Fix,
        QualityVerdict::Fail => NextAction::Stop(StopReason::BudgetExhausted),
        QualityVerdict::Unknown => NextAction::Stop(StopReason::EvidenceMissing),
    }
}
```

match 逐个尝试分支，没有匹配时编译错误。因为分支列全了 `QualityVerdict` 的三个变体，这里不需要 `_` 通配分支——将来给枚举加变体，漏掉处理会直接编译失败，错误不会静默溜进运行期。这是“用枚举建模决策”的最大收益。

`Fail if fixes_remaining > 0` 是带条件的分支（match guard）：先匹配变体，再检查条件；条件不满足落到下一个 `Fail` 分支。分支按顺序检查，守卫分支必须写在无守卫分支前面。

main.rs 的 `verdict_name` 展示 match 直接作为返回值：每个分支产出一个 `&'static str`。

## 3. 匹配带数据的枚举

第 4 章的新枚举 `NextAction` 有一个带数据的变体：

```rust
pub enum NextAction {
    Ready,
    Fix,
    Stop(StopReason),
}
```

`Stop(StopReason)` 的括号里带一个值。main.rs 里这样取出来：

```rust
match action {
    NextAction::Ready => "READY（候选可交付）".to_string(),
    NextAction::Fix => "FIXING（进入修复）".to_string(),
    NextAction::Stop(reason) => format!("STOPPED（{}）", stop_name(reason)),
}
```

匹配 &NextAction 时，`reason` 自动绑定成 &StopReason——不需要写 `&NextAction::Stop(reason)` 或解引用。这叫匹配自适应：match 目标是引用时，绑定变量也是引用。

## 4. 借用够用，就不交所有权

写 `show_decision` 时先遇到一个编译错误：`plan_next(verdict, ...)` 把 verdict 移走后，下一行 `verdict_name(verdict)` 不能再用。解法是让函数借用：

```rust
let verdict = decide(summary);
let action = plan_next(&verdict, fixes_remaining); // 只读结论
println!("{}", verdict_name(&verdict));            // 还能用
```

与第 2 章“校验函数借用文本”同理：只是读取的参数用引用传递；`decide(&VerificationSummary)`、`plan_next(&QualityVerdict, ...)`、`transition` 内部对比，全部只是读。所有权留在调用方，函数用完各回各家。

## 5. 常量表与切片

```rust
pub const ALLOWED_TRANSITIONS: &[(TaskStatus, TaskStatus)] = &[ ... ];
```

`const` 在编译期确定；这里的类型是“对元组数组的引用”。`&[..]` 字面量直接写数组，编译器给出整个生命周期固定的引用，不需要指定长度。查询两步：

```rust
ALLOWED_TRANSITIONS.contains(&(from, to))          // 整表查找
ALLOWED_TRANSITIONS.iter().any(|(source, _)| *source == from) // 只要来源匹配就算有出边
```

`contains` 逐项比较，`&` 取地址以匹配参数类型（`&T`）；`iter().any()` 里闭包的参数模式 `(source, _)` 直接解包元组，`_` 表示不需要第二个值。

## 6. 纯函数与测试

`decide`、`plan_next`、`transition` 都是纯函数：不读环境、不写文件、不产生随机，同样的输入必得同样的输出。这带来两个直接收益：测试不需要准备环境；规则可以被第 5 章模拟器和后续真实执行同用。

测试模块的辅助函数展示了一种表驱动风格：

```rust
fn summary(passed: &[&str], failed: &[&str], unverified: &[&str]) -> VerificationSummary {
    let to_owned =
        |lines: &[&str]| lines.iter().map(|line| line.to_string()).collect::<Vec<_>>();
    ...
}
```

调用处只需写字面量：`summary(&["原有输出保留"], &[], &[])`。`map` 把每个 `&str` 变成 `String`，`collect::<Vec<_>>` 收集成 Vec——尖括号里只写容器，元素类型由编译器推断。断言用 `assert_eq!` 直接比较两个枚举或 `Result`；`let Err(..) else { panic!() }`（见 status.rs 测试）在“希望它是 Err”时反向断言。

## 7. 常见错误

- match 少写一个变体分支：编译错误列出缺失变体——这正是想要它发生的时刻；
- 给 `contains` 传 `&(from, to)` 忘了 `&`：类型不匹配，编译器会提示期望 `&(TaskStatus, TaskStatus)`；
- 模块里的项忘了 `pub`：外部路径访问报“私有”错误；
- 在守卫分支后面写无守卫 `Fail` 分支顺序颠倒：守卫永远不触发，测试会抓住它。

## 8. 小练习

1. 删掉 plan_next 的 `Unknown` 分支，运行 `cargo check` 看穷尽性报错，再补回来。
2. 给 `ALLOWED_TRANSITIONS` 加一条 `(Ready, Cancelled)` 之外的新错误边（如 `Ready -> Fixing`），让全部测试仍通过——思考这条边在业务上说不说得通，测试为什么没拦住它。
3. 用 match 改写 `is_terminal`，让它对终态返回 true、活跃状态返回 false，保持测试通过。

[返回本章](README.md)
