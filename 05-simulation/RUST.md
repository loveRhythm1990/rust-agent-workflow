# 第 5 章 Rust 配套：Result、错误传播与测试替身

本章首次让函数给出可携带原因的错误，也第一次用测试替身代替还没实现的能力。语法点都围绕这两件事展开。

## 1. 用 Result 让函数失败得有原因

```rust
pub fn run_simulation(
    task: &Task,
    script: &Script,
    max_fixes: u32,
) -> Result<SimulationReport, SimulationError> {
```

三种失败都已命名：`Transition(String)`、`DuplicateCheck(String)`、`ScriptExhausted { attempt }`。前两种各带一条信息，第三种带一组命名字段——枚举每个变体都可以有自己的载荷。调用方用 match 或 `?` 来处理，`Err` 不会被 `unwrap` 隐掉。

给错误实现 `Display` 是为了打印得像个正常人，而不是一堆 Debug 结构；`fmt` 签名是固定的，内部 match 到各变体再 `write!` 到格式化器。

## 2. `?` 运算符：对就继续，错就返回

编写“走到某步失败即停”的流程，是在两个函数之间套一层 `?`：

```rust
fn step(from: TaskStatus, to: TaskStatus) -> Result<TaskStatus, SimulationError> {
    transition(from, to).map_err(SimulationError::Transition)
}

if status == TaskStatus::Planned {
    status = step(status, TaskStatus::Implementing)?;
    status = step(status, TaskStatus::Verifying)?;
} else {
    status = step(status, TaskStatus::Fixing)?;
    status = step(status, TaskStatus::Verifying)?;
}
```

`map_err` 把下层错误的 `String` 换成自己的 `Transition(String)`；`?` 等价于“出错就 return Err(...)”。因为函数返回的是 `Result<_, SimulationError>`，两种地方的错误类型一致，`?` 才能直接混用。省略 `?` 就是另外一章那种预演——check 完先记错、内置处理，而不是提前返回。

## 3. loop 作为表达式：循环给回最终值

```rust
let (verdict, stop_reason) = loop {
    ...
    match action {
        NextAction::Ready => {
            ...
            break (verdict, Some(reason));
        }
        ...
    }
};
```

`loop` 的返回值就是 `break` 携带的值；`break` 在不同分支带不同类型会编译错。这样最终结论来自一个清楚的控制顶点，而不是一堆“最后写哪个字段”的隐患。

## 4. 用 HashSet 校验重复

同一轮 attempt 同一个验收项出现两次，逻辑上会同时出现在 passed 和 failed——这一定是脚本写错了。办法是每遇到一项先尝试插入集合：

```rust
if !seen.insert(check.name.as_str()) {
    return Err(SimulationError::DuplicateCheck(check.name.clone()));
}
```

`HashSet::insert` 返回是否插入了新元素：已存在时返回 false。借 `&str` 进出集合，克隆只在出错时才分配。Vec 是“有顺序的列表”，HashSet 是“没有顺序的唯一集合”；本章用后者当作重复检测的活名单。

## 5. 测试不是只有 assert 写出来才叫测试

本章的错误用 `matches!` 取模式而不是逐一 unwrap：

```rust
assert!(matches!(
    run_simulation(&task(), &script, 0),
    Err(SimulationError::DuplicateCheck(_))
));
```

`matches!` 在编译期展开成 match 那种穷尽检查，匹配到即真。`let ... else` 用于“反着来”的分支：`let Err(error) = ... else { panic!(...) };`——error 留在后头继续对照。`attempt(...)` 辅助函数把 `&[(&str, CheckResult)]` 变成 `AttemptPlan`，是手工造替身时常用的“小构造器”。

## 6. 测试替身是什么、不是什么

`Script` 就是测试替身：用最小的可控制行为替代后续章节才会有的真实 Agent。它不假装产生证据、不访问网络和模型、不会改变状态机规则。把它换成真实委托前——第 7 章进程、第 10 章双 CLI——规则的正确性要靠它先证明。模拟替身与真实执行共享的，是第 3 章模型和第 4 章逻辑；不共享的——账号、网络、进程——只能在那些章节里被新增的证据补上。

## 7. 常见错误

- 两种 `Err` 类型直接混用：函数里 `?` 遇到不同类型会报“类型不匹配”；用 `map_err` 或一个统一错误枚举去对齐；
- 给集合插 `&str` 忘了 `.as_str()`：会得到“期望 &str，found &String”的类型错误；
- main 的 match 分支类型不一致：一条回 `()`、一条回 `ExitCode`——让所有分支各自返回 `ExitCode` 或同一种结果即可；
- 测试靠 `unwrap` 等到运行时才发现枚举错配：用 `matches!` 或 `assert_eq!` 精确到变体。

## 8. 小练习

1. 把 `run_simulation` 的 `?` 删除三个，看 cargo check 在哪里报错、报什么，改回来为止。
2. 让某次验收故意重复登记一项，只改脚本数据，观察 DuplicateCheck 出现的路径。
3. 把场景二的脚本改成“FAIL 再 FAIL 再 FAIL”，`max_fixes = 2`，依次记下 `attempts_used` 与最终的 stop_reason。

[返回本章](README.md)
