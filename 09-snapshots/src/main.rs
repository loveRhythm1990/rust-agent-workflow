use chapter_09_snapshots::{Snapshot, TempDir};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [] => demo(),
        [source, root, id] => snapshot_existing(Path::new(source), Path::new(root), id),
        _ => {
            eprintln!("用法：cargo run -p chapter-09-snapshots");
            eprintln!("  或：cargo run -p chapter-09-snapshots -- <源目录> <快照根目录> <快照 id>");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("快照失败：{error}");
            ExitCode::FAILURE
        }
    }
}

/// 演示：生成一个含未提交改动的工作区，复制并冻结，然后尝试写入和检查一致性。
fn demo() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::create("chapter-09-snapshots")?;
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(workspace.join("src"))?;
    fs::write(
        workspace.join("src/main.rs"),
        "fn main() {\n    println!(\"候选 v2\");\n}\n",
    )?;
    fs::write(workspace.join("src/greeting.txt"), "你好，快照！\n")?;
    fs::write(
        workspace.join("notes.md"),
        "未提交的改动也属于工作区状态。\n",
    )?;
    fs::create_dir_all(workspace.join(".git"))?;
    fs::write(workspace.join(".git/HEAD"), "ref: refs/heads/main\n")?;
    fs::create_dir_all(workspace.join("target/debug"))?;
    fs::write(
        workspace.join("target/debug/build.log"),
        "构建产物不进快照\n",
    )?;

    println!("工作区：{}", workspace.display());
    println!("模拟编码已停止：main.rs 是未提交改动，notes.md 是未跟踪文件。\n");

    let root = temp.path().join("snapshots");
    let snapshot = Snapshot::create(&workspace, &root, "candidate-1")?;

    println!("== 快照清单 ==");
    for entry in &snapshot.manifest().entries {
        println!(
            "{:>6} B  {}  {}",
            entry.size,
            &entry.sha256[..12],
            entry.path
        );
    }
    println!("\n快照目录：{}", snapshot.path().display());
    println!("清单文件：{}", snapshot.manifest_path().display());
    println!("快照摘要：{}", snapshot.digest());
    println!("\n.git 与 target 未进入快照：它们不是候选内容。\n");

    println!("== 冻结：审查方尝试写入 ==");
    let file = snapshot.path().join("src/main.rs");
    match fs::write(&file, "审查方偷偷改了源码") {
        Ok(()) => println!("写入成功？冻结失败，这是 bug。"),
        Err(error) => println!("写已有文件被拒：{error}"),
    }
    match fs::write(snapshot.path().join("src/injected.rs"), "new") {
        Ok(()) => println!("新建文件成功？冻结失败，这是 bug。"),
        Err(error) => println!("新建文件被拒：{error}"),
    }

    println!("\n== 验证结束：检查快照一致性 ==");
    let report = snapshot.verify()?;
    println!("一致：{} 个文件，摘要 {}", report.files, report.digest);
    let content = fs::read_to_string(snapshot.path().join("src/main.rs"))?;
    println!("审查读取 main.rs：{}", content.trim().replace('\n', " / "));

    println!("\n边界：只读权限拒绝普通写入；同用户进程若先 chmod 再写，verify 会发现并拒绝。");
    snapshot.remove()?;
    Ok(())
}

/// 对真实目录生成快照。快照是候选交付物，默认保留，由调用方决定何时删除。
fn snapshot_existing(source: &Path, root: &Path, id: &str) -> Result<(), Box<dyn Error>> {
    let snapshot = Snapshot::create(source, root, id)?;
    println!("快照 {} 已创建并冻结。", snapshot.id());
    println!("目录：{}", snapshot.path().display());
    println!("清单：{}", snapshot.manifest_path().display());
    println!("摘要：{}", snapshot.digest());
    for entry in &snapshot.manifest().entries {
        println!("  {}  {}", entry.sha256, entry.path);
    }
    let report = snapshot.verify()?;
    println!(
        "一致性检查通过：{} 个文件，摘要 {}",
        report.files, report.digest
    );
    Ok(())
}
