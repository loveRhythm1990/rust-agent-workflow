// main 是可执行程序的入口，运行程序时从这里开始。
fn main() {
    // let 创建变量；这里保存的是字符串字面量。
    let project_name = "rust-agent-workflow";
    let chapter = 1;

    println!("你好，Rust！");
    // {} 是占位符，输出时由后面的值替换。
    println!("项目：{}", project_name);
    println!("第 {} 章：Cargo 与第一个程序", chapter);

    print_learning_goal();
}

// 把一项操作放入函数，再通过函数名调用它。
fn print_learning_goal() {
    println!("学习目标：用 Rust 构建可验证的多 Agent 协作工具");
}
