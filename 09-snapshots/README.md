# 第 9 章：工作区隔离与快照冻结

状态：已实现工作区复制、只读冻结、源变化检测与一致性校验。本章不执行真实 Agent，也不判断代码是否正确；它只回答"候选到底是哪一份内容"。

## 为什么需要快照

编码结束后，工作区仍然是一个会变化的地方：验收命令可能生成文件，审查进程可能顺手改代码，开发者也可能继续编辑。如果验收、审查和最后交付各自读一次工作区，它们看到的可能不是同一份内容——"三项都通过"就变成了对三个不同版本的模糊结论。

快照把"候选"固定下来：复制发生在编码停止之后，验证与审查读同一棵只读的树，交付时用摘要绑定这份内容。

一个诚实的边界：快照能捕获磁盘上的一切，包括未提交修改和未跟踪文件；但编辑器里尚未保存的缓冲区不在磁盘上，任何工具都看不见。生成候选前必须停止写入并保存文件。

## 实现思路：先证明需求，再写代码

本章的重点不是"怎么复制目录"，而是演示一种实现顺序：**先定准证据需求，再倒推机制，最后用威胁清单推导测试**。

### 第一步：把"快照"翻译成三个证据需求

写代码之前，先把模糊的概念拆成可验证的命题：

| 证据需求 | 反问题 | 对应机制 |
| --- | --- | --- |
| 内容固定 | 凭什么说候选是"那一份"？ | 每个文件算 SHA-256，清单再算聚合摘要 |
| 时刻固定 | 复制过程中源还能改吗？ | 复制完立即重扫源目录并比对 |
| 之后不被改 | 验证和审查有两只手写吗？ | 冻结权限 + 验证结束时重新检查 |

这三个需求不是从"文件复制 API"正推出来的，而是从"候选必须可复核"倒推出来的。**清单摘要回答"是什么"，重扫回答"复制那一刻干不干净"，冻结 + verify 回答"之后有没有被碰过"。** 代码实现的是这三条证据链，复制只是副产品。

### 第二步：把每个需求变成一个独立机制

需求之间不能互相依赖，否则一个出问题整条链就断：

```text
需求1 ──> hash_file / compute_digest          纯计算，无状态
需求2 ──> collect_source / compare_source     两次扫描 + 一次比对
需求3 ──> freeze_tree / find_writable         一次写权限，一次读权限
Snapshot::create / verify                    只负责编排：
                                             按什么顺序调、失败如何回滚
```

这就是本章的架构分界：

| 层 | 代码 | 职责 | 为什么要分开 |
| --- | --- | --- | --- |
| 纯数据层 | `FileEntry` / `Manifest` / `SnapshotError` | 定义"长什么样"和"会怎么错"，零 IO | 不依赖文件系统，可序列化、可比较、可独立测试 |
| 机制层 | `walk` / `hash_file` / `compare_source` / `freeze_tree` / `find_writable` | 每个只做一件事，输入输出局限在一个目录 | 机制出错能在最小范围内定位；同一个机制可被 create 和 verify 复用 |
| 编排层 | `Snapshot::create` / `open` / `verify` / `remove` | 决定顺序、处理失败、清理残留 | 失败路径集中，半成品不会占用正式 id |
| 守卫层 | `Cleanup` / `TempDir` | 失败/退出时回收临时资源 | 资源生命周期与业务逻辑分离 |

分层不是形式，是测试性的来源：机制层可以单测（比如 `compare_source` 的漂移检测不需要真的"复制期间修改"），编排层再用钩子做集成测试。

### 第三步：用威胁清单推导测试，而不是测"它能不能跑"

本章 11 个测试，每一个对应一种"如果它错了会发生什么"：

```text
如果复制期间源被改了 → SourceChanged（模拟并发修改）
如果验证方先 chmod 再写 → Inconsistent（权限篡改）
如果有人改清单摘要 → Manifest 错误
如果有符号链接指向 /etc → Symlink（越界复制）
如果 id 含 ../ → InvalidId（覆盖逃逸）
如果失败后留半成品 → 正式 id 必须不存在
```

这些测试不是"功能跑通就完了"，而是问"这个机制在对抗环境下是否还成立"。**断言的反例写进测试名，断言的正例写进测试体。**

### 第四步：被否掉的方案，说明为什么选现在这条路

候选方案：

