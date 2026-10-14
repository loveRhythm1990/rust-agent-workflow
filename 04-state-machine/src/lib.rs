//! 状态机与确定性决策。
//!
//! 判断动作是否继续和判断质量是否通过是两个问题：
//! `decide` 只从验收清单计算结论，`plan_next` 再决定动作，
//! `transition` 保证只有允许表里的状态转换能发生。

mod decision;
mod status;

pub use decision::{NextAction, VerificationSummary, decide, plan_next};
pub use status::{ALLOWED_TRANSITIONS, TaskStatus, transition};
