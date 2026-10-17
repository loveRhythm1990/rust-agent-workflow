//! 模拟执行器：用脚本化替身驱动完整流程。
//!
//! 不调用模型或真实命令。脚本给定每次 attempt 的验收结果，
//! 流程按第 4 章状态机推进，质量结论与停止原因来自第 3 章模型。
//! 任何环节失败以 `Err` 返回，不伪装成成功。

use chapter_03_task_model::{QualityVerdict, Run, Task, TaskRecord};
use chapter_04_state_machine::{
    NextAction, TaskStatus, VerificationSummary, decide, plan_next, transition,
};
use std::collections::HashSet;
use std::fmt;

/// 一项模拟验收的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckResult {
    /// 该项通过。
    Passed,
    /// 该项失败。
    Failed,
    /// 无法得出结论（环境不可用等），进入未验证清单。
    Inconclusive,
}

/// 脚本中的一项验收记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOutcome {
    pub name: String,
    pub result: CheckResult,
}

/// 一次 attempt 的验收计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptPlan {
    pub checks: Vec<CheckOutcome>,
}

/// 模拟执行器（测试替身）：整个任务的脚本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub attempts: Vec<AttemptPlan>,
}

/// 一次 attempt 的决策记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptDecision {
    pub attempt: u32,
    pub verdict: QualityVerdict,
    pub action: NextAction,
}

/// 流程失败的原因：模拟会返回 `Err`，而不是伪装成成功。
#[derive(Debug, PartialEq, Eq)]
pub enum SimulationError {
    /// 状态机拒绝了某次转换。
    Transition(String),
    /// 脚本在同一次验收中重复登记了同一验收项。
    DuplicateCheck(String),
    /// 流程还需要下一轮 attempt，但脚本已经没有更多记录。
    ScriptExhausted { attempt: u32 },
}

impl fmt::Display for SimulationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SimulationError::Transition(message) => write!(f, "状态机拒绝：{}", message),
            SimulationError::DuplicateCheck(name) => write!(f, "脚本重复登记验收项：{name}"),
            SimulationError::ScriptExhausted { attempt } => {
                write!(f, "脚本没有第 {attempt} 次尝试的记录")
            }
        }
    }
}

/// 模拟完成后的报告。
#[derive(Debug, Clone)]
pub struct SimulationReport {
    /// 第 3 章的任务档案；运行记录、结论与停止原因各居其位。
    pub record: TaskRecord,
    /// 流程停止时的最终状态。
    pub final_status: TaskStatus,
    /// 实际使用的 attempt 数。
    pub attempts_used: u32,
    /// 实际消耗的修复次数。
    pub fixes_used: u32,
    /// 每一轮的结论与动作记录。
    pub decisions: Vec<AttemptDecision>,
}

const BASE_TIME: u64 = 1_770_000_000;

