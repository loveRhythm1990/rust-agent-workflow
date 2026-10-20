# 第 9 章 Rust 配套：工作区隔离与快照冻结

状态：已实现，随本章代码更新。

## 本章代码地图

本章的代码集中在两个文件：

- [src/lib.rs](src/lib.rs)：快照的核心实现与测试；
- [src/main.rs](src/main.rs)：演示与真实目录快照。

阅读顺序建议：

1. `FileEntry` / `Manifest` / `SnapshotOptions`：先看数据长什么样；
2. `Snapshot::create_internal`：复制、重扫、冻结、发布的主流程；
3. `Snapshot::verify` / `remove`：验证与清理；
4. `walk` / `hash_file` / `freeze_tree` / `make_tree_writable`：底层文件操作；
5. 测试模块：看失败路径如何被证明。

## Path 与 PathBuf

Rust 用两个类型表示路径：

- `Path` 是借用的路径切片，类似 `&str`，不能直接持有；
- `PathBuf` 是拥有所有权的路径，类似 `String`，可以存进结构体。

本章的 `Snapshot` 用 `PathBuf` 保存快照目录，因为快照对象比创建它的临时变量活得更久：

```rust
pub struct Snapshot {
    files: PathBuf,
    manifest_path: PathBuf,
    manifest: Manifest,
}
```

拼接路径用 `join`，不要手写字符串拼接：

```rust
let final_dir = root.join(id);
let manifest_path = root.join(format!("{id}.manifest.json"));
```

`join` 会自动处理分隔符。注意 `id` 已经过 `validate_id`，不允许 `/`、`\`、`.` 和 `..`，否则 `join` 会把快照写到根目录之外。

`canonicalize` 把路径变成绝对路径，并解析符号链接。它要求路径已经存在：

```rust
let source = source
    .canonicalize()
    .map_err(|error| io_error("解析源目录", source, error))?;
```

本章用它做两件事：得到可比较的绝对路径，以及判断快照根是否位于源目录内部：

```rust
if root.starts_with(source) {
    vec![root.to_path_buf()]
} else {
    Vec::new()
}
```

`Path::starts_with` 按组件比较，不是字符串前缀；`/tmp/ws2` 不会被误判为 `/tmp/ws` 的子路径。

`strip_prefix` 用来求相对路径：

```rust
let relative = path
    .strip_prefix(base)
    .map_err(|_| SnapshotError::InvalidPath(path.clone()))?;
```

`path_key` 进一步把相对路径变成清单里的字符串：

```rust
fn path_key(path: &Path) -> Result<String, SnapshotError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or_else(|| SnapshotError::NonUtf8Path(path.to_path_buf()))?;
                parts.push(value.to_string());
            }
            Component::CurDir => {}
            _ => return Err(SnapshotError::InvalidPath(path.to_path_buf())),
        }
    }
    Ok(parts.join("/"))
}
```

`components()` 把路径拆成平台无关的组件；`Normal` 是普通名字，`CurDir` 是 `.`，`ParentDir` 是 `..`，`RootDir` 是根。清单只接受 `Normal`，其余一律拒绝。路径统一用 `/` 连接，避免 macOS 与 Windows 生成不同摘要。

打印路径用 `path.display()`，它不要求路径是合法 UTF-8；只有在必须写入清单时才用 `to_str()` 并显式拒绝非 UTF-8。不要用 `to_string_lossy()` 偷偷替换字符——两个不同的非 UTF-8 名字可能变成同一个字符串。

## 遍历目录

`fs::read_dir` 返回一个迭代器，每一项是 `io::Result<DirEntry>`：

```rust
let mut entries = Vec::new();
for entry in fs::read_dir(dir).map_err(|error| io_error("读取目录", dir, error))? {
    entries.push(entry.map_err(|error| io_error("读取目录项", dir, error))?);
}
entries.sort_by_key(|entry| entry.file_name());
```

先把所有目录项收进 `Vec` 再排序，是因为 `ReadDir` 的顺序由文件系统决定，不可重复。排序后清单和摘要才是确定的。

`DirEntry::file_type()` 读取目录项自身的类型，不跟随符号链接；这正是本章需要的：

```rust
let file_type = entry
    .file_type()
    .map_err(|error| io_error("读取文件类型", &path, error))?;
