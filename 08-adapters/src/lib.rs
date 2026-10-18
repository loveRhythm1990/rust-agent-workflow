//! Agent 统一接口与结构化报告校验。
//!
//! 控制器生成显式会话身份并注入请求；Agent 交回的报告
//! 必须原样回带身份、内容非空。空报告、截断与身份错配都被拒绝。

use chapter_06_orchestration::Delegation;
use serde::{Deserialize, Serialize};
use std::fmt;

/// 发给 Agent 的请求：委派内容加上显式会话身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRequest {
    pub delegation_id: String,
    pub task_id: String,
    pub instruction: String,
    pub no_tools: bool,
    /// 会话身份由控制器生成；报告必须原样回带同一个 id。
    pub session_id: String,
}

/// 由委派与会话构造请求。
pub fn bind_request(delegation: &Delegation, session_id: &str) -> AgentRequest {
    AgentRequest {
        delegation_id: delegation.delegation_id.clone(),
        task_id: delegation.task_id.clone(),
        instruction: delegation.instruction.clone(),
        no_tools: delegation.no_tools,
        session_id: session_id.to_string(),
    }
}

/// 报告状态：Agent 自报的结局，还需过控制器的校验。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReportStatus {
    Completed,
    Failed,
}

/// Agent 交回的结构化报告。这份 serde 定义同时就是报告的 JSON 契约。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentReport {
    pub delegation_id: String,
    pub task_id: String,
    pub session_id: String,
    pub status: ReportStatus,
    pub summary: String,
    pub findings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

/// 统一调用接口：所有 Agent 适配器实现同一个 trait。
pub trait Agent {
    /// 适配器名称；来自实现与选型记录，不由报告自报。
    fn name(&self) -> &str;
    /// 交出原始报告文本。真实 CLI 的进程、超时与取消走第 7 章机制，
    /// 由第 10 章的适配器实现，本章先定接口与校验。
    fn call(&self, request: &AgentRequest) -> Result<String, AgentError>;
}

/// 可注入的故障类型，用来证明校验器真的会拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// 报文在中途断掉：截断。
    TruncatedJson,
    /// 状态 COMPLETED，但既无摘要也无发现：空报告。
    EmptyReport,
    /// 回带了别的会话身份：身份错配。
    WrongSession,
    /// 缺少 status 字段。
    MissingStatus,
    /// 状态是 FAILED，却不说原因。
    FailedNoReason,
}

/// 模拟适配器（测试替身）：按脚本返回报告。
pub struct ScriptedAgent {
    name: String,
    fault: Option<Fault>,
}

impl ScriptedAgent {
    pub fn valid(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fault: None,
        }
    }

    pub fn faulty(name: &str, fault: Fault) -> Self {
        Self {
            name: name.to_string(),
            fault: Some(fault),
        }
    }
}

impl Agent for ScriptedAgent {
    fn name(&self) -> &str {
        &self.name
    }

    fn call(&self, request: &AgentRequest) -> Result<String, AgentError> {
        let report = AgentReport {
            delegation_id: request.delegation_id.clone(),
            task_id: request.task_id.clone(),
            session_id: request.session_id.clone(),
            status: ReportStatus::Completed,
            summary: format!("模拟完成：{}", request.instruction),
            findings: Vec::new(),
            failure_reason: None,
        };
        let json = match self.fault {
            None => serde_json::to_string(&report),
            Some(Fault::TruncatedJson) => {
                let full = serde_json::to_string(&report)?;
                // 截断：保留前一半字符，模拟输出中途断流。
                Ok(full.chars().take(full.chars().count() / 2).collect())
            }
            Some(Fault::EmptyReport) => serde_json::to_string(&AgentReport {
                summary: String::new(),
                findings: Vec::new(),
                ..report
            }),
            Some(Fault::WrongSession) => serde_json::to_string(&AgentReport {
                session_id: "sess-elsewhere".to_string(),
                ..report
            }),
            Some(Fault::MissingStatus) => {
                let mut value = serde_json::to_value(&report)?;
                value
                    .as_object_mut()
                    .ok_or_else(|| AgentError::Adapter("报告应序列化为对象".into()))?
                    .remove("status");
                Ok(value.to_string())
            }
            Some(Fault::FailedNoReason) => serde_json::to_string(&AgentReport {
                status: ReportStatus::Failed,
                ..report
            }),
        }?;
        Ok(json)
    }
}