<table>
<tr><th>方案</th><th>否掉的原因</th></tr>
<tr><td>从 git 克隆或 worktree 取内容</td><td>候选是"磁盘上的未提交改动"，不是版本库状态；从 HEAD 检出会静默丢失它们，把 git 的语义错误引入快照边界</td></tr>
<tr><td>只比 大小 + mtime 证明一致</td><td>mtime 可以被 touch 伪造，大小一样不表示内容一样；SHA-256 是摘要证据，大小只是索引</td></tr>
<tr><td>chmod 000 彻底锁死</td><td>自己读不到 verify 需要的内容；444/555 保留读/执行位，只收走写</td></tr>
<tr><td>让 Snapshot 自动 Drop 删除</td><td>快照是交付物，生命周期应由调用方管理；库不该替调用方决定删除时机</td></tr>
<tr><td>保持权限原样，靠"信任"做只读</td><td>freeze 的核心是把"停止改变"变成系统约束而不是口头约定；chmod 能绕过，但 verify 能事后发现</td></tr>
</table>

实现思路的核心是：**每一个决策都能说出"为什么不是别的"和被否掉方案的致命伤。讲不出反面的决策撑不过修改。**

## 架构总览

```text
src/main.rs （演示入口，生产不调用）
    ↓  library crate
┌─────────────────────────────────────────────────────────────┐
│  编排层：Snapshot                                          │
│    create → open → verify → remove                          │
│    失败时 Cleanup 守卫回滚                                   │
└─────────────────────────────────────────────────────────────┘
    ↓ 调用
┌─────────────────────────────────────────────────────────────┐
│  机制层：三个独立的证据机制                                   │
│    collect_source → copy + hash（复制 + 逐文件摘要）          │
│    compare_source（重扫源，校验复制期间无写入）               │
│    freeze_tree / find_writable（目录树一次冻结，一次找回写位） │
└─────────────────────────────────────────────────────────────┘
    ↓ 读取
┌─────────────────────────────────────────────────────────────┐
│  纯数据层：Manifest / FileEntry / SnapshotError             │
│    serde Serialize/Deserialize，零 IO                        │
└─────────────────────────────────────────────────────────────┘
```

快照产物在磁盘上呈"目录 + 清单"配对：

```text
<root>/candidate-1/                 只读文件树（去写位）
<root>/candidate-1.manifest.json    只读清单（SHA-256 聚合摘要）
```

清单故意放在树外：树保持纯源码，清单单独只读；篡改清单时 `open` 会因摘要不匹配拒绝。

## 生命周期与失败路径

```text
Snapshot::create(source, root, id)
  ├─ validate_id → canonicalize → 存在则拒绝
  ├─ WalkExcludes（.git / target / 快照根本身）
  ├─ walk + sort → collect_source（拒绝符号链接/特殊文件）
  ├─ 复制到 <root>/.<id>.tmp-<nonce>/，逐文件 hash（下称"复制后扫描"）
  ├─ 重扫源 → compare_source → SourceChanged（帮凶：清点侦察）
  ├─ freeze_tree（分开冻结文件和目录，从深到浅）
  ├─ 写临时清单
  ├─ rename 发布 / 失败则 Cleanup 删除
  └─ Ok(Snapshot)

Snapshot::verify(root, id)
  ├─ 遍历快照 → hash → 与清单比对 changed / added / removed
  ├─ 检查写位 → writable
  ├─ 重算 digest 与清单记录比对
  ├─ 全部 0 才返回 Ok(VerifyReport{ files, digest })
  └─ 否则 Err(Inconsistent/Manifest/...)
```

失败和 Tree 守卫设计：

- `create` 成功后 `Cleanup.defuse()`，守卫失职才 delete；
- 任何 `Err` 路径里，守卫 Drop 自动清理临时目录，`remove_file_force` 删“发布前就冻结”的临时清单；
- `TempDir` 只做演示，生产代码不 drop，不留下隐形现场。

## 核心接口

| 接口 | 作用 |
| --- | --- |
| `Snapshot::create(source, root, id)` | 默认选项创建并冻结；目标已存在拒绝覆盖 |
| `Snapshot::create_with_options(..., &SnapshotOptions)` | 配置排除项、是否重扫源目录 |
| `Snapshot::open(root, id)` | 从磁盘重新加载清单；校验 id、排序与摘要，但仍需调用 `verify` |
| `Snapshot::path()` / `manifest()` / `digest()` | 只读树根、内存清单、聚合摘要 |
| `Snapshot::verify()` | 一致性检查；返回 `VerifyReport{files, digest}` |
| `Snapshot::remove()` | 恢复写权限后删除；消费 `self` 防止误用 |
| `TempDir::create(prefix)` | 测试/演示用临时目录守卫，Drop 时清理 |

清单结构（serde JSON）：