/// 按脚本驱动流程，直到 `DONE`、`STOPPED` 或 `Err`。
pub fn run_simulation(
    task: &Task,
    script: &Script,
    max_fixes: u32,
) -> Result<SimulationReport, SimulationError> {
    let mut status = TaskStatus::Planned;
    let mut runs = Vec::new();
    let mut decisions = Vec::new();
    let mut fixes_remaining = max_fixes;
    let mut attempt = 1u32;

    // `loop` 作为表达式取值：走完即给出结论与停止原因。
    let (verdict, stop_reason) = loop {
        let plan = script
            .attempts
            .get(usize::try_from(attempt - 1).unwrap_or(usize::MAX))
            .ok_or(SimulationError::ScriptExhausted { attempt })?;

        if status == TaskStatus::Planned {
            status = step(status, TaskStatus::Implementing)?;
            status = step(status, TaskStatus::Verifying)?;
        } else {
            status = step(status, TaskStatus::Fixing)?;
            status = step(status, TaskStatus::Verifying)?;
        }
        status = step(status, TaskStatus::Reviewing)?;
        status = step(status, TaskStatus::Deciding)?;

        let summary = build_summary(plan)?;
        let verdict = decide(&summary);
        let action = plan_next(&verdict, fixes_remaining);
        decisions.push(AttemptDecision {
            attempt,
            verdict: verdict.clone(),
            action: action.clone(),
        });

        // 记录运行事实：exit_code 0 只表示模拟步骤完成，不是质量结论。
        let started = BASE_TIME + u64::from(attempt) * 10;
        runs.push(Run {
            run_id: u64::from(attempt),
            task_id: task.task_id.clone(),
            attempt,
            command: format!("模拟 attempt {attempt}：编码 → 验收 → 审查"),
            started_at: started,
            ended_at: Some(started + 5),
            exit_code: Some(0),
        });

        match action {
            NextAction::Ready => {
                status = step(status, TaskStatus::Ready)?;
                status = step(status, TaskStatus::Done)?;
                break (verdict, None);
            }
            NextAction::Fix => {
                fixes_remaining -= 1;
                attempt += 1;
            }
            NextAction::Stop(reason) => {
                status = step(status, TaskStatus::Stopped)?;
                break (verdict, Some(reason));
            }
        }
    };

    Ok(SimulationReport {
        record: TaskRecord {
            task: task.clone(),
            runs,
            verdict: Some(verdict),
            stop_reason,
        },
        final_status: status,
        attempts_used: attempt,
        fixes_used: max_fixes - fixes_remaining,
        decisions,
    })
}

fn step(from: TaskStatus, to: TaskStatus) -> Result<TaskStatus, SimulationError> {
    transition(from, to).map_err(SimulationError::Transition)
}

