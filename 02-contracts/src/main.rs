use chapter_02_contracts::{validate_contract, validate_revision};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() > 2 {
        eprintln!("用法：chapter-02-contracts [当前契约文件] [上一版契约文件]");
        return ExitCode::from(2);
    }
    let contract = if let Some(path) = args.first() {
        match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("无法读取当前契约：{}", error);
                return ExitCode::from(2);
            }
        }
    } else {
        include_str!("../examples/task.txt").to_string()
    };

    let mut errors = validate_contract(&contract);
    if let Some(path) = args.get(1) {
        match fs::read_to_string(path) {
            Ok(previous) => errors.extend(validate_revision(&previous, &contract)),
            Err(error) => {
                eprintln!("无法读取上一版契约：{}", error);
                return ExitCode::from(2);
            }
        }
    }
    if !errors.is_empty() {
        eprintln!("输入门禁拒绝：");
        for error in errors {
            eprintln!("- {}", error);
        }
        return ExitCode::FAILURE;
    }
    println!("输入门禁通过：契约字段齐全，格式有效。");
    println!("{}", contract.trim());
    println!("这不代表实现已完成或验收通过；业务含义仍需人工确认。");
    ExitCode::SUCCESS
}
