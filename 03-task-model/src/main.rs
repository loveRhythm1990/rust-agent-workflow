use chapter_03_task_model::{QualityVerdict, Run, Task, TaskRecord, parse_contract};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() > 1 {
        eprintln!("用法：chapter-03-task-model [契约文件]");
        return ExitCode::from(2);
    }
    let contract_text = if let Some(path) = args.first() {
        match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("无法读取契约：{}", error);
                return ExitCode::from(2);
            }
        }
    } else {
        include_str!("../examples/task.txt").to_string()
    };

    let contract = match parse_contract(&contract_text) {
        Ok(contract) => contract,
        Err(problems) => {
            eprintln!("输入门禁拒绝：{}", problems);
            return ExitCode::FAILURE;
        }
    };

    let task = Task {
        task_id: "task-001".to_string(),
        contract,
    };
    // 示例执行记录：手工构造。本章不真正执行任何命令。
    let run = Run {
        run_id: 1,
        task_id: "task-001".to_string(),
        attempt: 1,
        command: task.contract.acceptance_command.clone(),
        // 示例时间戳，不是真实运行时间。
        started_at: 1_770_000_000,
        ended_at: Some(1_770_000_005),
        exit_code: Some(0),
    };
    let record = TaskRecord {
        task,
        runs: vec![run],
        verdict: Some(QualityVerdict::Unknown),
        stop_reason: None,
    };

    let json = match serde_json::to_string_pretty(&record) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("序列化失败：{}", error);
            return ExitCode::FAILURE;
        }
    };
    println!("任务档案 JSON：");
    println!("{}", json);

    let restored: TaskRecord = match serde_json::from_str(&json) {
        Ok(record) => record,
        Err(error) => {
            eprintln!("反序列化失败：{}", error);
            return ExitCode::FAILURE;
        }
    };
    println!("序列化后原样读回，与原档案相等：{}", restored == record);
    println!();
    println!("这条运行以退出码 0 成功结束，质量结论仍是 UNKNOWN。");
    println!("运行成功只是运行事实；PASS 只能来自有效证据，本章不产生证据。");
    ExitCode::SUCCESS
}
