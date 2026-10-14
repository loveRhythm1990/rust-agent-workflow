//! 质量结论与下一步动作：纯函数，无副作用。

use chapter_03_task_model::{QualityVerdict, StopReason};

/// 验收清单：每一项必需验收归入三类之一。
///
/// 三类同时非空是合法的：已知 FAIL 与缺失证据共存时，
/// 结论记 FAIL，未验证项照旧保留在清单里。
pub struct VerificationSummary {
    pub passed: Vec<String>,
    pub failed: Vec<String>,
    pub unverified: Vec<String>,
}

/// 独立计算质量结论：只看验收清单，不看流程状态或预算。
pub fn decide(summary: &VerificationSummary) -> QualityVerdict {
    if !summary.failed.is_empty() {
        QualityVerdict::Fail
    } else if !summary.unverified.is_empty() || summary.passed.is_empty() {
        // 缺少证据与"没有任何验收"都不是通过；空清单不能充当 PASS。
        QualityVerdict::Unknown
    } else {
        QualityVerdict::Pass
    }
}

/// 决策结果：继续修复、准备交付，还是停止。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextAction {
    /// 全部必需验收通过，候选可交付。
    Ready,
    /// 结论 FAIL 且还有修复机会，进入修复。
    Fix,
    /// 不再继续；质量结论原样保留，原因另记。
    Stop(StopReason),
}

/// 据质量结论与剩余修复次数决定动作。
///
/// 判断"是否继续"与"质量是否通过"是两个问题：
/// verdict 原样保留，这里只决定下一步。
pub fn plan_next(verdict: &QualityVerdict, fixes_remaining: u32) -> NextAction {
    match verdict {
        QualityVerdict::Pass => NextAction::Ready,
        QualityVerdict::Fail if fixes_remaining > 0 => NextAction::Fix,
        QualityVerdict::Fail => NextAction::Stop(StopReason::BudgetExhausted),
        QualityVerdict::Unknown => NextAction::Stop(StopReason::EvidenceMissing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn known_fail_wins_over_missing_evidence() {
        let mixed = summary(
            &["原有输出保留"],
            &["新增行为触发反例"],
            &["预期结果未复跑"],
        );
        // FAIL 与缺失证据共存：结论记 FAIL，未验证项不被丢弃。
        assert_eq!(decide(&mixed), QualityVerdict::Fail);
        assert!(!mixed.unverified.is_empty());
    }

    #[test]
    fn missing_evidence_without_failures_is_unknown() {
        let partial = summary(&["原有输出保留"], &[], &["预期结果未执行"]);
        assert_eq!(decide(&partial), QualityVerdict::Unknown);
    }

    #[test]
    fn empty_summary_is_unknown_not_pass() {
        let empty = summary(&[], &[], &[]);
        assert_eq!(decide(&empty), QualityVerdict::Unknown);
    }

    #[test]
    fn all_required_passed_is_pass() {
        let complete = summary(&["原有输出保留", "新增行为符合预期"], &[], &[]);
        assert_eq!(decide(&complete), QualityVerdict::Pass);
    }

    #[test]
    fn next_action_depends_on_verdict_and_remaining_fixes() {
        assert_eq!(plan_next(&QualityVerdict::Pass, 0), NextAction::Ready);
        assert_eq!(plan_next(&QualityVerdict::Fail, 2), NextAction::Fix);
        assert_eq!(
            plan_next(&QualityVerdict::Fail, 0),
            NextAction::Stop(StopReason::BudgetExhausted)
        );
        // UNKNOWN 不因为剩余机会而转成 FAIL，也不假装是 PASS。
        assert_eq!(
            plan_next(&QualityVerdict::Unknown, 5),
            NextAction::Stop(StopReason::EvidenceMissing)
        );
    }
}
