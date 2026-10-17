//! 编排层选型与接入：用模拟委派把任务交给外部执行者。
//!
//! 本章落成三件事：选型结论（README 中的实验记录）、委派模型
//! （Delegation + DelegationResult）、以及一个可重复运行的模拟适配器。
//! 真实 CLI 调用属于第 7 章；控制器不假装本会话的交互已经发生。

use chapter_03_task_model::Task;
use serde::{Deserialize, Serialize};
use std::fmt;

/// 委派给外部执行者的指令。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delegation {
    /// 本次委派的唯一标识；结果必须回带同一个 id。
    pub delegation_id: String,
    /// 原任务 id：委派结果必须能关联回原任务。
    pub task_id: String,
    /// 执行者看到的说明文字。
    pub instruction: String,
    /// 本次委派是否允许使用会写源文件的工具。
    pub no_tools: bool,
}

/// 委派得到的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationResult {
    /// 必须等于请求里的 delegation_id，否则不接诊。
    pub delegation_id: String,
    /// 回传的任务 id；与请求不一致则拒绝。
    pub task_id: String,
    /// 执行者叙述的结果。
    pub summary: String,
}

/// 一次委派可能如何失败：不伪装成成功。
#[derive(Debug, PartialEq, Eq)]
pub enum DelegationError {
    /// 结果与请求不匹配（返回了别家委派、别家任务的结果）。
    MismatchedIds {
        expected_id: String,
        got_id: String,
        expected_task: String,
        got_task: String,
    },
    /// 结果内容是空的：没有值可说，就当作失败。
    EmptyResult,
    /// 适配器自身错误（命令失败、账号未配置、输出不合法等）。
    AdapterFailure(String),
    /// 配置不合法（不认识的适配器名、读不出来的 JSON 等）。
    Config(String),
}

impl fmt::Display for DelegationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DelegationError::MismatchedIds {
                expected_id,
                got_id,
                expected_task,
                got_task,
            } => write!(
                f,
                "结果对应不上：委派 id {} -> {}，任务 id {} -> {}",
                expected_id, got_id, expected_task, got_task
            ),
            DelegationError::EmptyResult => write!(f, "返回了空结果，不当成成功"),
            DelegationError::AdapterFailure(detail) => write!(f, "适配器失败：{}", detail),
            DelegationError::Config(detail) => write!(f, "配置错误：{}", detail),
        }
    }
}

/// 委派结果必须能关联回原请求；空结果、张冠李戴都不算成功。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegationOutcome {
    /// 结果合法，可以交给下一环节。
    Accepted(DelegationResult),
}

/// 由任务契约生成编码委派。
pub fn encode_delegation(task: &Task, delegation_id: &str) -> Delegation {
    Delegation {
        delegation_id: delegation_id.to_string(),
        task_id: task.task_id.clone(),
        instruction: task.contract.goal.clone(),
        no_tools: true,
    }
}

/// 检查委派结果：id 一致、非空，才能算成功。
pub fn accept_result(
    request: &Delegation,
    result: &DelegationResult,
) -> Result<DelegationOutcome, DelegationError> {
    if result.summary.trim().is_empty() {
        return Err(DelegationError::EmptyResult);
    }
    let id_ok = result.delegation_id == request.delegation_id;
    let task_ok = result.task_id == request.task_id;
    if !id_ok || !task_ok {
        return Err(DelegationError::MismatchedIds {
            expected_id: request.delegation_id.clone(),
            got_id: result.delegation_id.clone(),
            expected_task: request.task_id.clone(),
            got_task: result.task_id.clone(),
        });
    }
    Ok(DelegationOutcome::Accepted(result.clone()))
}

/// 适配器配置；固定在 JSON 文件里，不藏在代码。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterConfig {
    /// "simulated" 或 "pi"；未知名字被拒绝。
    pub adapter: String,
    /// 适配器侧使用的模型名。
    pub model: String,
    /// 是否关闭所有工具，用于无源码写入的模拟委派。
    pub no_tools: bool,
    /// 会话目录；None 表示不持久化。
    pub session_dir: Option<String>,
}

/// 模拟适配器：每次委派都回一个可预测的 DelegationResult。
pub struct SimulatedAdapter;

impl SimulatedAdapter {
    /// 提交委派并得到结果；模拟不需要命令行。
    pub fn delegate(request: &Delegation) -> Result<DelegationResult, DelegationError> {
        if request.instruction.trim().is_empty() {
            return Err(DelegationError::AdapterFailure(
                "委派说明为空，模拟器无法送达".to_string(),
            ));
        }
        Ok(DelegationResult {
            delegation_id: request.delegation_id.clone(),
            task_id: request.task_id.clone(),
            summary: format!(
                "模拟执行：已收到「{}」的说明，尚未实现真实编码",
                request.instruction
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chapter_03_task_model::{Contract, Task};

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

    fn request() -> Delegation {
        encode_delegation(&task(), "del-001")
    }

    #[test]
    fn delegation_serializes_with_ids() {
        let delegation = request();
        let json = serde_json::to_string(&delegation).unwrap();
        assert!(json.contains("\"delegation_id\":\"del-001\""));
        assert!(json.contains("\"task_id\":\"task-001\""));
        let restored: Delegation = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, delegation);
    }

    #[test]
    fn accepted_result_links_back_to_task() {
        let request = request();
        let result = SimulatedAdapter::delegate(&request).unwrap();
        match accept_result(&request, &result).unwrap() {
            DelegationOutcome::Accepted(accepted) => {
                assert_eq!(accepted.task_id, request.task_id);
                assert_eq!(accepted.delegation_id, request.delegation_id);
                assert!(!accepted.summary.is_empty());
            }
        }
    }

    #[test]
    fn mismatched_ids_are_rejected() {
        let request = request();
        let wrong = DelegationResult {
            delegation_id: "del-999".to_string(),
            task_id: "task-999".to_string(),
            summary: "别人的结果".to_string(),
        };
        assert!(matches!(
            accept_result(&request, &wrong),
            Err(DelegationError::MismatchedIds { .. })
        ));
    }

    #[test]
    fn empty_result_is_not_success() {
        let request = request();
        let empty = DelegationResult {
            delegation_id: request.delegation_id.clone(),
            task_id: request.task_id.clone(),
            summary: "   ".to_string(),
        };
        assert!(matches!(
            accept_result(&request, &empty),
            Err(DelegationError::EmptyResult)
        ));
    }

    #[test]
    fn config_round_trip_includes_false_tool_permission() {
        let config = AdapterConfig {
            adapter: "simulated".to_string(),
            model: "glm-5.3".to_string(),
            no_tools: true,
            session_dir: None,
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let restored: AdapterConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, config);
        assert!(json.contains("\"no_tools\": true"));
    }

    #[test]
    fn empty_instruction_fails_clearly() {
        let bad = Delegation {
            delegation_id: "del-002".to_string(),
            task_id: "task-001".to_string(),
            instruction: "   ".to_string(),
            no_tools: true,
        };
        assert!(matches!(
            SimulatedAdapter::delegate(&bad),
            Err(DelegationError::AdapterFailure(_))
        ));
    }
}