if file_type.is_symlink() {
    return Err(SnapshotError::Symlink(path));
}
if file_type.is_dir() {
    walk(&path, base, excludes, out)?;
} else if file_type.is_file() {
    // 记录文件
} else {
    return Err(SnapshotError::UnsupportedFileType(path));
}
```

如果用 `fs::metadata`（会跟随符号链接），一个指向 `/etc/passwd` 的链接就会被当成普通文件复制进候选。`symlink_metadata` 不跟随链接；本章在扫描和删除时都使用它。

递归函数用 `&mut Vec<SourceFile>` 收集结果：

```rust
fn walk(
    dir: &Path,
    base: &Path,
    excludes: &WalkExcludes,
    out: &mut Vec<SourceFile>,
) -> Result<(), SnapshotError>
```

这比返回 `Vec` 再层层拼接更省分配，也更容易在错误处直接返回。

排除项用 `BTreeSet<String>` 保存名称，查询是 `O(log n)`，同时保证行为可预测：

```rust
if let Some(name) = name.to_str()
    && self.names.contains(name)
{
    return true;
}
```

这里的 `if let ... && ...` 是 let-chain：第一个条件匹配成功后，第二个条件才求值。它在 Rust 2024 edition 中可用，读起来比嵌套 `if` 简洁。

## 文件操作与错误上下文

本章用到的标准库文件操作：

| 操作 | 用途 |
| --- | --- |
| `fs::create_dir_all` | 创建快照根和子目录 |
| `fs::create_dir` | 创建临时目录；目标已存在时失败，用于唯一性竞争 |
| `fs::copy` | 复制普通文件 |
| `fs::read` | 读取清单 |
| `fs::write` | 写清单 |
| `fs::rename` | 发布临时目录/清单 |
| `fs::remove_dir_all` | 删除目录树 |
| `fs::remove_file` | 删除清单 |

每个操作都可能失败，本章不把错误压成一个字符串，而是保留“哪个动作、哪个路径、底层错误”：

```rust
#[derive(Debug)]
pub enum SnapshotError {
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    // ...
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> SnapshotError {
    SnapshotError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}
```

调用处用 `map_err` 添加上下文，再用 `?` 提前返回：

```rust
fs::create_dir(&tmp_dir)
    .map_err(|error| io_error("创建临时快照目录", &tmp_dir, error))?;
```

`?` 会在错误类型不一致时尝试 `From` 转换；本章大多数错误已经是 `SnapshotError`，所以 `map_err` 之后可以直接 `?`。

给错误实现 `Display` 决定用户看到什么，实现 `Error::source` 保留底层错误链：

```rust
impl std::error::Error for SnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SnapshotError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
```

`..` 在模式里忽略其余字段；`&'static str` 的 `action` 不拥有字符串，适合固定的动作名。

清理函数对“文件已经不存在”是幂等的：

```rust
match fs::remove_file(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error),
}
```

失败路径清理时，目标可能已被另一段清理代码删掉；把 `NotFound` 当作成功，避免用“清理失败”掩盖真正的原始错误。

## 权限：冻结与恢复

Unix 权限用 `PermissionsExt` 操作：

```rust
#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), SnapshotError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| io_error("设置只读权限", path, error))
}
```

冻结规则：

- 文件与目录都去掉所有写位，但保留原有的读/执行位与属主/组/其他的访问类别；
- 普通源码文件通常从 `0644` 变成 `0444`；
- 可执行脚本从 `0755` 变成 `0555`，仍然可以执行；
- 目录通常从 `0755` 变成 `0555`，私有目录从 `0700` 变成 `0500`，不会被放宽给其他用户。

实现上不是写死 `0444` / `0555`，而是读取原权限后计算：

```rust
let mode = (metadata.permissions().mode() & !0o222) | 0o400; // 文件：保留读/执行位
let mode = (metadata.permissions().mode() & !0o222) | 0o500; // 目录：保留读/进入位
```

`!0o222` 清掉三个写位，`| 0o400` / `| 0o500` 保证属主至少能读（目录还要能进入），否则验证阶段连文件内容都读不到。目录的写权限控制“能否修改目录内容”，文件的写权限控制“能否修改文件内容”。所以只冻结文件不够：目录仍可写时，进程可以删除文件或新建文件。`freeze_tree` 先冻结文件，再把目录按深度从深到浅设为只读，最后是快照根；如果先冻结父目录，后面就没法修改子目录的权限了。

检查可写位：

```rust
#[cfg(unix)]
fn is_writable(metadata: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o222 != 0
}
```

`0o222` 是三个写位的掩码。`verify` 用 `find_writable` 遍历快照，任何文件或目录只要还有写位就进入 `writable` 列表。

清理已冻结的目录前必须先恢复写权限：

```rust
let mode = if metadata.is_dir() { 0o700 } else { 0o600 };
fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
```

`remove_dir_all` 需要父目录可写；文件本身只读时，删除操作由父目录权限决定，但先恢复权限能让清理在更多平台上一致工作。

非 Unix 平台没有 `PermissionsExt`，代码用 `set_readonly(true)` 退化处理：

```rust
#[cfg(not(unix))]
fn set_read_only(path: &Path) -> Result<(), SnapshotError> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions)
}
```

Windows 上目录的只读标志并不阻止写入，因此“写入被拒”的测试只在 Unix 上运行。这不是遗漏，而是平台能力的真实差异。

### 只读不是沙箱

文件属于当前用户，当前用户的进程可以：

1. `chmod` 去掉只读位；
2. 写入文件；
3. 再把权限改回去。

`verify` 能发现第 2 步留下的内容变化，也能发现权限没有恢复；但它无法阻止一个在验证过程中持续篡改的进程。真正的隔离需要不同的用户、只读挂载、容器或系统调用过滤，属于第 17 章的安全验收。本章的只读是流程约束和事后证据，不是安全边界。

## Drop 与资源清理

`TempDir` 是 RAII 守卫：

```rust
pub struct TempDir {
    path: PathBuf,
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = remove_tree(&self.path);
    }
}
```

创建后无论函数正常返回还是提前 `?` 返回，`Drop` 都会执行。`Drop` 里不能返回错误，所以用 `let _ =` 忽略清理失败；测试和演示不应因为清理失败而掩盖主流程结果。

`Cleanup` 用于复制过程中的临时目录，多了一个 `armed` 标志：

```rust
struct Cleanup {
    path: PathBuf,
    armed: bool,
}

impl Cleanup {
    fn defuse(&mut self) {
        self.armed = false;
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = remove_tree(&self.path);
        }
    }
}
```

发布成功后调用 `defuse`，守卫不再删除正式目录；发布前任何错误返回都会触发清理。这是“失败不留下半成品”的实现方式。

`Snapshot` 故意不实现 `Drop`：快照是候选交付物，进程退出后仍应存在，删除必须由调用方显式调用 `remove(self)`。`remove` 接收 `self`，调用后快照值被移动、不能再使用——这能防止“删除后继续读”的误用：

```rust
pub fn remove(self) -> Result<(), SnapshotError> {
    // ...
}
```

## SHA-256 与 serde

本章用 `sha2` crate 计算内容摘要：

```rust
use sha2::{Digest, Sha256};

let mut hasher = Sha256::new();
hasher.update(b"hello");
let digest: [u8; 32] = hasher.finalize().into();
```

`Digest` 是 trait，提供 `new`、`update`、`finalize` 等方法；`Sha256` 实现它。读文件时分块更新，避免把大文件整个读进内存：

```rust
let mut buffer = [0u8; HASH_BUFFER_BYTES];
loop {
    let read = file.read(&mut buffer)?;
    if read == 0 {
        break;
    }
    hasher.update(&buffer[..read]);
    size += read as u64;
}
```

`&buffer[..read]` 只取本次真正读到的字节；缓冲区剩余部分是旧内容，不能一起喂给哈希。

清单用 serde 序列化：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}
```

`Serialize` 让结构体变成 JSON，`Deserialize` 让 JSON 变回结构体。`serde_json::to_vec_pretty` 生成带缩进的字节，`serde_json::from_slice` 解析。

聚合摘要不是简单地把所有文件摘要拼起来，而是按固定格式喂给哈希：

```rust
hasher.update(entry.path.as_bytes());
hasher.update([0u8]);
hasher.update(entry.size.to_be_bytes());
hasher.update(entry.sha256.as_bytes());
hasher.update(b"\n");
```

分隔符和固定长度编码避免“路径和摘要拼在一起产生歧义”。条目必须先按路径排序，否则同样的内容会因遍历顺序不同得到不同摘要。

## 迭代器、集合与闭包

本章大量使用迭代器链：

```rust
let known: BTreeSet<&str> = self
    .manifest
    .entries
    .iter()
    .map(|entry| entry.path.as_str())
    .collect();