```json
{
  "snapshot_id": "candidate-1",
  "digest": "a671a2d6…",
  "entries": [
    { "path": "notes.md", "size": 46, "sha256": "83546b48…" },
    { "path": "src/main.rs", "size": 41, "sha256": "b9264169…" }
  ]
}
```

`entries` 按路径严格排序，路径统一用 `/` 分隔且必须是 UTF-8——排序让摘要可重复，UTF-8 限制避免清单无法表示的名字。非 UTF-8 路径会被 `NonUtf8Path` 拒绝。

## 运行实验

从仓库根目录运行，无需账号或网络：

```bash
cargo run -p chapter-09-snapshots
```

演示会创建临时工作区，模拟"编码已停止"的状态，然后生成快照、尝试写入并执行一致性检查。预期输出：

```text
工作区：/var/folders/.../workspace
模拟编码已停止：main.rs 是未提交改动，notes.md 是未跟踪文件。

== 快照清单 ==
    46 B  83546b48c873  notes.md
    19 B  b9f82c414bb0  src/greeting.txt
    41 B  b9264169811d  src/main.rs

快照目录：/var/folders/.../snapshots/candidate-1
清单文件：/var/folders/.../snapshots/candidate-1.manifest.json
快照摘要：a671a2d6813d7314dde68a35d4e227991326125d287211fa9cb04ab1e91d54ad

.git 与 target 未进入快照：它们不是候选内容。

== 冻结：审查方尝试写入 ==
写已有文件被拒：Permission denied (os error 13)
新建文件被拒：Permission denied (os error 13)

== 验证结束：检查快照一致性 ==
一致：3 个文件，摘要 a671a2d6813d7314dde68a35d4e227991326125d287211fa9cb04ab1e91d54ad
审查读取 main.rs：fn main() { /     println!("候选 v2"); / }

边界：只读权限拒绝普通写入；同用户进程若先 chmod 再写，verify 会发现并拒绝。
```

四个观察点：

- `notes.md` 没有经过 git 跟踪，仍然进入快照；候选不是"HEAD 的内容"；
- `.git` 与 `target` 不在清单里，排除规则生效；
- 两次写入都被操作系统拒绝，说明冻结不是口头约定；
- 摘要一致，说明验证前后是同一份候选。

对真实目录生成快照：

```bash
cargo run -p chapter-09-snapshots -- . /tmp/rast-snapshots repo-1
```

快照会保留在 `/tmp/rast-snapshots`；这是预期行为：候选交付物不因进程退出而消失。需要清理时用库的 `Snapshot::remove()`。不要让快照根目录等于源目录；快照根位于源目录内部时会被扫描排除，但最好放到仓库之外。

## 实现结构与代码地图

- [src/lib.rs](src/lib.rs)：纯数据层 + 机制层 + 编排层 + 守卫层，以及 11 个自动化测试。
- [src/main.rs](src/main.rs)：默认演示与真实目录快照两种用法。

阅读顺序推荐：

1. `FileEntry` / `Manifest` / `SnapshotOptions`：先看数据长什么样；
2. `Snapshot::create_internal`：编排层的主流程；
3. `walk` / `hash_file` / `compare_source` / `freeze_tree` / `find_writable`：每个机制层函数只干一件事；
4. `Cleanup` / `TempDir`：守卫层的资源回收；
5. `Snapshot::verify` / `open` / `remove`：验证与清理；
6. `mod tests`：对应每个威胁的反例测试。

## 边界的真实成本

**未提交不等于未保存。** 快照复制磁盘文件，未提交修改、未跟踪文件、被 `.gitignore` 忽略的文件（只要不在排除名单里）都会进入候选。编辑器里尚未落盘的内容任何外部工具都无法捕获——这是流程约束，不是本实现的缺陷。

**只读不是安全沙箱。** 文件属于当前用户，同用户进程可以先 `chmod` 再写。`verify` 能发现验证结束后留下的痕迹并拒绝，但不能阻止一个在验证过程中持续篡改的恶意进程。真正的权限隔离、只读挂载或容器边界属于第 17 章安全验收范围。

**排除规则是名称匹配。** 任何层级下名为 `.git` 或 `target` 的目录/文件都会被跳过。项目里恰好有同名源码目录时，需要用 `SnapshotOptions` 调整。排除项不进入清单，因此它们的后续变化不会触发 `SourceChanged`：候选只对纳入清单的内容负责。

**重扫的代价是双倍读取。** `verify_source` 默认开启，复制后会把源目录再完整读一遍并计算摘要。大仓库会明显变慢；关闭它能省一次读取，但也失去了"复制期间没有写入"的证据。首版选择证据优先。

