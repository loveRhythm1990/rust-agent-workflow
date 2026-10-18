use chapter_07_process::{ExecOutcome, Execution, ExecutionRequest};
use std::process::ExitCode;
use std::time::Duration;

#[tokio::main]
async fn main() -> ExitCode {
    println!("本章演示受控执行：固定程序与参数、限时、整组清理。\n");

    println!("== 实验一：固定命令成功 ==");
    let request = ExecutionRequest::new("/bin/echo", &["第 7 章：受控执行"]);
    match Execution::spawn(&request) {
        Ok(execution) => {
            let record = execution.settle(Duration::from_secs(5)).await;
            println!("程序：{}", request.describe());
            println!(
                "结果 {}，用时 {}ms",
                describe_outcome(&record.outcome),
                record.duration_ms
            );
            println!("stdout：{}", record.stdout.trim());
        }
        Err(message) => println!("启动失败：{}", message),
    }

    println!("\n== 实验二：非零退出码被记录 ==");
    let request = ExecutionRequest::new("/usr/bin/false", &[]);
    match Execution::spawn(&request) {
        Ok(execution) => {
            let record = execution.settle(Duration::from_secs(5)).await;
            println!("程序：{}", request.describe());
            println!(
                "结果 {}，用时 {}ms",
                describe_outcome(&record.outcome),
                record.duration_ms
            );
            println!("非零退出码是运行事实：记录下来交给决策，而不是当场崩溃。");
        }
        Err(message) => println!("启动失败：{}", message),
    }

    println!("\n== 实验三：超时终止整组 ==");
    let request = ExecutionRequest::new("/bin/sleep", &["10"]);
    match Execution::spawn(&request) {
        Ok(execution) => {
            let record = execution.settle(Duration::from_secs(1)).await;
            println!("程序：{}（预算 1 秒）", request.describe());
            println!(
                "结果 {}，用时 {}ms",
                describe_outcome(&record.outcome),
                record.duration_ms
            );
            println!("预算耗尽即终止：进程组按时清理，不等待进程自觉退出。");
        }
        Err(message) => println!("启动失败：{}", message),
    }

    println!("\n== 实验四：取消并验证无残留 ==");
    // 生产路径只跑固定程序；这里用 /bin/sh 派生孙进程，
    // 专门验证"整组清理"对会再生子进程的工具同样成立。
    let request = ExecutionRequest::new("/bin/sh", &["-c", "sleep 30"]);
    match Execution::spawn(&request) {
        Ok(execution) => {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let record = execution.cancel().await;
            println!("程序：{}（500ms 后取消）", request.describe());
            println!(
                "结果 {}，用时 {}ms",
                describe_outcome(&record.outcome),
                record.duration_ms
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
            let check = ExecutionRequest::new("/usr/bin/pgrep", &["-f", "sleep 30"]);
            let check_record = Execution::spawn(&check)
                .expect("pgrep 应能启动")
                .settle(Duration::from_secs(5))
                .await;
            match check_record.outcome {
                ExecOutcome::Failed(1) => {
                    println!("残留检查：pgrep 未发现任何 sleep 30，整组清理通过。")
                }
                _ => {
                    println!("残留检查：{}", describe_outcome(&check_record.outcome));
                    println!("stdout：{}", check_record.stdout.trim());
                    eprintln!("发现残留子进程，演示失败。");
                    return ExitCode::FAILURE;
                }
            }
        }
        Err(message) => println!("启动失败：{}", message),
    }

    println!("\n== 实验五：启动失败如实报告 ==");
    let request = ExecutionRequest::new("/definitely-not/a-real-program", &[]);
    match Execution::spawn(&request) {
        Ok(_) => println!("不该成功。"),
        Err(message) => println!("启动失败：{}（不会伪装成执行成功）", message),
    }

    println!();
    println!("五种结局都被记录：Completed、Failed、TimedOut、Cancelled、启动失败。");
    println!("真实 CLI 的受控调用在第 10 章接入；本章机制先行完成验收。");
    ExitCode::SUCCESS
}

fn describe_outcome(outcome: &ExecOutcome) -> String {
    match outcome {
        ExecOutcome::Completed => "Completed（退出码 0）".to_string(),
        ExecOutcome::Failed(code) => format!("Failed（退出码 {}）", code),
        ExecOutcome::Killed => "Killed（被信号终止）".to_string(),
        ExecOutcome::TimedOut => "TimedOut（超时，进程组已终止）".to_string(),
        ExecOutcome::Cancelled => "Cancelled（取消，进程组已终止）".to_string(),
        ExecOutcome::WaitFailed(detail) => format!("WaitFailed（{}）", detail),
    }
}
