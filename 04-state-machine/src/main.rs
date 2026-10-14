use chapter_03_task_model::{QualityVerdict, StopReason};
use chapter_04_state_machine::{
    NextAction, TaskStatus, VerificationSummary, decide, plan_next, transition,
};
use std::process::ExitCode;

fn main() -> ExitCode {
    println!("决策分两步：先独立计算质量结论，再决定动作。清单为手工构造的示例。\n");

    println!("== 场景一：全部验收通过 ==");
    show_decision(&summary(&["原有输出保留", "新增行为符合预期"], &[], &[]), 2);
    walk(&[
        TaskStatus::Planned,
        TaskStatus::Implementing,
        TaskStatus::Verifying,
        TaskStatus::Reviewing,
        TaskStatus::Deciding,
        TaskStatus::Ready,
        TaskStatus::Done,
    ]);

    println!("\n== 场景二：FAIL 且还有修复机会 ==");
    let mut round = summary(
        &["原有输出保留"],
        &["新增行为触发反例"],
        &["预期结果未复跑"],
    );
    show_decision(&round, 2);
    walk(&[
        TaskStatus::Deciding,
        TaskStatus::Fixing,
        TaskStatus::Verifying,
        TaskStatus::Reviewing,
        TaskStatus::Deciding,
    ]);
    // 修复后复验：清单翻新，修复次数消耗一次。
    round.failed.clear();
    round.unverified.clear();
    round.passed.push("预期结果复跑通过".to_string());
    show_decision(&round, 1);
    walk(&[TaskStatus::Deciding, TaskStatus::Ready, TaskStatus::Done]);

    println!("\n== 场景三：FAIL 且修复次数耗尽 ==");
    show_decision(&summary(&[], &["反例仍然触发"], &[]), 0);
    walk(&[TaskStatus::Deciding, TaskStatus::Stopped]);

    println!("\n== 场景四：证据缺失 ==");
    show_decision(&summary(&[], &[], &["验收环境不可用，预设命令未执行"]), 2);
    walk(&[TaskStatus::Deciding, TaskStatus::Stopped]);

    println!("\n== 非法转换 ==");
    walk(&[TaskStatus::Planned, TaskStatus::Done]);
    walk(&[TaskStatus::Done, TaskStatus::Planned]);

    println!();
    println!("STOPPED 不是 FAIL：场景三保留 FAIL，场景四保留 UNKNOWN。");
    println!("运行状态与质量结论分开记录，停止原因说明为什么停。");
    ExitCode::SUCCESS
}

fn summary(passed: &[&str], failed: &[&str], unverified: &[&str]) -> VerificationSummary {
    let to_owned = |lines: &[&str]| {
        lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
    };
    VerificationSummary {
        passed: to_owned(passed),
        failed: to_owned(failed),
        unverified: to_owned(unverified),
    }
}

fn show_decision(summary: &VerificationSummary, fixes_remaining: u32) {
    let verdict = decide(summary);
    let action = plan_next(&verdict, fixes_remaining);
    println!(
        "质量结论 {}，剩余修复次数 {}，动作：{}",
        verdict_name(&verdict),
        fixes_remaining,
        action_name(&action)
    );
}

fn walk(path: &[TaskStatus]) {
    let mut current = path[0];
    print!("{:?}", current);
    for &next in &path[1..] {
        match transition(current, next) {
            Ok(_) => {
                print!(" -> {:?}", next);
                current = next;
            }
            Err(message) => {
                println!("\n  拒绝：{}", message);
                return;
            }
        }
    }
    println!();
}

fn verdict_name(verdict: &QualityVerdict) -> &'static str {
    match verdict {
        QualityVerdict::Pass => "PASS",
        QualityVerdict::Fail => "FAIL",
        QualityVerdict::Unknown => "UNKNOWN",
    }
}

fn action_name(action: &NextAction) -> String {
    match action {
        NextAction::Ready => "READY（候选可交付）".to_string(),
        NextAction::Fix => "FIXING（进入修复）".to_string(),
        NextAction::Stop(reason) => format!("STOPPED（{}）", stop_name(reason)),
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