/// 校验或调用过程中的失败：每一种都不伪装成成功。
#[derive(Debug, PartialEq, Eq)]
pub enum AgentError {
    /// 适配器自身无法完成调用（进程、账号、IO）。
    Adapter(String),
    /// 报文不是合法 JSON；截断通常表现为解析中途断开。
    Malformed { detail: String },
    /// COMPLETED 但没有任何内容：空报告不能充当成功。
    EmptyReport,
    /// 身份对不上：会话、委派或任务 id 与请求不一致。
    IdentityMismatch {
        field: &'static str,
        expected: String,
        found: String,
    },
    /// FAILED 必须说明原因。
    MissingFailureReason,
}

impl From<serde_json::Error> for AgentError {
    fn from(error: serde_json::Error) -> Self {
        AgentError::Adapter(format!("序列化失败：{}", error))
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::Adapter(detail) => write!(f, "适配器失败：{}", detail),
            AgentError::Malformed { detail } => write!(f, "报告不是合法 JSON：{}", detail),
            AgentError::EmptyReport => write!(f, "空报告：COMPLETED 但没有任何内容"),
            AgentError::IdentityMismatch {
                field,
                expected,
                found,
            } => write!(
                f,
                "身份错配：{} 应为 {}，报告回带 {}",
                field, expected, found
            ),
            AgentError::MissingFailureReason => write!(f, "FAILED 报告缺少失败原因"),
        }
    }
}

/// 校验报告：合法 JSON、身份回带一致、内容非空。
pub fn validate_report(request: &AgentRequest, raw: &str) -> Result<AgentReport, AgentError> {
    let report: AgentReport = serde_json::from_str(raw).map_err(|error| AgentError::Malformed {
        detail: error.to_string(),
    })?;
    for (field, expected, found) in [
        ("session_id", &request.session_id, &report.session_id),
        (
            "delegation_id",
            &request.delegation_id,
            &report.delegation_id,
        ),
        ("task_id", &request.task_id, &report.task_id),
    ] {
        if expected != found {
            return Err(AgentError::IdentityMismatch {
                field,
                expected: expected.clone(),
                found: found.clone(),
            });
        }
    }
    match report.status {
        ReportStatus::Completed => {
            if report.summary.trim().is_empty() && report.findings.is_empty() {
                return Err(AgentError::EmptyReport);
            }
        }
        ReportStatus::Failed => {
            if report
                .failure_reason
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                return Err(AgentError::MissingFailureReason);
            }
        }
    }
    Ok(report)
}

/// 校验通过的报告。
#[derive(Debug, Clone, PartialEq)]
pub struct AcceptedReport {
    pub agent: String,
    pub report: AgentReport,
}

/// 用同一个校验器调用任何 Agent：换适配器不改调用方。
pub fn run_with<A: Agent + ?Sized>(
    agent: &A,
    request: &AgentRequest,
) -> Result<AcceptedReport, AgentError> {
    let raw = agent.call(request)?;
    let report = validate_report(request, &raw)?;
    Ok(AcceptedReport {
        agent: agent.name().to_string(),
        report,
    })
}

/// 组合：运行时替换的适配器与固定的会话绑定。
pub struct AgentBinding {
    pub agent: Box<dyn Agent>,
    pub session_id: String,
}

impl AgentBinding {
    pub fn new(agent: Box<dyn Agent>, session_id: &str) -> Self {
        Self {
            agent,
            session_id: session_id.to_string(),
        }
    }