let added: Vec<String> = actual
    .keys()
    .filter(|path| !known.contains(path.as_str()))
    .cloned()
    .collect();
```

- `iter()` 借用元素，不消耗集合；
- `map` 转换，`filter` 过滤，`collect` 收进目标容器；
- `cloned()` 把 `&String` 变成 `String`；
- 类型标注帮助 `collect` 推断目标容器。

`BTreeMap` 按 key 排序，`BTreeSet` 去重且有序；本章用它们比较“清单里的文件”和“磁盘上的文件”，差异天然按路径排序，报告稳定。

排序用闭包：

```rust
entries.sort_by(|left, right| left.path.cmp(&right.path));
dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
```

`sort_by_key` 适合从元素直接算一个可比较的 key；`Reverse` 把升序变降序，用来先处理最深的目录。

测试钩子用了 trait object 闭包：

```rust
fn create_internal(
    // ...
    mut after_copy: Option<&mut dyn FnMut(&Path)>,
) -> Result<Self, SnapshotError>
```

`dyn FnMut(&Path)` 表示“可以反复调用的闭包”，`&mut` 允许闭包修改捕获的环境。测试传入一个修改源文件的闭包，生产路径传 `None`。这比在公开 API 上暴露测试专用字段更干净。

## 测试：证明拒绝路径

本章测试在 `src/lib.rs` 的 `mod tests` 中，使用 `use super::*;` 访问私有函数。测试统一通过 `TempDir` 创建隔离现场：

```rust
fn temp_dir() -> TempDir {
    TempDir::create("chapter-09-test").expect("临时目录应能创建")
}
```

失败路径用模式匹配断言具体错误，而不是只看“是不是 Err”：

```rust
match snapshot.verify() {
    Err(SnapshotError::Inconsistent { changed, added, removed, writable }) => {
        assert!(changed.contains(&"src/main.rs".to_string()));
        // ...
    }
    other => panic!("应检测到不一致，实际：{other:?}"),
}
```

`match` 要求覆盖所有情况；`other => panic!` 让意外成功也变成测试失败。`matches!` 宏适合只关心变体的场景：

```rust
assert!(matches!(error, SnapshotError::Symlink(_)));
```

权限相关测试依赖 Unix 权限语义，因此整个测试模块标注 `#[cfg(all(test, unix))]`，在非 Unix 平台不会编译这些断言。平台条件编译不是“跳过失败”，而是承认平台能力不同。

