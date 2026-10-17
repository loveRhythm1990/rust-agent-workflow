# 第 6 章 Rust 配套：模块、接口建模、配置与错误传播

本章第一次把"对接外部工具"拆成库与配置：库负责类型与校验，配置承载选择。语法点都围绕这两件事展开。

## 1. 一包的模块化拆分

`lib.rs` 不再是一整块代码：委派和结果的结构体、错误枚举、`accept_result`、模拟适配器既在同一库里，职责又各自独立。这种"小、可见、可测"的库也符合计划：在真实命令约定还没定下来时，不着急写接口适配器。

## 2. 结构体先行，接口后做

离接口还远的时候，先定义数据形状：

```rust
pub struct Delegation {
    pub delegation_id: String,
    pub task_id: String,
    pub instruction: String,
    pub no_tools: bool,
}
```

Delegation 与 DelegationResult 将来要作为适配器的输入输出，首先成型的是这两种类型。建模的定义本身即是"这张牌就长啥样"的声明——id 必填、no_tools 不能省略，这张牌不需要再解释。

## 3. 配置不是常量：JSON 文件加载

```rust
let text = fs::read_to_string(path)?;
let config: AdapterConfig = serde_json::from_str(&text)?;
```

配置是张文件而不是埋在代码里的常量；文件路径从命令行参数接收。读取失败报错退出，JSON 不能全部解析时 serde 也给出具体错误——这两类失败都直面用户：

- 文件读不出来：路径无效或权限有问题；
- JSON 不合法：错误消息直指哪个字段不能适应类型。

名称无效被 refuse 对代码：

```rust
match config.adapter.as_str() {
    "simulated" => ...,
    other => {
        eprintln!("未知适配器 {}：本章只接通模拟，扩展留待后续。", other);
        ExitCode::FAILURE
    }
}
```

名字不认就不假装接通了——这是选型实验和第 7–10 章进接入阶段至关重要的门禁。

## 4. 错误传播：? 与错误枚举

委派可能失败的每一步都被命名：id 张冠李戴（MismatchedIds）、空结果（EmptyResult）、适配器自身失败（AdapterFailure）、配置不合法（Config）。"不当作成功"就是让这些路径都经由 `Err(...)` 继续传递，而不是 printf 掺混输出呈现差异。`?` 适合其中：遇到 `Err` 就散开返回结果。`map_err` 常用于包装下层错误：把转换内的错误包装为自己的枚举，这样 `?` 或 match 才一致工作。

## 5. 常见错误

- serde JSON 字段名错误：`from_str` 的错误消息里有哪个字段不能校验；可先把原始 JSON 打印出来再对照锁定问题；
- 用 `unwrap` 解 `from_str`：serde_json 可能失败，必须用 match 或 `?` 处理；
- 把 "unknown adapter" 当成成功：本章应把它在收尾之前拒绝；
- MismatchedIds 里 expected 与 got 顺序混掉：测试里把错误消息打印出来对照，胜过纯代码想象。

## 6. 小练习

1. 给 `Delegation` 增加一个 `review_instruction` 字段，实现"审查委派"，让编译、测试、演示整条路径都走通。
2. 在 adapter-config.json 里加一个未知字段，验证 serde 的容忍规则；再删除它并验证反序列化仍然一致。
3. 对照 lib.rs 已测试的行为，把 main.rs 的错误路径都补到标准错误输出上，并在文档中记上错误细节。

[返回本章](README.md)
