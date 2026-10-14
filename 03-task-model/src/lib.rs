//! 任务、运行、结果的数据模型。
//!
//! 模型把四件事分开记录：任务身份（`Task`）、执行记录（`Run`）、
//! 质量结论（`QualityVerdict`）与停止原因（`StopReason`）。
//! 运行成功是运行事实，不充当质量证据。

use serde::{Deserialize, Serialize};

/// 质量结论：只由证据决定，与运行是否成功分属两个维度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QualityVerdict {
    /// 有效证据证明必需验收全部通过。
    Pass,
    /// 有效证据证明至少一项验收失败。
    Fail,
    /// 证据缺失、无效或无法执行；这是合法结论，不是失败。
    Unknown,
}

/// 停止原因：流程为什么不再继续，不冒充质量结论。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StopReason {
    /// 时间、次数或资源预算耗尽。
    BudgetExhausted,
    /// 连续多轮没有实质进展。
    NoProgress,
    /// 必要验收无法执行，证据不足。
    EvidenceMissing,
    /// 用户主动取消。
    Cancelled,
}

/// 任务契约：第 2 章文本格式的类型化版本。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contract {
    pub version: u32,
    pub goal: String,
    pub scope: String,
    pub non_goals: String,
    pub constraints: String,
    pub acceptance_command: String,
    pub expected_result: String,
    pub counter_example: String,
}

/// 任务身份：编号加契约，不携带任何质量状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub contract: Contract,
}

/// 执行记录：一次运行的运行事实。本章只构造示例，不真正执行命令。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub run_id: u64,
    pub task_id: String,
    /// 同一任务的第几次执行，从 1 开始。
    pub attempt: u32,
    pub command: String,
    /// Unix 秒。
    pub started_at: u64,
    /// None 表示运行尚未结束。
    pub ended_at: Option<u64>,
    /// None 表示进程尚未退出。
    pub exit_code: Option<i32>,
}

/// 任务档案：身份、执行记录、质量结论与停止原因各居其位。
///
/// 一条运行可以成功结束（`exit_code` 为 `Some(0)`），
/// 同时结论仍是 `Unknown`：运行成功不是质量通过。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRecord {
    pub task: Task,
    /// 执行记录，按发生顺序追加。
    pub runs: Vec<Run>,
    /// None 表示尚未评估；Some(Unknown) 表示已评估但证据不足。
    pub verdict: Option<QualityVerdict>,
    /// None 表示流程仍在继续。
    pub stop_reason: Option<StopReason>,
}

/// 解析通过第 2 章输入门禁的契约文本。
///
/// 校验规则由 chapter-02-contracts 统一实现，本章不复制规则；
/// 未通过门禁时返回 Err，携带全部问题。
pub fn parse_contract(text: &str) -> Result<Contract, String> {
    let errors = chapter_02_contracts::validate_contract(text);
    if !errors.is_empty() {
        return Err(errors.join("；"));
    }
    // 门禁通过后 8 个字段必然各出现一次且非空；这里只取值，不重新校验。
    let version: Option<u32> = field(text, "版本")
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0);
    // 门禁已拒绝非法版本，这里不会触发；写出 Err 分支以避免 unwrap。
    let version = version.ok_or_else(|| "版本必须是大于 0 的整数".to_string())?;
    Ok(Contract {
        version,
        goal: owned(text, "目标"),
        scope: owned(text, "范围"),
        non_goals: owned(text, "非目标"),
        constraints: owned(text, "约束"),
        acceptance_command: owned(text, "验收操作"),
        expected_result: owned(text, "预期结果"),
        counter_example: owned(text, "反例"),
    })
}

// 与第 2 章同名私有函数相同：从一行文本中取出字段值。
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == key).then_some(value.trim())
    })
}