**清单自身也需要锚点。** `open` 会检查清单 id、排序和摘要，但一个能同时改写条目和摘要的进程仍可伪造一份自洽的清单。要让候选摘要可追责，需要把它写到快照之外并由验收记录引用——这正是第 11 章独立验证的工作。

**崩溃窗口。** 临时目录和清单都用临时名字，正常失败会被清理；进程在 `rename` 之间崩溃时可能留下 `.<id>.tmp-*` 或临时清单。它们不会占用正式 id，但需要运维或恢复流程清理。第 14、15 章的持久化与恢复会对账这类残留。

**验证可能也需要写权限。** 快照本身是只读的；如果验收命令要生成构建产物，应把输出目录指向快照之外（例如设置独立的 target 目录），或先把快照复制成一次性的可写工作区。不要为了跑验收而解冻候选快照，否则验证前后的内容不再是同一份。

**平台差异。** Unix 上通过去掉写位得到 `Permission denied` 完成验收；非 Unix 平台只能可靠地设置文件只读，目录写保护不完整（Windows 的目录只读标志不阻止写入），因此写入拒绝测试以 `cfg(all(test, unix))` 为条件。当前验收范围是本机 macOS/Linux。

## 验证与完成标准

```bash
cargo test -p chapter-09-snapshots
cargo check --workspace
cargo fmt --all --check
```

测试覆盖（每个反例对应一个威胁场景）：

- 未提交改动、未跟踪文件进入快照，`.git` 与 `target` 被排除；
- 符号链接被拒绝，且不留下临时目录或正式清单；
- 冻结后写已有文件、新建文件、删除文件都被拒绝；
- 内容修改、新增、删除都被 `verify` 报告；
- 仅改权限、不改内容也会被 `verify` 报告；
- 复制期间源文件变化返回 `SourceChanged`，正式 id 不被占用；
- 相同内容、不同 id 得到相同聚合摘要；
- 重复 id 被拒绝；非法 id（空、`.`、`..`、含分隔符）在触碰文件系统前被拒绝；
- `open` 能重新加载并通过验证，篡改清单摘要会被拒绝；
- `TempDir` 的 `Drop` 能清理已冻结的快照。

完成本章后，你应能完成 12 件事：

1. 把"快照"拆成三条独立的证据需求（内容固定、时刻固定、之后不变）；
2. 说清每层的代码为什么值得独立存在；
3. 讲出每个被否掉方案的致命伤，而不是只知道"选了什么"；
4. 能把威胁清单映射到具体测试名，而不是反过来解释测试；
5. 解释 `hash_file` 和 `compare_source` 为什么分开，又能被 create 和 verify 复用；
6. 说明 `Snapshot` 不自动删除是怀疑而不是信任；
7. 明白只读权限只是"从不写"的约束，验证还得靠 verify 事后追偿；
8. 知道什么时候谁会需要看重扫的双倍读取代价、平台权限差异和崩溃窗口；
9. 区分 manifest 文件的"自洽"和"可信"：前者能过 digest 检查，后者要证据锚点；
10. 能独立写出每条证据链的反例，比如"SOP 里完全没有 chmod 的可能吗？"；
11. 知道 test hook（after_copy）是把"并发修改"变确定性的关键手段；
12. 回答"修改不是恶意，而是另一台 mac 在 rsync，怎么处理？"这种现实边界问题。

## 动手练习（试着改实现，不只是跑代码）

1. 加一个 `--exclude` 参数传到 `SnapshotOptions::exclude_names`，跑示例仓库，看排除项如何改变清单摘要；接着查一遍"这个排除项漏掉了什么该进快照的东西"。
2. 给 `create_internal` 的复制后钩子里加入更复杂的对抗模式（比如改相同长度但不同内容），观察 `SourceChanged` 报告的"changed"列表，思考边界；再把钩子的 closure 换成会自己读取环境的（比如从 `HashMap` 里查），感受 `FnMut` 的可变性。
3. 写一个新测试：创建一个硬链接指向快照外的文件（`link()`），实现时拒绝；再思考"硬链接是同一个文件的另一个名字"这一语义对快照意味着什么。
4. 给 `Snapshot::verify` 增加"每次报告含修改前 hash/修改后 hash"的输出，思考第 11 章直接用它作为证据记录时还多需什么。
5. 模拟 `verify_source: false` 且复制后钩子里删一个文件，观察快照仍被接受；讨论候选代表的是哪个时刻以及现实部署中多长时间能跑一轮。

参见 [Rust 新手讲解](RUST.md)。

[上一章](../08-adapters/README.md) · [下一章：接入 Codex 编码与 Claude Code 审查](../10-dual-cli/README.md) · [返回学习路线](../README.md)
