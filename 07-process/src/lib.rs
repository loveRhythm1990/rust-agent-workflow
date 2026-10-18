//! 受控执行：固定程序与参数、限时、超时或取消后清理整个进程组。
//!
//! 本章执行的都是本地固定程序，不经过 shell 拼接；
//! 非零退出码与超时都记录为事实，而不是异常或崩溃。

use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

/// 一次受控执行请求：程序路径与参数都固定，不经过 shell。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRequest {
    pub program: String,
    pub args: Vec<String>,
}

impl ExecutionRequest {
    pub fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_string(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
        }
    }

    pub fn describe(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }
}

/// 一次执行的结局：所有可能的结果都是枚举值，没有一种是"异常"。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecOutcome {
    /// 正常结束，退出码 0。
    Completed,
    /// 正常结束，退出码非零：是事实，不是程序错误。
    Failed(i32),
    /// 被信号杀死（Unix 上退出码取不到）。
    Killed,
    /// 超过时间预算，进程组被终止。
    TimedOut,
    /// 主动取消，进程组被终止。
    Cancelled,
    /// 等待进程本身出了问题（极少见）。
    WaitFailed(String),
}

/// 执行记录：一次执行的运行事实。
#[derive(Debug, Clone)]
pub struct ExecutionRecord {
    pub outcome: ExecOutcome,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u128,
    pub pid: Option<u32>,
}

/// 一次受控执行：spawn 之后要么 settle（等结果或超时），要么 cancel。
pub struct Execution {
    child: Child,
    pid: Option<u32>,
    started: Instant,
    stdout_reader: JoinHandle<Vec<u8>>,
    stderr_reader: JoinHandle<Vec<u8>>,
}

impl Execution {
    /// 启动受控进程。stdin 关闭，stdout/stderr 收集；
    /// Unix 上子进程独立成为进程组组长，便于整组终止。
    pub fn spawn(request: &ExecutionRequest) -> Result<Execution, String> {
        let mut command = Command::new(&request.program);
        command
            .args(&request.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        command.process_group(0);
        match command.spawn() {
            Ok(mut child) => {
                let pid = child.id();
                Ok(Execution {
                    stdout_reader: pipe_reader(child.stdout.take()),
                    stderr_reader: pipe_reader(child.stderr.take()),
                    child,
                    pid,
                    started: Instant::now(),
                })
            }
            Err(error) => Err(format!("无法启动 {}: {}", request.describe(), error)),
        }
    }

    /// 等待结束，最多等 timeout；超时则终止整个进程组并记录 TimedOut。
    pub async fn settle(mut self, timeout: Duration) -> ExecutionRecord {
        let outcome = match tokio::time::timeout(timeout, self.child.wait()).await {
            Ok(Ok(status)) => match status.code() {
                Some(0) => ExecOutcome::Completed,
                Some(code) => ExecOutcome::Failed(code),
                None => ExecOutcome::Killed,
            },
            Ok(Err(error)) => ExecOutcome::WaitFailed(error.to_string()),
            Err(_elapsed) => {
                self.kill_group();
                let _ = self.child.wait().await;
                ExecOutcome::TimedOut
            }
        };
        self.into_record(outcome).await
    }

    /// 主动取消：立即终止整个进程组并记录 Cancelled。
    pub async fn cancel(mut self) -> ExecutionRecord {
        self.kill_group();
        let _ = self.child.wait().await;
        self.into_record(ExecOutcome::Cancelled).await
    }

    /// 终止整个进程组：负 pid 表示组而不是单个进程。
    fn kill_group(&mut self) {
        #[cfg(unix)]
        {
            if let Some(pid) = self.pid {
                // safety: kill(2) 只对自身派生的进程组发信号。
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = self.child.start_kill();
        }
    }

    async fn into_record(self, outcome: ExecOutcome) -> ExecutionRecord {
        let stdout = self.stdout_reader.await.unwrap_or_default();
        let stderr = self.stderr_reader.await.unwrap_or_default();
        ExecutionRecord {
            outcome,
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            duration_ms: self.started.elapsed().as_millis(),
            pid: self.pid,
        }
    }
}

/// 边跑边读管道：进程不退出，也要把已产出的输出收走；
/// 管道写满时进程会被卡住，并发读是受控执行的一部分。
fn pipe_reader<P>(pipe: Option<P>) -> JoinHandle<Vec<u8>>
where
    P: AsyncReadExt + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        match pipe {
            Some(mut pipe) => {
                let mut buffer = Vec::new();
                let _ = pipe.read_to_end(&mut buffer).await;
                buffer
            }
            None => Vec::new(),
        }
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn success_captures_stdout() {
        let request = ExecutionRequest::new("/bin/echo", &["captured"]);
        let record = Execution::spawn(&request)
            .expect("echo 应能启动")
            .settle(Duration::from_secs(5))
            .await;
        assert_eq!(record.outcome, ExecOutcome::Completed);
        assert_eq!(record.stdout.trim(), "captured");
        assert!(record.duration_ms < 5_000);
    }

    #[tokio::test]
    async fn nonzero_exit_is_recorded_not_fatal() {
        let request = ExecutionRequest::new("/usr/bin/false", &[]);
        let record = Execution::spawn(&request)
            .expect("false 应能启动")
            .settle(Duration::from_secs(5))
            .await;
        assert_eq!(record.outcome, ExecOutcome::Failed(1));
    }

    #[tokio::test]
    async fn timeout_records_and_returns_quickly() {
        let request = ExecutionRequest::new("/bin/sleep", &["40"]);
        let record = Execution::spawn(&request)
            .expect("sleep 应能启动")
            .settle(Duration::from_millis(300))
            .await;
        assert_eq!(record.outcome, ExecOutcome::TimedOut);
        assert!(record.duration_ms < 2_500);
    }

    #[tokio::test]
    async fn cancel_kills_the_whole_group() {
        // 双层进程不仅为了演示：外部工具本身就会再生子进程，
        // 生产路径同样不能靠"只杀直接子进程"。
        let request = ExecutionRequest::new("/bin/sh", &["-c", "sleep 30"]);
        let execution = Execution::spawn(&request).expect("sh 应能启动");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let record = execution.cancel().await;
        assert_eq!(record.outcome, ExecOutcome::Cancelled);
        // 给内核一小段时间收尸，再确认组里没有活进程。
        tokio::time::sleep(Duration::from_millis(200)).await;
        let check = ExecutionRequest::new("/usr/bin/pgrep", &["-f", "sleep 30"]);
        let check_record = Execution::spawn(&check)
            .expect("pgrep 应能启动")
            .settle(Duration::from_secs(5))
            .await;
        // pgrep 找不到进程时退出码为 1：这正是"无残留"的证据。
        assert_eq!(check_record.outcome, ExecOutcome::Failed(1));
    }

    #[tokio::test]
    async fn missing_program_reports_spawn_failure() {
        let request = ExecutionRequest::new("/definitely-not/a-real-program", &[]);
        let Err(message) = Execution::spawn(&request) else {
            panic!("不存在的程序应报告启动失败");
        };
        assert!(message.contains("无法启动"));
    }
}
