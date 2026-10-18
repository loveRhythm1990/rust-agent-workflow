use chapter_03_task_model::{Task, parse_contract};
use chapter_06_orchestration::encode_delegation;
use chapter_08_adapters::{AgentBinding, Fault, ScriptedAgent};

use std::process::ExitCode;

fn main() -> ExitCode {
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
    let delegation = encode_delegation(&task, "del-101");
    println!(
        "委派 {}（任务 {}），下面用统一接口调两类适配器。\n",
        delegation.delegation_id, delegation.task_id
    );

    // 两个合法适配器：同一个调用方，同一套校验。
    let bindings = [
        ("pi 模拟适配器", "sess-pi-01"),
        ("codex 模拟适配器", "sess-codex-01"),
    ];
    println!("== 合法报告 ==");
    for (name, session) in bindings {
        let binding = AgentBinding::new(Box::new(ScriptedAgent::valid(name)), session);
        match binding.run(&delegation) {
            Ok(accepted) => println!(
                "{}（会话 {}）：{}",
                accepted.agent, accepted.report.session_id, accepted.report.summary
            ),
            Err(error) => {
                eprintln!("{}：{}", name, error);
                return ExitCode::FAILURE;
            }
        }
    }

    // 五种故障：空报告、截断与身份错配都必须被拒绝。
    let faults = [
        (Fault::TruncatedJson, "截断"),
        (Fault::EmptyReport, "空报告"),
        (Fault::WrongSession, "身份错配"),
        (Fault::MissingStatus, "缺字段"),
        (Fault::FailedNoReason, "无失败原因"),
    ];
    println!("\n== 故障报告都被拒绝 ==");
    for (fault, label) in faults {
        let binding = AgentBinding::new(
            Box::new(ScriptedAgent::faulty(&format!("故障-{label}"), fault)),
            "sess-01",
        );
        match binding.run(&delegation) {
            Ok(_) => println!("{}：竟然通过了，这是 bug。", label),
            Err(error) => println!("{}：拒绝，{}", label, error),
        }
    }

    println!();
    println!("校验在控制器侧：Agent 自报 COMPLETED 不算数，身份要回带、内容要非空。");
    println!("会话身份显式绑定；第 10 章接真实 CLI 时，报告必须原样回带同一会话。");
    ExitCode::SUCCESS
}