    pub fn run(&self, delegation: &Delegation) -> Result<AcceptedReport, AgentError> {
        let request = bind_request(delegation, &self.session_id);
        run_with(self.agent.as_ref(), &request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chapter_03_task_model::{Contract, Task};
    use chapter_06_orchestration::encode_delegation;

    fn delegation() -> Delegation {
        let task = Task {
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
        };
        encode_delegation(&task, "del-101")
    }

    fn request(session: &str) -> AgentRequest {
        bind_request(&delegation(), session)
    }

    #[test]
    fn valid_report_is_accepted_and_echoes_identity() {
        let accepted =
            run_with(&ScriptedAgent::valid("pi 模拟适配器"), &request("sess-a1")).unwrap();
        assert_eq!(accepted.agent, "pi 模拟适配器");
        assert_eq!(accepted.report.session_id, "sess-a1");
        assert_eq!(accepted.report.delegation_id, "del-101");
        assert_eq!(accepted.report.task_id, "task-001");
        assert_eq!(accepted.report.status, ReportStatus::Completed);
        assert!(!accepted.report.summary.is_empty());
    }

    #[test]
    fn status_vocabulary_is_uppercase() {
        assert_eq!(
            serde_json::to_string(&ReportStatus::Completed).unwrap(),
            "\"COMPLETED\""
        );
        assert_eq!(
            serde_json::to_string(&ReportStatus::Failed).unwrap(),
            "\"FAILED\""
        );
    }

    #[test]
    fn truncated_json_is_rejected() {
        let error = run_with(
            &ScriptedAgent::faulty("截断适配器", Fault::TruncatedJson),
            &request("sess-a1"),
        )
        .unwrap_err();
        assert!(matches!(error, AgentError::Malformed { .. }));
    }

    #[test]
    fn empty_report_is_rejected() {
        let error = run_with(
            &ScriptedAgent::faulty("空报告适配器", Fault::EmptyReport),
            &request("sess-a1"),
        )
        .unwrap_err();
        assert_eq!(error, AgentError::EmptyReport);
    }

    #[test]
    fn wrong_session_is_identity_mismatch() {
        let error = run_with(
            &ScriptedAgent::faulty("串会话适配器", Fault::WrongSession),
            &request("sess-a1"),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            AgentError::IdentityMismatch {
                field: "session_id",
                ..
            }
        ));
    }

    #[test]
    fn missing_status_is_malformed() {
        let error = run_with(
            &ScriptedAgent::faulty("缺字段适配器", Fault::MissingStatus),
            &request("sess-a1"),
        )
        .unwrap_err();
        assert!(matches!(error, AgentError::Malformed { .. }));
    }

    #[test]
    fn failed_report_requires_reason() {
        let error = run_with(
            &ScriptedAgent::faulty("哑失败适配器", Fault::FailedNoReason),
            &request("sess-a1"),
        )
        .unwrap_err();
        assert_eq!(error, AgentError::MissingFailureReason);
    }

    #[test]
    fn adapters_are_interchangeable() {
        let request = request("sess-a1");
        let pi = run_with(&ScriptedAgent::valid("pi 模拟适配器"), &request).unwrap();
        let codex = run_with(&ScriptedAgent::valid("codex 模拟适配器"), &request).unwrap();
        // 调用方一行没改；换的只是实现。
        assert_eq!(pi.report, codex.report);
        assert_eq!(pi.agent, "pi 模拟适配器");
        assert_eq!(codex.agent, "codex 模拟适配器");
    }

    #[test]
    fn binding_swaps_agents_at_runtime() {
        let good = AgentBinding::new(Box::new(ScriptedAgent::valid("甲适配器")), "sess-g1");
        assert!(good.run(&delegation()).is_ok());
        let bad = AgentBinding::new(
            Box::new(ScriptedAgent::faulty("乙适配器", Fault::EmptyReport)),
            "sess-g1",
        );
        assert_eq!(bad.run(&delegation()).unwrap_err(), AgentError::EmptyReport);
    }
}
