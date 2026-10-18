# 第 7 章 Rust 配套：异步、Tokio 与进程控制

本章第一次写异步代码。围绕“等一个进程，同时看着时钟”这条主线，认识 async/await、tokio 和进程组。

## 1. 为什么同步代码做不了超时

同步地 `child.wait()` 会一直等到进程退出，没有机会看表。老办法是轮询或另开线程；现在的标准做法是异步：`tokio::time::timeout(duration, future)` 表示“最多等 future 这么久，到点没完成就返回 Err”。本章的 `settle` 核心就一行模式：

```rust
match tokio::time::timeout(timeout, self.child.wait()).await {
    Ok(status) => ...          // 预算内完成
    Err(_elapsed) => ...       // 超预算
}
```

`child.wait()` 本身是个 future；`.await` 把控制权交还给运行时（runtime），timeout 在同一个等待里替你看表。

## 2. async fn 与 #[tokio::main]

```rust
#[tokio::main]
async fn main() -> ExitCode { ... }
```

`async fn` 的调用返回一个 future，必须 `.await` 才真正运行；`.await` 点表示“等这件事完成，期间别的任务可以跑”。`main` 不是 async fn，所以用 `#[tokio::main]` 宏在入口处搭一个运行时。测试里对应的是 `#[tokio::test]`。

## 3. tokio 的 Command（和 std 的区别）

`tokio::process::Command` 用法和 `std::process::Command` 几乎一样（`new`、`args`、`stdin/stdout/stderr`），但有三个关键差异：

1. 它的 `Child::wait()` 是异步的，可以被 timeout 包住；
2. `Child::id()` 返回 `Option<u32>`——进程被收割后 pid 失效，类型直接表达“可能没有了”；
3. 构造配置时可用 `process_group(0)`（Unix）：让子进程自己当组长，之后 `kill(-pid)` 一发信号整组消失。

负 pid 的 kill 是 Unix 语义，不是 tokio 的功能：`libc::kill(-(pid as i32), libc::SIGKILL)`。`libc` crate 提供裸的系统调用封装；那行代码是 unsafe，因为 Rust 无法证明你杀的组是你自己的组——本章在注释里写明这个约定。

## 4. 并发读管道：tokio::spawn

进程往 stdout 写、我们读，两边速度不一致时靠管道缓冲区协调；缓冲区有限，进程写满还没人读就会卡死。所以读是并发任务：

```rust
let stdout_reader = tokio::spawn(async move {
    match pipe {
        Some(mut pipe) => {
            let mut buffer = Vec::new();
            let _ = pipe.read_to_end(&mut buffer).await;
            buffer
        }
        None => Vec::new(),
    }
});
```

`tokio::spawn` 启动一个并发任务，返回 `JoinHandle<Vec<u8>>`；拿到结果要 `.await`，得到 `Result<Vec<u8>, JoinError>`，本章用 `unwrap_or_default()`：任务失败（几乎不可能）就当作空输出。管道读不到结尾（进程被杀）时 `read_to_end` 会正常返回已收到的部分，不会报错——这正好符合“杀掉进程也要拿到已产生的输出”的需求。

注意 `pipe_reader` 是泛型函数：`P: AsyncReadExt + Unpin + Send + 'static`。stdout 和 stderr 是两种类型，泛型让一个函数服务两种管道。这些约束的本义：能异步读（AsyncReadExt）、地址固定（Unpin）、能搬到别的线程（Send + 'static）。第八章会正式讲 trait，这里先记住“P 是满足这组要求的任何管道类型”。

## 5. Duration 与 Instant

`std::time::Duration::from_secs(1)` / `from_millis(300)` 是时间长度；`Instant::now()` 加 `elapsed()` 测时间点之间的距离，`as_millis()` 得到 u128。时间戳现在只是数字，没有时区语义——真要与真实时间对账是第 14 章的事。

## 6. 常见错误

- 在非 async 函数里写 `.await`：编译器直接拒绝；要么把函数改成 async，要么在 `#[tokio::main]` 的 main 里调用；
- 用了 `std::process::Command` 却当 tokio 的用：`wait()` 类型对不上，错误信息会提示两个 `Child` 是不同类型——本章作者第一次就踩了这坑：`use std::process::Command` 应为 `use tokio::process::Command`；
- 对 `timeout` 的返回值只 match 一层：它的 Ok 里还包着 wait 的 `io::Result`，正确写法是 `Ok(Ok(status))`、`Ok(Err(error))`、`Err(_elapsed)` 三分支；
- 杀完进程不等收割：`kill_group` 之后要再 `wait().await` 一次，让孩子进程被“收尸”，否则留下僵尸进程条目。

## 7. 小练习

1. 把 `settle` 的超时分支改成不杀进程直接返回，跑实验三，然后 `pgrep sleep` 看看留下了什么（看完记得改回来并手动清理）。
2. 给 `ExecutionRecord` 加一个 `finished_at_unix: u64` 字段，从 `Instant` 换算或另取 `SystemTime`，想一想哪个对“运行记录”更合适。
3. 把五个实验的输出改成一段 JSON（serde 可用），与第 3 章 `Run` 的字段对照：哪些字段两边都有，为什么？

[返回本章](README.md)
