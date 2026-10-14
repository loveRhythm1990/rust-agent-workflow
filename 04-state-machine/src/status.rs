//! 任务状态与状态机规则：转换显式列举，终态冻结。

/// 任务的流程位置；不代表质量结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    /// 契约已确认，尚未开始实现。
    Planned,
    /// 编码进行中。
    Implementing,
    /// 自动验收进行中。
    Verifying,
    /// 审查进行中。
    Reviewing,
    /// 证据齐备，等待计算结论与动作。
    Deciding,
    /// 已知失败，进入修复。
    Fixing,
    /// 候选可交付，等待完成交付动作。
    Ready,
    /// 交付动作完成；终态。
    Done,
    /// 等待人工决定或输入；自动触发条件在后续章节接入。
    WaitingHuman,
    /// 不再继续；终态，原因另记 StopReason。
    Stopped,
    /// 用户主动取消；终态。
    Cancelled,
}

/// 合法转换的完整允许列表：表里没有的转换一律被拒绝。
pub const ALLOWED_TRANSITIONS: &[(TaskStatus, TaskStatus)] = &[
    (TaskStatus::Planned, TaskStatus::Implementing),
    (TaskStatus::Implementing, TaskStatus::Verifying),
    (TaskStatus::Verifying, TaskStatus::Reviewing),
    (TaskStatus::Reviewing, TaskStatus::Deciding),
    (TaskStatus::Deciding, TaskStatus::Fixing),
    (TaskStatus::Fixing, TaskStatus::Verifying),
    (TaskStatus::Deciding, TaskStatus::WaitingHuman),
    (TaskStatus::WaitingHuman, TaskStatus::Deciding),
    (TaskStatus::Deciding, TaskStatus::Ready),
    (TaskStatus::Ready, TaskStatus::Done),
    (TaskStatus::Deciding, TaskStatus::Stopped),
    (TaskStatus::Planned, TaskStatus::Cancelled),
    (TaskStatus::Implementing, TaskStatus::Cancelled),
    (TaskStatus::Verifying, TaskStatus::Cancelled),
    (TaskStatus::Reviewing, TaskStatus::Cancelled),
    (TaskStatus::Deciding, TaskStatus::Cancelled),
    (TaskStatus::Fixing, TaskStatus::Cancelled),
    (TaskStatus::Ready, TaskStatus::Cancelled),
    (TaskStatus::WaitingHuman, TaskStatus::Cancelled),
];

/// 按允许表转换状态；非法转换与终态出边都被拒绝。
pub fn transition(from: TaskStatus, to: TaskStatus) -> Result<TaskStatus, String> {
    if is_terminal(from) {
        return Err(format!(
            "{:?} 是终态，不可再转换；恢复请创建新 attempt",
            from
        ));
    }
    if ALLOWED_TRANSITIONS.contains(&(from, to)) {
        Ok(to)
    } else {
        Err(format!("非法状态转换：{:?} 不可直接转到 {:?}", from, to))
    }
}

fn is_terminal(status: TaskStatus) -> bool {
    !ALLOWED_TRANSITIONS.iter().any(|(from, _)| *from == status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use TaskStatus::*;

    const ALL: [TaskStatus; 11] = [
        Planned,
        Implementing,
        Verifying,
        Reviewing,
        Deciding,
        Fixing,
        Ready,
        Done,
        WaitingHuman,
        Stopped,
        Cancelled,
    ];

    #[test]
    fn full_pass_path_reaches_done() {
        let mut current = Planned;
        for next in [Implementing, Verifying, Reviewing, Deciding, Ready, Done] {
            assert_eq!(transition(current, next), Ok(next));
            current = next;
        }
    }

    #[test]
    fn fix_loop_and_wait_loop_back_to_deciding() {
        let mut current = Deciding;
        for next in [Fixing, Verifying, Reviewing, Deciding] {
            assert_eq!(transition(current, next), Ok(next));
            current = next;
        }
        for next in [WaitingHuman, Deciding] {
            assert_eq!(transition(current, next), Ok(next));
            current = next;
        }
    }

    #[test]
    fn every_active_state_can_cancel() {
        for from in [
            Planned,
            Implementing,
            Verifying,
            Reviewing,
            Deciding,
            Fixing,
            Ready,
            WaitingHuman,
        ] {
            assert_eq!(transition(from, Cancelled), Ok(Cancelled));
        }
    }

    #[test]
    fn terminal_states_are_frozen() {
        for from in [Done, Stopped, Cancelled] {
            for to in ALL {
                let Err(message) = transition(from, to) else {
                    panic!("{:?} 是终态，任何转换都应被拒绝", from);
                };
                assert!(message.contains("终态"));
            }
        }
    }

    #[test]
    fn illegal_jumps_are_rejected() {
        for (from, to) in [
            (Planned, Done),
            (Planned, Deciding),
            (Ready, Implementing),
            (Verifying, Fixing),
            (Ready, Fixing),
        ] {
            let Err(message) = transition(from, to) else {
                panic!("非法转换应被拒绝：{:?} -> {:?}", from, to);
            };
            assert!(message.contains("非法"));
        }
    }
}