测试覆盖的失败路径包括：

- 符号链接被拒绝；
- 冻结后写入、新建、删除都失败；
- 内容修改、新增、删除被 `verify` 发现；
- 只改权限被 `verify` 发现；
- 复制期间源变化返回 `SourceChanged`；
- 重复 id、非法 id 被拒绝；
- 篡改清单摘要被 `open` 拒绝；
- `TempDir` 能清理冻结目录。

运行：

```bash
cargo test -p chapter-09-snapshots
cargo check --workspace
cargo fmt --all --check
```

## 常见错误

1. **直接 `remove_dir_all` 冻结目录**：会得到 `Permission denied`。先 `make_tree_writable` 恢复目录写权限。
2. **用 `Path::exists` 判断是否可覆盖**：它跟随符号链接，悬空链接会返回 `false`。用 `symlink_metadata` 判断“名字是否已被占用”。
3. **忘记排序**：`read_dir` 顺序不稳定，清单摘要会随文件系统变化，无法复现。
4. **用 `to_string_lossy` 生成路径 key**：两个不同的非 UTF-8 路径可能变成同一个字符串；本章直接拒绝非 UTF-8。
5. **先冻结父目录**：之后无法修改子目录权限。`freeze_tree` 从深到浅处理。
6. **把只读当沙箱**：同用户进程可以先 `chmod`；需要靠 `verify` 事后发现，真正的隔离另做。
7. **复制时源文件正在变化**：`fs::copy` 不提供一致性保证；本章复制后重扫源目录并比对摘要，发现变化就拒绝。
8. **在 `Drop` 里传播错误**：`Drop` 不能返回 `Result`；清理失败只能忽略或记录，不能改变主流程的返回值。

## 动手练习

1. 把 `SnapshotOptions::exclude_names` 接到命令行参数，观察新增排除项后清单和摘要的变化。
2. 给 `verify` 增加一个只检查大小和修改时间的快速模式，思考它比 SHA-256 弱在哪里、什么时候可以接受。
3. 写一个测试：冻结后 `chmod` 一个文件再改回原权限，但内容不变；验证 `verify` 是否仍能通过。再改成内容变化，观察结果。
4. 给 `Snapshot` 增加 `files()` 方法，返回按路径排序的只读文件列表；比较返回 `Vec<&FileEntry>` 与迭代器的取舍。
5. 用 `std::time::Instant` 测量大目录快照的复制与重扫耗时，验证 README 中“双倍读取”的成本描述。

[返回本章](README.md)
