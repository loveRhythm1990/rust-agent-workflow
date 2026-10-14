# 第 2 章 Rust 配套：字符串、所有权与借用

本章代码比第一章长。先理解“读取文本 → 借用校验 → 保留原文本输出”这条路径，再阅读字段解析和命令行错误处理。无需一次记住所有语法。

## 1. String 与 &str

```rust
let contract = String::from("目标: 输出问候");
let view: &str = &contract;
println!("{}", view);
```

`String` 拥有一段可增长的 UTF-8 文本；`&str` 是对文本的借用视图。字符串字面量也可以作为 `&str`。读取文件返回 String，我们的校验函数接收 &str，因此文件文本和字面量都能用于校验。

源码的 `.to_string()` 将内嵌示例转换为拥有的 String。中文不需要特殊转义；不要用整数下标直接访问字符串，UTF-8 中字符不等于单个字节。

## 2. 所有权移动

```rust
let original = String::from("任务");
let moved = original;
println!("{}", moved);
```

赋值后文本的所有者变成 moved。若再使用 original，编译器会报告它已被移动。函数按值接收 String 时也可能发生移动。

如果只是读取，不必复制文本：

```rust
fn has_text(text: &str) -> bool {
    !text.trim().is_empty()
}

let contract = String::from("任务");
println!("{}", has_text(&contract));
println!("{}", contract);
```

`&contract` 借用文本；调用后原 String 仍然可用。这就是 main 校验后还能打印契约的原因。`trim()` 返回忽略首尾空白的视图，不修改原文本；`!` 将布尔值取反。

## 3. 函数参数与返回值

`fn validate_contract(contract: &str) -> Vec<String>` 表示借用文本并返回一组拥有的错误信息。调用者不需要保持输入文本来使用这些错误字符串。

```rust
fn message() -> String {
    "需要填写预期结果".to_string()
}
```

函数末尾表达式没有分号，作为返回值。提前返回写 `return value;`。如果给上面的末尾表达式加分号，会返回 `()`，与声明的 String 不匹配。

## 4. 错误列表与循环

`Vec<String>` 是可增长的字符串列表；这是本章需要的少量集合知识，正式任务类型在下一章学习。

```rust
let mut errors: Vec<String> = Vec::new();
errors.push("目标不能为空".to_string());
for error in &errors {
    println!("{}", error);
}
```

`mut` 允许更新列表；`push` 添加元素。`for error in &errors` 借用元素，循环后仍可使用列表。main 的 `for error in errors` 消耗列表，适合输出后立即退出。

## 5. 从文本到字段

`lines()` 按行遍历，`split_once(':')` 在第一个英文冒号处分成字段名和值。它可能找不到冒号，因此返回 `Option`：`Some` 表示有值，`None` 表示缺失。

源码使用 `match` 区分合法字段和其他输入；`enumerate()` 同时给出行号索引，索引从 0 开始，显示时加 1。`format!` 构造 String，`println!` 则打印文本。

字段值通过 `filter_map`、`filter`、`map` 和 `collect` 收集：依次丢弃无法分割的行、筛选字段、取得值、构造列表。可先按这个顺序读，不必立即自己写链式表达式。`|line| ...` 是闭包，可理解为传给遍历操作的小函数。

## 6. 为什么 field 有一个 'a

源码签名：`fn field<'a>(contract: &'a str, key: &str) -> Option<&'a str>`。

返回值借用 contract 中的一部分，`'a` 告诉编译器返回视图的有效期来自 contract，而不是 key。它不会延长文本的实际寿命。本章只需理解：不能返回指向已销毁的临时 String 的引用。下一章不要求先掌握复杂生命周期设计。

## 7. 读取文件可能失败

`fs::read_to_string` 返回 `Result`：成功是 `Ok(text)`，失败是 `Err(error)`。main 明确处理两条分支，读取失败时输出原因并结束。

库函数中的 `?` 用于 Option：遇到 None 就提前返回 None；`parse().ok()` 将解析失败转换成 None。这里不同于 panic，不会因非法版本直接崩溃。版本使用 u32，`checked_add` 在溢出时返回 None。

`ExitCode` 表示程序退出状态：0 为成功，1 为契约拒绝，2 为调用或文件读取错误。这些状态描述本章校验器，不是未来系统的 PASS / FAIL / UNKNOWN。

## 8. 小练习

1. 在临时 Rust 示例中运行所有权移动代码，再尝试打印 original，读懂报错。
2. 运行借用示例，确认校验后文本仍能打印。
3. 给新契约字段加空白，比较 trim 前后的行为。
4. 将版本写成中文或 0，检查程序给出的错误。

建议先改 examples 中的任务文本，再修改校验代码。语法练习使用独立临时文件，不必改变正式契约规则。

[返回本章](README.md)
