# 第 1 章：Cargo 与第一个程序

这一章先建立最小的学习循环：读代码 → 运行 → 修改 → 检查。完成后，你应能找到程序入口，解释 Cargo 文件的作用，修改变量，并调用一个函数。

## 1. Cargo 是什么

Rust 编译器 `rustc` 将 Rust 代码编译成程序。Cargo 管理项目、依赖、编译、运行与测试，日常开发通常从 Cargo 开始。

先确认工具链可用：

```bash
rustc --version
cargo --version
```

从零创建独立项目的方法是 `cargo new hello`，它会创建配置文件和 `src/main.rs`。本仓库已经准备好示例，无需再次创建。

## 2. 运行本章

在仓库根目录执行：

```bash
cargo run -p chapter-01-cargo
```

`-p` 指定包名，包名来自本章 Cargo.toml 的 `name`，不是目录名。Cargo 先编译，再运行；输出中编译进度属于 Cargo，以下四行属于我们的程序：

```text
你好，Rust！
项目：rust-agent-workflow
第 1 章：Cargo 与第一个程序
学习目标：用 Rust 构建可验证的多 Agent 协作工具
```

也可以进入本章目录运行：

```bash
cd 01-cargo
cargo run
```

## 3. 文件各自负责什么

```text
rust-agent-workflow/
├── Cargo.toml          # workspace：列出已实现的成员包
├── Cargo.lock          # Cargo 自动维护的版本锁定文件
├── target/             # 运行编译后生成，Git 忽略它
└── 01-cargo/
    ├── Cargo.toml      # 本章包名、版本、语言 edition 与依赖
    ├── README.md       # 本章教程
    └── src/main.rs     # 本章程序入口
```

workspace 让多个章节共用管理入口和编译产物目录。本章没有第三方依赖，但 Cargo 仍会生成 Cargo.lock。提交锁定文件便于后续依赖加入时固定解析结果，通常不手工编辑。

本章配置：

```toml
[package]
name = "chapter-01-cargo"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
```

`version` 是我们这个包的版本。`edition` 是 Rust 的语言兼容规则版本，不是本机编译器版本；2024 edition 要求 Rust 1.85 或更新版本。`publish = false` 禁止向包注册表发布，不影响 GitHub 仓库公开。[dependencies] 暂时为空。

## 4. 阅读程序

打开 [src/main.rs](src/main.rs)。核心写法是：

```rust
fn main() {
    let project_name = "rust-agent-workflow";
    let chapter = 1;

    println!("你好，Rust！");
    println!("项目：{}", project_name);
    println!("第 {} 章：Cargo 与第一个程序", chapter);

    print_learning_goal();
}
```

- `fn` 定义函数，`main` 是程序入口，花括号包住函数体。
- `let` 创建变量。Rust 可从初始值推断类型；这两个变量暂时不必显式写类型。
- `println!` 打印一行文本。感叹号说明它是宏；本章先掌握使用方式。
- `{}` 是格式化占位符。中文字符串可以直接写在源码中。
- `print_learning_goal()` 调用另一个函数。示例中的语句以分号结束。

默认变量不可重新赋值。如果要修改数字，需要明确写 `mut`：

```rust
let mut chapter = 1;
chapter = 2;
println!("第 {} 章", chapter);
```

可以将这段代码暂时替换原来的章节变量及对应输出来观察变化。不要直接把 `chapter = 2;` 加到原来的不可变变量后面；编译器会指出错误。读懂错误信息也是学习过程的一部分。

字符串的所有权和借用将在第二章展开，本章只使用字符串字面量。

## 5. 常用命令

以下命令都在仓库根目录执行：

| 命令 | 作用 |
| --- | --- |
| `cargo check -p chapter-01-cargo` | 快速检查编译错误，不生成最终可执行文件 |
| `cargo build -p chapter-01-cargo` | 编译程序 |
| `cargo run -p chapter-01-cargo` | 编译并运行 |
| `cargo fmt --all` | 格式化代码 |
| `cargo test -p chapter-01-cargo` | 运行测试；本章尚无测试用例 |

本章是输出示例，不为固定问候语添加测试。`cargo test` 显示 0 个测试不代表功能已经验收；运行输出与下面的练习才是本章的检查依据。

## 6. 动手练习

1. 将问候语改成你自己的文字，运行确认输出变化。
2. 将 `chapter` 改成 2，确认占位符输出新数字。
3. 新建 `print_next_step` 函数，输出“下一步：学习所有权与借用”，然后在 main 中调用。
4. 尝试给不可变变量重新赋值，阅读编译错误；再加入 `mut`，检查是否成功。

练习后可用 `git diff` 查看修改。保留你的学习成果，无需重置文件。

## 7. 完成标准

- 原始示例正常运行，输出与本章展示一致。
- 能说明根目录与章节目录的 Cargo.toml 各自作用。
- 能修改变量、添加函数，并通过 `cargo check`。
- 能解释“编译成功”和“行为符合预期”需要分别检查。

[返回学习路线](../README.md) · [下一章：任务输入与字符串处理](../02-ownership/README.md)（规划中）