/// 把下一次 attempt 的验收计划翻译成第 4 章的 VerificationSummary；
/// 同一次 attempt 重复登记某项验收会被拒绝。
fn build_summary(plan: &AttemptPlan) -> Result<VerificationSummary, SimulationError> {
    let mut seen = HashSet::new();
    let mut passed = Vec::new();
    let mut failed = Vec::new();
    let mut unverified = Vec::new();
    for check in &plan.checks {
        if !seen.insert(check.name.as_str()) {
            return Err(SimulationError::DuplicateCheck(check.name.clone()));
        }
        match check.result {
            CheckResult::Passed => passed.push(check.name.clone()),
            CheckResult::Failed => failed.push(check.name.clone()),
            CheckResult::Inconclusive => unverified.push(check.name.clone()),
        }
    }
    Ok(VerificationSummary {
        passed,
        failed,
        unverified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chapter_03_task_model::{Contract, StopReason, Task};

    fn task() -> Task {
        Task {
            task_id: "task-001".to_string(),
            contract: Contract {
                version: 1,
                goal: "为本地任务工具增加问候输出".to_string(),
                scope: "仅修改 01-cargo/src/main.rs".to_string(),
                non_goals: "不接入模型，不修改依赖".to_string(),
                constraints: "保持现有输出，新增内容使用中文".to_string(),
                acceptance_command: "在仓库根目录运行 cargo run -p chapter-01-cargo".to_string(),
                expected_result: "原有输出保留，最后一行精确为“任务已接收”".to_string(),
                counter_example: "若缺少该行或删除原有输出，验收不通过".to_string(),
            },
        }
    }

    fn outcome(name: &str, result: CheckResult) -> CheckOutcome {
        CheckOutcome {
            name: name.to_string(),
            result,
        }
    }

    fn attempt(checks: &[(&str, CheckResult)]) -> AttemptPlan {
        AttemptPlan {
            checks: checks
                .iter()
                .map(|(name, result)| outcome(name, result.clone()))
                .collect(),
        }
    }

    #[test]
    fn full_pass_reaches_done() {
        let script = Script {
            attempts: vec![attempt(&[
                ("原有输出保留", CheckResult::Passed),
                ("新增行为符合预期", CheckResult::Passed),
            ])],
        };
        let report = run_simulation(&task(), &script, 2).unwrap();
        assert_eq!(report.final_status, TaskStatus::Done);
        assert_eq!(report.record.verdict, Some(QualityVerdict::Pass));
        assert_eq!(report.record.stop_reason, None);
        assert_eq!(report.attempts_used, 1);
        assert_eq!(report.fixes_used, 0);
        assert_eq!(report.decisions[0].action, NextAction::Ready);
    }

    #[test]
    fn fail_then_fix_succeeds_uses_one_fix() {
        let script = Script {
            attempts: vec![
                attempt(&[
                    ("原有输出保留", CheckResult::Passed),
                    ("新增行为触发反例", CheckResult::Failed),
                    ("预期结果未复跑", CheckResult::Inconclusive),
                ]),
                attempt(&[
                    ("原有输出保留", CheckResult::Passed),
                    ("新增行为符合预期", CheckResult::Passed),
                    ("预期结果复跑通过", CheckResult::Passed),
                ]),
            ],
        };
        let report = run_simulation(&task(), &script, 2).unwrap();
        assert_eq!(report.final_status, TaskStatus::Done);
        assert_eq!(report.fixes_used, 1);
        assert_eq!(
            report
                .decisions
                .iter()
                .map(|d| d.action.clone())
                .collect::<Vec<_>>(),
            vec![NextAction::Fix, NextAction::Ready]
        );
        assert_eq!(report.record.runs.len(), 2);
        assert_eq!(report.record.runs[1].exit_code, Some(0));
    }

    #[test]
    fn fail_with_no_fix_budget_stops_with_fail_kept() {
        let script = Script {
            attempts: vec![
                attempt(&[("验收执行仍触发反例", CheckResult::Failed)]),
                attempt(&[("验收执行仍触发反例", CheckResult::Failed)]),
            ],
        };
        let report = run_simulation(&task(), &script, 1).unwrap();
        assert_eq!(report.final_status, TaskStatus::Stopped);
        assert_eq!(report.record.verdict, Some(QualityVerdict::Fail));
        assert_eq!(report.record.stop_reason, Some(StopReason::BudgetExhausted));
        assert_eq!(report.decisions.len(), 2);
    }

    #[test]
    fn inconclusive_only_is_unknown_not_fail() {
        let script = Script {
            attempts: vec![attempt(&[("验收环境不可用", CheckResult::Inconclusive)])],
        };
        let report = run_simulation(&task(), &script, 2).unwrap();
        assert_eq!(report.record.verdict, Some(QualityVerdict::Unknown));
        assert_eq!(report.record.stop_reason, Some(StopReason::EvidenceMissing));
        assert_eq!(report.final_status, TaskStatus::Stopped);
    }

    #[test]
    fn duplicate_checks_are_rejected() {
        let script = Script {
            attempts: vec![attempt(&[
                ("重复检查", CheckResult::Passed),
                ("重复检查", CheckResult::Failed),
            ])],
        };
        assert!(matches!(
            run_simulation(&task(), &script, 0),
            Err(SimulationError::DuplicateCheck(_))
        ));
    }

    #[test]
    fn script_exhaustion_is_rejected() {
        let script = Script { attempts: vec![] };
        let Err(error) = run_simulation(&task(), &script, 0) else {
            panic!("空脚本应被拒绝");
        };
        assert!(matches!(
            error,
            SimulationError::ScriptExhausted { attempt: 1 }
        ));
    }

    #[test]
    fn record_json_round_trip_keeps_verdict_and_stop_reason() {
        let script = Script {
            attempts: vec![
                attempt(&[("反例仍触发", CheckResult::Failed)]),
                attempt(&[("反例仍触发", CheckResult::Failed)]),
            ],
        };
        let report = run_simulation(&task(), &script, 1).unwrap();
        let json = serde_json::to_string(&report.record).unwrap();
        let restored: TaskRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, report.record);
        assert!(json.contains("\"verdict\":\"FAIL\""));
        assert!(json.contains("\"stop_reason\":\"BUDGET_EXHAUSTED\""));
    }
}
