# 第 8 章 Rust 配套：trait、泛型与动态分发

本章第一次定义 trait，也第一次在 trait 对象和泛型之间做选择。围绕“同一套调用与校验服务所有适配器”展开。

## 1. trait：一组行为的约定

```rust
pub trait Agent {
    fn name(&self) -> &str;
    fn call(&self, request: &AgentRequest) -> Result<String, AgentError>;
}
```

trait 声明“实现我的类型必须会做什么”，不关心具体怎么做。`ScriptedAgent` 实现它返回脚本内容；将来第 10 章的 `PiAgent`、`CodexAgent` 实现它去真的调进程。定义方法用 `impl Trait for Type`：

```rust
impl Agent for ScriptedAgent {
    fn name(&self) -> &str { &self.name }
    fn call(&self, request: &AgentRequest) -> Result<String, AgentError> { ... }
}
```

`&self` 表示“方法借用实例自身”；实例里未被 move 的字段照常可用。

## 2. 泛型参数：编译期的“按约定调用”

```rust
pub fn run_with<A: Agent + ?Sized>(
    agent: &A,
    request: &AgentRequest,
) -> Result<AcceptedReport, AgentError>
```

`A: Agent` 是trait bound：任何实现 Agent 的类型都行，编译器为每个具体类型生成一份专用代码（静态分发）。`?Sized` 是补充说明——A 不一定是固定大小类型，也可以是 trait 对象（见下节）。

调用时不需要写 `run_with::<ScriptedAgent>(...)`，类型由参数自动推断。

## 3. trait 对象：运行时替换

```rust
pub struct AgentBinding {
    pub agent: Box<dyn Agent>,
    pub session_id: String,
}
```

`Box<dyn Agent>` 是“堆上的某个 Agent 实现”——具体是哪个，运行时才定。这带来两个成本：

- 动态分发：每次方法调用查虚表，比泛型的静态分发慢一点（本章完全无感）；
- 编译期检查变弱：?Sized 类型的 `&dyn Agent` 没有通用大小，只能借引用用。

为什么好处也真实：`AgentBinding` 可以装配任何实现，测试换掉适配器、运行时换工具，`run` 方法一行不改。练习里的“两个适配器互换”测试就是这个性质的可执行证明。

`self.agent.as_ref()` 给出 `&dyn Agent`，落进 `run_with::<dyn Agent>`——这要求泛型版本声明 `?Sized`，两种写法因此汇合在同一条路径上。

## 4. serde 结构：从“解析配置”到“定义契约”

第 3、6 章的解析是反序列化成类型再校验；本章更进一步——`AgentReport` 的 serde 定义本身就是报告的 JSON 契约：

```rust
#[derive(..., Serialize, Deserialize)]
pub struct AgentReport {
    pub delegation_id: String,
    ...
    pub status: ReportStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}
```

- 字段名即 JSON 键；类型即取值约束（`status` 只认 COMPLETED / FAILED，别的值解析失败）；
- `skip_serializing_if` 让 None 的字段不写入输出，可读性更好；解析时字段缺失当 None；
- 缺字段、多值、类型错误都由 serde 在“进入类型系统”那一刻拦下，我们的校验器只处理语义层剩下的问题（身份、空内容、缺原因）。

`Fault::MissingStatus` 的实现顺带演示了另一件常事：用 `serde_json::to_value` 拿到 `serde_json::Value`（任意 JSON 的枚举表示），再改这个值、序列化回字符串——生成“接近合法但差一点”的测试报文，比手写 JSON 字符串可靠。

## 5. 结构体更新语法与 From 转换

报文构造器的两种简写：

```rust
serde_json::to_string(&AgentReport {
    summary: String::new(),
    findings: Vec::new(),
    ..report          // 其余字段照抄 report
})
```

`..report` 把没列出的字段从 report 复制/搬走；列出的字段优先。配合 `impl From<serde_json::Error> for AgentError`，函数里的 `?` 自动把 serde 错误转成语义错误——魔法的名字叫 `From` trait + `?` 运算符（第 5 章的老朋友，这次是我们自己提供了转换规则）。

## 6. 常见错误

- 给 trait 方法写了返回类型却不返回值：接口实现必须精确匹配签名；
- 忘写 `?Sized`：`Box<dyn Agent>` 传给泛型函数时报“大小不满足”的错误，错误信息会提到 Sized；
- `..report` 之后再使用 report：被 move 走了；需要重复使用就先 `.clone()`；
- 在 serde 结构里混用 `skip_serializing_if` 与自定义 Default：字段缺失时反序列化会拿 None，而不是你以为的默认值。

## 7. 小练习

1. 把 `run_with` 的泛型版删掉，只保留 `&dyn Agent` 版本的 `AgentBinding`，跑测试；再把方向反过来，观察哪些调用点需要改动——这是两种分发的取舍实感。
2. 给 `Agent` 增加一个带默认实现的方法 `describe(&self) -> String`，已有实现不用改就能用——这是默认方法。
3. 结合第 7 章：想一个真实 `PiAgent::call` 的实现要考虑的事（写个清单：进程启动、超时取消、JSON 流读取、报告映射），本章哪些已就绪、哪些还缺？

[返回本章](README.md)
