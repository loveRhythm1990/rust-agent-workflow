use chapter_03_task_model::{QualityVerdict, StopReason, Task, parse_contract};
use chapter_04_state_machine::NextAction;
use chapter_05_simulation::{
    AttemptPlan, CheckOutcome, CheckResult, Script, SimulationReport, run_simulation,
};
use std::{env, process::ExitCode};

const CONTRACT: &str = include_str!("../examples/task.txt");

struct Scenario {
    name: &'static str,
    script: Script,
    max_fixes: u32,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "场景一：全部验收通过",
            script: Script {
                attempts: vec![AttemptPlan {
                    checks: vec![
                        outcome("原有输出保留", CheckResult::Passed),
                        outcome("新增行为符合预期", CheckResult::Passed),
                        outcome("反例未触发", CheckResult::Passed),
                    ],
                }],
            },
            max_fixes: 2,
        },
        Scenario {
            name: "场景二：先失败，修复后通过",
            script: Script {
                attempts: vec![
                    AttemptPlan {
                        checks: vec![
                            outcome("原有输出保留", CheckResult::Passed),
                            outcome("新增行为触发反例", CheckResult::Failed),
                            outcome("预期结果未复跑", CheckResult::Inconclusive),
                        ],
                    },
                    AttemptPlan {
                        checks: vec![
                            outcome("原有输出保留", CheckResult::Passed),
                            outcome("新增行为符合预期", CheckResult::Passed),
                            outcome("预期结果复跑通过", CheckResult::Passed),
                        ],
                    },
                ],
            },
            max_fixes: 2,
        },
        Scenario {
            name: "场景三：修复次数耗尽",
            script: Script {
                attempts: vec![
                    AttemptPlan {
                        checks: vec![outcome("反例仍触发", CheckResult::Failed)],
                    },
                    AttemptPlan {
                        checks: vec![outcome("反例仍触发", CheckResult::Failed)],
                    },
                ],
            },
            max_fixes: 1,
        },
        Scenario {
            name: "场景四：证据缺失",
            script: Script {
                attempts: vec![AttemptPlan {
                    checks: vec![outcome("验收环境不可用", CheckResult::Inconclusive)],
                }],
            },
            max_fixes: 2,
        },
    ]
}

fn outcome(name: &str, result: CheckResult) -> CheckOutcome {
    CheckOutcome {
        name: name.to_string(),
        result,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let task = match parse_contract(CONTRACT) {
        Ok(contract) => Task {
            task_id: "task-001".to_string(),
            contract,
        },
        Err(problems) => {
            eprintln!("契约解析失败：{}", problems);
            return ExitCode::FAILURE;
        }
    };

    match args.as_slice() {
        [] => {
            println!("四个场景独立运行；结果不属于任何模型，全部来自脚本与状态机。\n");
            for scenario in scenarios() {
                run_scenario(&scenario, &task);
            }
            println!("模拟不会产出证据；模拟报告与真实验收的区别见 README。");
            ExitCode::SUCCESS
        }
        [flag, index] if flag == "--json" => print_json_scenario(&task, index),
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("用法：chapter-05-simulation [--json <场景号 1-4>]");
    ExitCode::from(2)
}

fn run_scenario(scenario: &Scenario, task: &Task) {
    println!(
        "== {}（最多 {} 次修复） ==",
        scenario.name, scenario.max_fixes
    );
    match run_simulation(task, &scenario.script, scenario.max_fixes) {
        Ok(report) => narrate(scenario, &report),
        Err(error) => println!("流程失败：{}（不当作成功）", error),
    }
    println!();
}

fn narrate(scenario: &Scenario, report: &SimulationReport) {
    for (index, decision) in report.decisions.iter().enumerate() {
        let checks = &scenario.script.attempts[index].checks;
        let listed = checks
            .iter()
            .map(|check| format!("{}：{}", check.name, check_name(check.result)))
            .collect::<Vec<_>>()
            .join("、");
        println!("脚本 attempt {}：{}", decision.attempt, listed);
        println!(
            "  结论 {}，动作 {}",
            verdict_name(&decision.verdict),
            action_name(&decision.action)
        );
    }
    println!(
        "最终状态 {:?}，结论 {}，停止原因 {}",
        report.final_status,
        report
            .record
            .verdict
            .as_ref()
            .map(verdict_name)
            .unwrap_or("无"),
        report
            .record
            .stop_reason
            .as_ref()
            .map(stop_name)
            .unwrap_or("无")
    );
}

fn print_json_scenario(task: &Task, index: &str) -> ExitCode {
    let number: usize = match index.parse() {
        Ok(number) if (1..=4).contains(&number) => number,
        _ => return usage(),
    };
    let scenario = &scenarios()[number - 1];
    println!("== {} ==", scenario.name);
    match run_simulation(task, &scenario.script, scenario.max_fixes) {
        Ok(report) => match serde_json::to_string_pretty(&report.record) {
            Ok(json) => {
                println!("{}", json);
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("序列化失败：{}", error);
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("流程失败：{}", error);
            ExitCode::FAILURE
        }
    }
}

fn check_name(result: CheckResult) -> &'static str {
    match result {
        CheckResult::Passed => "通过",
        CheckResult::Failed => "失败",
        CheckResult::Inconclusive => "未定",
    }
}

fn verdict_name(verdict: &QualityVerdict) -> &'static str {
    match verdict {
        QualityVerdict::Pass => "PASS",
        QualityVerdict::Fail => "FAIL",
        QualityVerdict::Unknown => "UNKNOWN",
    }
}

fn action_name(action: &NextAction) -> &'static str {
    match action {
        NextAction::Ready => "READY",
        NextAction::Fix => "FIXING",
        NextAction::Stop(_) => "STOPPED",
    }
}

fn stop_name(reason: &StopReason) -> &'static str {
    match reason {
        StopReason::BudgetExhausted => "BUDGET_EXHAUSTED",
        StopReason::NoProgress => "NO_PROGRESS",
        StopReason::EvidenceMissing => "EVIDENCE_MISSING",
        StopReason::Cancelled => "CANCELLED",
    }
}
