use chapter_03_task_model::{Task, parse_contract};
use chapter_06_orchestration::{
    AdapterConfig, DelegationOutcome, SimulatedAdapter, accept_result, encode_delegation,
};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [] => run_demo_task(),
        [path] => run_with_config(path),
        _ => {
            eprintln!("用法：chapter-06-orchestration [适配器配置 JSON]");
            ExitCode::from(2)
        }
    }
}

fn run_demo_task() -> ExitCode {
    let task = match parse_contract(include_str!("../examples/task.txt")) {
        Ok(contract) => Task {
            task_id: "task-001".to_string(),
            contract,
        },
        Err(problems) => {
            eprintln!("契约解析失败：{}", problems);
            return ExitCode::FAILURE;
        }
    };

    println!("任务 {} 的目标：{}", task.task_id, task.contract.goal);
    let delegation = encode_delegation(&task, "del-001");
    println!("委派 {} 交给模拟执行者。", delegation.delegation_id);

    let result = match SimulatedAdapter::delegate(&delegation) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("委派失败：{}", error);
            return ExitCode::from(3);
        }
    };
    println!("执行者回送：{}", result.summary);

    match accept_result(&delegation, &result) {
        Ok(DelegationOutcome::Accepted(accepted)) => {
            println!("结果已接收并关联回任务 {}", accepted.task_id);
            println!("这真是模拟；控制器尚未交托真实 Agent（第 7 章起）。");
        }
        Err(error) => {
            eprintln!("结果被拒绝：{}", error);
            return ExitCode::FAILURE;
        }
    }
    println!("边界是固定的：控制器仍持，由它决定任务是否通过；委派只交付候选。");
    ExitCode::SUCCESS
}

fn run_with_config(path: &str) -> ExitCode {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("无法读取配置：{}", error);
            return ExitCode::from(2);
        }
    };
    let config: AdapterConfig = match serde_json::from_str(&text) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("配置不合法：{}", error);
            return ExitCode::from(2);
        }
    };
    println!(
        "适配器 {}（模型 {}，{}工具）已加载。",
        config.adapter,
        config.model,
        if config.no_tools {
            "不启用"
        } else {
            "启用"
        }
    );
    match config.adapter.as_str() {
        "simulated" => {
            println!("验证配置与本章的模拟适配器一致；真实 CLI 调用从第 7 章开始。");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("未知适配器 {}：本章只接通模拟，扩展留待后续。", other);
            ExitCode::FAILURE
        }
    }
}