fn owned(text: &str, key: &str) -> String {
    // 门禁保证字段存在；空串兜底，避免 unwrap 崩溃。
    field(text, key).unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = include_str!("../examples/task.txt");

    fn contract() -> Contract {
        Contract {
            version: 1,
            goal: "为本地任务工具增加问候输出".to_string(),
            scope: "仅修改 01-cargo/src/main.rs".to_string(),
            non_goals: "不接入模型，不修改依赖".to_string(),
            constraints: "保持现有输出，新增内容使用中文".to_string(),
            acceptance_command: "在仓库根目录运行 cargo run -p chapter-01-cargo".to_string(),
            expected_result: "原有输出保留，最后一行精确为“任务已接收”".to_string(),
            counter_example: "若缺少该行或删除原有输出，验收不通过".to_string(),
        }
    }

    fn record_with(verdict: Option<QualityVerdict>) -> TaskRecord {
        TaskRecord {
            task: Task {
                task_id: "task-001".to_string(),
                contract: contract(),
            },
            runs: Vec::new(),
            verdict,
            stop_reason: None,
        }
    }

    #[test]
    fn example_contract_parses_into_typed_model() {
        assert_eq!(parse_contract(BASE), Ok(contract()));
    }

    #[test]
    fn contracts_failing_the_gate_are_rejected() {
        for bad in [
            BASE.replace("目标: 为本地任务工具增加问候输出\n", ""),
            format!("{}目标: 另一个目标\n", BASE),
            BASE.replace("版本: 1", "版本: 0"),
            BASE.replace("版本: 1", "版本: abc"),
        ] {
            let Err(problems) = parse_contract(&bad) else {
                panic!("无效契约应被拒绝");
            };
            assert!(!problems.is_empty());
        }
    }

    #[test]
    fn completed_run_round_trips_with_unknown_verdict() {
        let record = TaskRecord {
            task: Task {
                task_id: "task-001".to_string(),
                contract: contract(),
            },
            runs: vec![
                Run {
                    run_id: 1,
                    task_id: "task-001".to_string(),
                    attempt: 1,
                    command: "cargo run -p chapter-01-cargo".to_string(),
                    started_at: 1_770_000_000,
                    ended_at: Some(1_770_000_005),
                    exit_code: Some(0),
                },
                Run {
                    run_id: 2,
                    task_id: "task-001".to_string(),
                    attempt: 2,
                    command: "cargo run -p chapter-01-cargo".to_string(),
                    started_at: 1_770_001_000,
                    ended_at: None,
                    exit_code: None,
                },
            ],
            // 运行成功结束，但没有任何证据支持验收。
            verdict: Some(QualityVerdict::Unknown),
            stop_reason: Some(StopReason::EvidenceMissing),
        };
        let json = serde_json::to_string(&record).unwrap();
        let restored: TaskRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, record);
        // 往返后两个维度互不覆盖：退出码仍是 0，结论仍是 UNKNOWN。
        assert_eq!(restored.runs[0].exit_code, Some(0));
        assert_eq!(restored.verdict, Some(QualityVerdict::Unknown));
    }

    #[test]
    fn none_and_unknown_verdict_serialize_differently() {
        let none_json = serde_json::to_string(&record_with(None)).unwrap();
        let unknown_json =
            serde_json::to_string(&record_with(Some(QualityVerdict::Unknown))).unwrap();
        assert!(none_json.contains("\"verdict\":null"));
        assert!(unknown_json.contains("\"verdict\":\"UNKNOWN\""));
        assert_ne!(none_json, unknown_json);
    }

    #[test]
    fn enum_names_match_design_vocabulary() {
        assert_eq!(
            serde_json::to_string(&QualityVerdict::Pass).unwrap(),
            "\"PASS\""
        );
        assert_eq!(
            serde_json::to_string(&QualityVerdict::Fail).unwrap(),
            "\"FAIL\""
        );
        assert_eq!(
            serde_json::to_string(&QualityVerdict::Unknown).unwrap(),
            "\"UNKNOWN\""
        );
        assert_eq!(
            serde_json::to_string(&StopReason::BudgetExhausted).unwrap(),
            "\"BUDGET_EXHAUSTED\""
        );
        assert_eq!(
            serde_json::to_string(&StopReason::NoProgress).unwrap(),
            "\"NO_PROGRESS\""
        );
        assert_eq!(
            serde_json::to_string(&StopReason::EvidenceMissing).unwrap(),
            "\"EVIDENCE_MISSING\""
        );
        assert_eq!(
            serde_json::to_string(&StopReason::Cancelled).unwrap(),
            "\"CANCELLED\""
        );
    }
}
