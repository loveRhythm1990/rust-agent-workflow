//! 本章使用简单的文本契约；下一章再引入正式的任务类型。

const FIELDS: [&str; 8] = [
    "版本",
    "目标",
    "范围",
    "非目标",
    "约束",
    "验收操作",
    "预期结果",
    "反例",
];

// 返回所有格式问题；空列表只代表输入门禁通过。
pub fn validate_contract(contract: &str) -> Vec<String> {
    let mut errors = Vec::new();
    for (index, line) in contract.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match line.split_once(':') {
            Some((key, _)) if FIELDS.contains(&key.trim()) => {}
            _ => errors.push(format!("第 {} 行包含未知字段或缺少英文冒号", index + 1)),
        }
    }
    for key in FIELDS {
        let values: Vec<&str> = contract
            .lines()
            .filter_map(|line| line.split_once(':'))
            .filter(|(name, _)| name.trim() == key)
            .map(|(_, value)| value.trim())
            .collect();
        if values.len() != 1 {
            errors.push(format!("字段“{}”必须且只能出现一次", key));
        } else if values[0].is_empty() {
            errors.push(format!("字段“{}”不能为空", key));
        } else if key == "版本" && version(contract).is_none() {
            errors.push("版本必须是大于 0 的整数".to_string());
        }
    }
    errors
}

fn field<'a>(contract: &'a str, key: &str) -> Option<&'a str> {
    contract.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == key).then_some(value.trim())
    })
}

fn version(contract: &str) -> Option<u32> {
    field(contract, "版本")?.parse().ok().filter(|v| *v > 0)
}

// 两份契约必须分别通过输入门禁；修改业务字段时版本恰好加一。
pub fn validate_revision(previous: &str, current: &str) -> Vec<String> {
    if !validate_contract(previous).is_empty() || !validate_contract(current).is_empty() {
        return vec!["比较前，两份契约都必须通过输入门禁".to_string()];
    }
    let changed = FIELDS[1..]
        .iter()
        .any(|key| field(previous, key) != field(current, key));
    let expected = if changed {
        version(previous).and_then(|v| v.checked_add(1))
    } else {
        version(previous)
    };
    if expected.is_none() || version(current) != expected {
        return vec!["业务字段变化时版本必须加一；仅排版变化时版本应保持不变".to_string()];
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    const BASE: &str = include_str!("../examples/task.txt");

    #[test]
    fn complete_contract_passes_but_empty_expectation_is_rejected() {
        assert!(validate_contract(BASE).is_empty());
        let errors = validate_contract(include_str!("../examples/missing.txt"));
        assert!(errors.iter().any(|e| e.contains("预期结果")));
    }

    #[test]
    fn missing_duplicate_unknown_and_invalid_versions_are_rejected() {
        for bad in [
            BASE.replace("范围: 仅修改 01-cargo/src/main.rs\n", ""),
            format!("{}目标: 另一个目标\n", BASE),
            format!("{}允许: 任意文件\n", BASE),
            BASE.replace("版本: 1", "版本: 0"),
            BASE.replace("版本: 1", "版本: abc"),
        ] {
            assert!(!validate_contract(&bad).is_empty());
        }
    }

    #[test]
    fn changed_contract_requires_next_version() {
        let changed = BASE.replace("任务已接收", "任务已登记");
        assert!(!validate_revision(BASE, &changed).is_empty());
        assert!(validate_revision(BASE, include_str!("../examples/revised.txt")).is_empty());
        assert!(!validate_revision(BASE, &changed.replace("版本: 1", "版本: 3")).is_empty());
    }

    #[test]
    fn formatting_does_not_require_new_version_and_overflow_is_rejected() {
        assert!(validate_revision(BASE, &BASE.replace(": ", ":    ")).is_empty());
        assert!(!validate_revision(BASE, &BASE.replace("版本: 1", "版本: 2")).is_empty());
        let max = BASE.replace("版本: 1", "版本: 4294967295");
        assert!(!validate_revision(&max, &max.replace("任务已接收", "任务已登记")).is_empty());
    }
}
