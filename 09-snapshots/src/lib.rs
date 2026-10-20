//! 工作区快照：把磁盘上的工作区复制成只读候选，并校验复制前后一致。
//!
//! 快照记录的是磁盘状态：已提交、未提交和未跟踪的文件都在内。
//! 编辑器里尚未保存的缓冲区不在磁盘上，任何工具都无法捕获；
//! 因此“生成快照”之前必须先停止写入并保存文件。
//!
//! 复制完成后会再次扫描源目录：如果复制期间源工作区发生变化，
//! 本次快照被拒绝，不会把两个时刻的内容混成一个候选。
//! 冻结使用只读权限；它不是安全沙箱（同用户进程仍可先 chmod 再写），
//! 但 `verify` 会在验证结束后发现内容或权限被改动。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ID_LEN: usize = 64;
const HASH_BUFFER_BYTES: usize = 64 * 1024;

/// 清单中的一个文件：路径相对于快照根，使用 `/` 分隔。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    /// 文件内容的 SHA-256，十六进制小写。
    pub sha256: String,
}

/// 快照清单：候选内容的完整文件列表与聚合摘要。
///
/// `digest` 只由排序后的条目计算，不包含快照 id；
/// 同样内容在不同 id 下应得到相同摘要，方便绑定候选版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub snapshot_id: String,
    pub digest: String,
    pub entries: Vec<FileEntry>,
}

impl Manifest {
    pub fn entry(&self, path: &str) -> Option<&FileEntry> {
        self.entries.iter().find(|entry| entry.path == path)
    }
}

/// 快照复制选项。
#[derive(Debug, Clone)]
pub struct SnapshotOptions {
    /// 按名称排除的目录或文件；默认排除版本库元数据与构建产物。
    pub exclude_names: Vec<String>,
    /// 复制结束后重扫源目录；默认开启，关闭后无法证明复制期间没有写入。
    pub verify_source: bool,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            exclude_names: vec![".git".to_string(), "target".to_string()],
            verify_source: true,
        }
    }
}

/// 一致性检查通过后的报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyReport {
    pub files: usize,
    pub digest: String,
}

/// 快照操作失败的原因：每种失败都明确拒绝，不产生半成品候选。
#[derive(Debug)]
pub enum SnapshotError {
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    /// 源路径不是目录。
    NotDirectory(PathBuf),
    /// 快照 id 含路径分隔符、为空或过长。
    InvalidId(String),
    /// 同名的目录或清单已经存在，拒绝覆盖。
    AlreadyExists(PathBuf),
    /// 快照不跟随符号链接，避免把工作区外内容带进候选。
    Symlink(PathBuf),
    /// 只接受普通文件和目录。
    UnsupportedFileType(PathBuf),
    /// 路径无法用 UTF-8 表示，清单无法可靠记录。
    NonUtf8Path(PathBuf),
    /// 相对路径出现了 `..` 或平台前缀等异常组件。
    InvalidPath(PathBuf),
    /// 快照根目录不能等于源目录。
    InvalidRoot(PathBuf),
    /// 复制期间源工作区发生变化。
    SourceChanged {
        changed: Vec<String>,
        added: Vec<String>,
        removed: Vec<String>,
    },
    /// 验证结束后快照内容或只读权限被改动。
    Inconsistent {
        changed: Vec<String>,
        added: Vec<String>,
        removed: Vec<String>,
        writable: Vec<String>,
    },
    /// 清单文件损坏、id 不匹配或摘要对不上。
    Manifest(String),
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SnapshotError::Io {
                action,
                path,
                source,
            } => write!(f, "{}失败（{}）：{}", action, path.display(), source),
            SnapshotError::NotDirectory(path) => write!(f, "源不是目录：{}", path.display()),
            SnapshotError::InvalidId(id) => write!(
                f,
                "快照 id 不合法：{id:?}（只允许字母、数字、-、_、.，长度不超过 {MAX_ID_LEN}）"
            ),
            SnapshotError::AlreadyExists(path) => {
                write!(f, "快照已存在，拒绝覆盖：{}", path.display())
            }
            SnapshotError::Symlink(path) => {
                write!(f, "快照不跟随符号链接：{}", path.display())
            }
            SnapshotError::UnsupportedFileType(path) => write!(
                f,
                "快照只接受普通文件和目录，发现其他类型：{}",
                path.display()
            ),
            SnapshotError::NonUtf8Path(path) => {
                write!(f, "路径不是 UTF-8，无法写入清单：{}", path.display())
            }
            SnapshotError::InvalidPath(path) => {
                write!(f, "相对路径含异常组件：{}", path.display())
            }
            SnapshotError::InvalidRoot(path) => write!(
                f,
                "快照根目录不能等于源目录：{}（请放到源目录之外）",
                path.display()
            ),
            SnapshotError::SourceChanged {
                changed,
                added,
                removed,
            } => write!(
                f,
                "复制期间源工作区发生变化，快照被拒绝：修改 [{}]，新增 [{}]，删除 [{}]",
                join(&changed),
                join(&added),
                join(&removed)
            ),
            SnapshotError::Inconsistent {
                changed,
                added,
                removed,
                writable,
            } => write!(
                f,
                "快照一致性检查失败：内容变化 [{}]，新增 [{}]，删除 [{}]，可写项 [{}]",
                join(&changed),
                join(&added),
                join(&removed),
                join(&writable)
            ),
            SnapshotError::Manifest(detail) => write!(f, "快照清单不可信：{detail}"),
        }
    }
}

impl std::error::Error for SnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SnapshotError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn join(items: &[String]) -> String {
    if items.is_empty() {
        "无".to_string()
    } else {
        items.join(", ")
    }
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> SnapshotError {
    SnapshotError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

/// 一个已冻结的工作区快照。
///
/// 快照目录与清单都位于用户指定的快照根目录下：
///
/// ```text
/// <root>/candidate-1/                    只读文件树（去掉写位，保留读/执行位）
/// <root>/candidate-1.manifest.json       只读清单
/// ```
///
/// `Snapshot` 被丢弃时不会自动删除：快照是候选交付物，生命周期由调用方管理。
#[derive(Debug)]
pub struct Snapshot {
    files: PathBuf,
    manifest_path: PathBuf,
    manifest: Manifest,
}

impl Snapshot {
    /// 用默认选项创建并冻结快照。目标已存在时拒绝覆盖。
    pub fn create(source: &Path, root: &Path, id: &str) -> Result<Self, SnapshotError> {
        Self::create_with_options(source, root, id, &SnapshotOptions::default())
    }

    pub fn create_with_options(
        source: &Path,
        root: &Path,
        id: &str,
        options: &SnapshotOptions,
    ) -> Result<Self, SnapshotError> {
        Self::create_internal(source, root, id, options, None)
    }

    /// 内部入口保留一个复制后、重扫前的钩子，测试用它模拟复制期间的源写入。
    fn create_internal(
        source: &Path,
        root: &Path,
        id: &str,
        options: &SnapshotOptions,
        mut after_copy: Option<&mut dyn FnMut(&Path)>,
    ) -> Result<Self, SnapshotError> {
        validate_id(id)?;

        let source = source
            .canonicalize()
            .map_err(|error| io_error("解析源目录", source, error))?;
        if !source.is_dir() {
            return Err(SnapshotError::NotDirectory(source));
        }

        fs::create_dir_all(root).map_err(|error| io_error("创建快照根目录", root, error))?;
        let root = root
            .canonicalize()
            .map_err(|error| io_error("解析快照根目录", root, error))?;
        if root == source {
            return Err(SnapshotError::InvalidRoot(root));
        }

        let final_dir = root.join(id);
        let manifest_path = root.join(format!("{id}.manifest.json"));
        if fs::symlink_metadata(&final_dir).is_ok() || fs::symlink_metadata(&manifest_path).is_ok()
        {
            return Err(SnapshotError::AlreadyExists(final_dir));
        }

        let excludes = WalkExcludes::for_source(options, &source, &root);
        let source_files = collect_source(&source, &excludes)?;

        let tmp_dir = root.join(format!(".{id}.tmp-{}", unique_suffix()));
        fs::create_dir(&tmp_dir).map_err(|error| io_error("创建临时快照目录", &tmp_dir, error))?;
        let mut cleanup = Cleanup::new(tmp_dir.clone());

        let mut entries = Vec::with_capacity(source_files.len());
        for file in &source_files {
            let target = tmp_dir.join(&file.rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| io_error("创建快照子目录", parent, error))?;
            }
            fs::copy(&file.abs, &target).map_err(|error| io_error("复制文件", &file.abs, error))?;
            let (size, sha256) = hash_file(&target)?;
            entries.push(FileEntry {
                path: file.rel.clone(),
                size,
                sha256,
            });
        }

        if let Some(callback) = after_copy.as_mut() {
            (**callback)(&source);
        }

        if options.verify_source {
            let after = collect_source(&source, &excludes)?;
            let drift = compare_source(&entries, &after)?;
            if !drift.is_empty() {
                return Err(SnapshotError::SourceChanged {
                    changed: drift.changed,
                    added: drift.added,
                    removed: drift.removed,
                });
            }
        }

        entries.sort_by(|left, right| left.path.cmp(&right.path));
        let digest = compute_digest(&entries);
        let manifest = Manifest {
            snapshot_id: id.to_string(),
            digest,
            entries,
        };
        validate_manifest(&manifest)?;

        freeze_tree(&tmp_dir)?;

        let json = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| SnapshotError::Manifest(format!("清单序列化失败：{error}")))?;
        let tmp_manifest = root.join(format!(".{id}.manifest.tmp-{}", unique_suffix()));
        fs::write(&tmp_manifest, json)
            .map_err(|error| io_error("写快照清单", &tmp_manifest, error))?;
        if let Err(error) = set_read_only(&tmp_manifest) {
            let _ = remove_file_force(&tmp_manifest);
            return Err(error);
        }

        // 目录和清单都先写成临时名字，成功后再发布，避免半成品占用正式 id。
        if let Err(error) = fs::rename(&tmp_dir, &final_dir)
            .map_err(|error| io_error("发布快照目录", &final_dir, error))
        {
            let _ = remove_file_force(&tmp_manifest);
            return Err(error);
        }
        cleanup.defuse();
        if let Err(error) = fs::rename(&tmp_manifest, &manifest_path)
            .map_err(|error| io_error("发布快照清单", &manifest_path, error))
        {
            let _ = remove_tree(&final_dir);
            let _ = remove_file_force(&tmp_manifest);
            return Err(error);
        }

        Ok(Self {
            files: final_dir,
            manifest_path,
            manifest,
        })
    }

    /// 从磁盘重新加载一个已存在的快照；加载后仍需调用 `verify`。
    pub fn open(root: &Path, id: &str) -> Result<Self, SnapshotError> {
        validate_id(id)?;
        let files = root.join(id);
        let manifest_path = root.join(format!("{id}.manifest.json"));
        let bytes = fs::read(&manifest_path)
            .map_err(|error| io_error("读取清单", &manifest_path, error))?;
        let manifest: Manifest = serde_json::from_slice(&bytes)
            .map_err(|error| SnapshotError::Manifest(format!("清单不是合法 JSON：{error}")))?;
        if manifest.snapshot_id != id {
            return Err(SnapshotError::Manifest(format!(
                "清单 id 为 {:?}，与请求 {id:?} 不符",
                manifest.snapshot_id
            )));
        }
        validate_manifest(&manifest)?;
        if !files.is_dir() {
            return Err(SnapshotError::NotDirectory(files));
        }
        Ok(Self {
            files,
            manifest_path,
            manifest,
        })
    }

    pub fn id(&self) -> &str {
        &self.manifest.snapshot_id
    }

    /// 只读文件树的根目录。
    pub fn path(&self) -> &Path {
        &self.files
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// 候选内容的聚合摘要，可绑定到验收证据。
    pub fn digest(&self) -> &str {
        &self.manifest.digest
    }

    /// 重新计算每个文件的内容摘要，并检查所有文件与目录仍是只读。
    ///
    /// 验证或审查结束后调用；任何内容变化、增删或权限松动都会返回 Err。
    pub fn verify(&self) -> Result<VerifyReport, SnapshotError> {
        let mut actual: BTreeMap<String, (u64, String)> = BTreeMap::new();
        let mut files = Vec::new();
        walk(&self.files, &self.files, &WalkExcludes::none(), &mut files)?;
        for file in files {
            let (size, sha256) = hash_file(&file.abs)?;
            actual.insert(file.rel, (size, sha256));
        }

        let mut changed = Vec::new();
        let mut removed = Vec::new();
        for entry in &self.manifest.entries {
            match actual.get(&entry.path) {
                Some((size, sha256)) if *size == entry.size && *sha256 == entry.sha256 => {}
                Some(_) => changed.push(entry.path.clone()),
                None => removed.push(entry.path.clone()),
            }
        }
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
        let writable = find_writable(&self.files)?;

        if !changed.is_empty() || !added.is_empty() || !removed.is_empty() || !writable.is_empty() {
            return Err(SnapshotError::Inconsistent {
                changed,
                added,
                removed,
                writable,
            });
        }

        let digest = compute_digest(&self.manifest.entries);
        if digest != self.manifest.digest {
            return Err(SnapshotError::Manifest(
                "条目摘要与清单记录的 digest 不一致".to_string(),
            ));
        }

        Ok(VerifyReport {
            files: actual.len(),
            digest,
        })
    }

    /// 恢复写权限并删除快照目录与清单。
    pub fn remove(self) -> Result<(), SnapshotError> {
        remove_tree(&self.files).map_err(|error| io_error("删除快照目录", &self.files, error))?;
        remove_file_force(&self.manifest_path)
            .map_err(|error| io_error("删除快照清单", &self.manifest_path, error))?;
        Ok(())
    }
}

/// 临时目录守卫：离开作用域时递归删除，测试和演示用它避免留下现场。
///
/// 删除前先恢复写权限，因此可以清理已经冻结的快照。
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn create(prefix: &str) -> io::Result<Self> {
        let prefix = prefix.replace('/', "-").replace('\\', "-");
        let base = std::env::temp_dir();
        for _ in 0..100 {
            let path = base.join(format!("{prefix}-{}", unique_suffix()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "无法创建唯一的临时目录",
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = remove_tree(&self.path);
    }
}

struct SourceFile {
    /// 相对源目录的路径，已规范为 `/` 分隔。
    rel: String,
    abs: PathBuf,
}

struct SourceDrift {
    changed: Vec<String>,
    added: Vec<String>,
    removed: Vec<String>,
}

impl SourceDrift {
    fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.added.is_empty() && self.removed.is_empty()
    }
}

struct WalkExcludes {
    names: BTreeSet<String>,
    roots: Vec<PathBuf>,
}

impl WalkExcludes {
    fn none() -> Self {
        Self {
            names: BTreeSet::new(),
            roots: Vec::new(),
        }
    }

    fn for_source(options: &SnapshotOptions, source: &Path, root: &Path) -> Self {
        let roots = if root.starts_with(source) {
            vec![root.to_path_buf()]
        } else {
            Vec::new()
        };
        Self {
            names: options.exclude_names.iter().cloned().collect(),
            roots,
        }
    }

    fn matches(&self, path: &Path, name: &OsStr) -> bool {
        if let Some(name) = name.to_str()
            && self.names.contains(name)
        {
            return true;
        }
        self.roots.iter().any(|root| path.starts_with(root))
    }
}

/// 复制前的源目录扫描；只记录普通文件，符号链接与其他类型直接拒绝。
fn collect_source(
    source: &Path,
    excludes: &WalkExcludes,
) -> Result<Vec<SourceFile>, SnapshotError> {
    let mut files = Vec::new();
    walk(source, source, excludes, &mut files)?;
    files.sort_by(|left, right| left.rel.cmp(&right.rel));
    Ok(files)
}

fn walk(
    dir: &Path,
    base: &Path,
    excludes: &WalkExcludes,
    out: &mut Vec<SourceFile>,
) -> Result<(), SnapshotError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).map_err(|error| io_error("读取目录", dir, error))? {
        entries.push(entry.map_err(|error| io_error("读取目录项", dir, error))?);
    }
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        if name.to_str().is_none() {
            return Err(SnapshotError::NonUtf8Path(path));
        }
        if excludes.matches(&path, &name) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| io_error("读取文件类型", &path, error))?;
        if file_type.is_symlink() {
            return Err(SnapshotError::Symlink(path));
        }
        if file_type.is_dir() {
            walk(&path, base, excludes, out)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(base)
                .map_err(|_| SnapshotError::InvalidPath(path.clone()))?;
            out.push(SourceFile {
                rel: path_key(relative)?,
                abs: path,
            });
        } else {
            return Err(SnapshotError::UnsupportedFileType(path));
        }
    }
    Ok(())
}

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

fn compare_source(
    entries: &[FileEntry],
    scanned: &[SourceFile],
) -> Result<SourceDrift, SnapshotError> {
    let expected: BTreeMap<String, &FileEntry> = entries
        .iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();
    let mut actual: BTreeMap<String, (u64, String)> = BTreeMap::new();
    for file in scanned {
        let (size, sha256) = hash_file(&file.abs)?;
        actual.insert(file.rel.clone(), (size, sha256));
    }

    let mut changed = Vec::new();
    let mut removed = Vec::new();
    for (path, entry) in &expected {
        match actual.get(path) {
            Some((size, sha256)) if *size == entry.size && *sha256 == entry.sha256 => {}
            Some(_) => changed.push(path.clone()),
            None => removed.push(path.clone()),
        }
    }
    let added = actual
        .keys()
        .filter(|path| !expected.contains_key(*path))
        .cloned()
        .collect();

    Ok(SourceDrift {
        changed,
        added,
        removed,
    })
}

fn hash_file(path: &Path) -> Result<(u64, String), SnapshotError> {
    let mut file = File::open(path).map_err(|error| io_error("打开文件", path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; HASH_BUFFER_BYTES];
    let mut size = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| io_error("读取文件", path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((size, hex(&hasher.finalize())))
}

fn compute_digest(entries: &[FileEntry]) -> String {
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.path.as_bytes());
        hasher.update([0u8]);
        hasher.update(entry.size.to_be_bytes());
        hasher.update(entry.sha256.as_bytes());
        hasher.update(b"\n");
    }
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

fn validate_id(id: &str) -> Result<(), SnapshotError> {
    let valid = !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id != "."
        && id != ".."
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.');
    if valid {
        Ok(())
    } else {
        Err(SnapshotError::InvalidId(id.to_string()))
    }
}

fn validate_manifest(manifest: &Manifest) -> Result<(), SnapshotError> {
    if manifest
        .entries
        .windows(2)
        .any(|pair| pair[0].path >= pair[1].path)
    {
        return Err(SnapshotError::Manifest(
            "条目未按路径严格排序或存在重复".to_string(),
        ));
    }
    if compute_digest(&manifest.entries) != manifest.digest {
        return Err(SnapshotError::Manifest("清单摘要与条目不一致".to_string()));
    }
    Ok(())
}

/// 冻结：文件 0444、目录 0555；目录按深度从深到浅设置，最后是快照根。
fn freeze_tree(root: &Path) -> Result<(), SnapshotError> {
    let mut dirs = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        dirs.push(dir.clone());
        for entry in fs::read_dir(&dir).map_err(|error| io_error("读取目录", &dir, error))? {
            let entry = entry.map_err(|error| io_error("读取目录项", &dir, error))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| io_error("读取文件类型", &path, error))?;
            if file_type.is_symlink() {
                return Err(SnapshotError::Symlink(path));
            }
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                set_read_only(&path)?;
            } else {
                return Err(SnapshotError::UnsupportedFileType(path));
            }
        }
    }
    dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for dir in dirs {
        set_dir_read_only(&dir)?;
    }
    Ok(())
}

/// 检查快照里是否还有可写项；目录或文件被 chmod 后这里会报告。
fn find_writable(root: &Path) -> Result<Vec<String>, SnapshotError> {
    let mut writable = Vec::new();
    let metadata = fs::metadata(root).map_err(|error| io_error("读取目录", root, error))?;
    if is_writable(&metadata) {
        writable.push(".".to_string());
    }

    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).map_err(|error| io_error("读取目录", &dir, error))? {
            let entry = entry.map_err(|error| io_error("读取目录项", &dir, error))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| io_error("读取元数据", &path, error))?;
            let relative = path
                .strip_prefix(root)
                .map_err(|_| SnapshotError::InvalidPath(path.clone()))?;
            let relative = path_key(relative)?;
            if metadata.file_type().is_symlink() {
                writable.push(relative);
                continue;
            }
            if is_writable(&metadata) {
                writable.push(relative);
            }
            if metadata.is_dir() {
                dirs.push(path);
            }
        }
    }
    writable.sort();
    Ok(writable)
}

#[cfg(unix)]
fn is_writable(metadata: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o222 != 0
}

#[cfg(not(unix))]
fn is_writable(metadata: &Metadata) -> bool {
    // Windows 目录的只读标志不阻止写入，只有文件级别的只读可靠。
    metadata.is_file() && !metadata.permissions().readonly()
}

#[cfg(unix)]
fn set_read_only(path: &Path) -> Result<(), SnapshotError> {
    use std::os::unix::fs::PermissionsExt;
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("读取元数据", path, error))?;
    // 去掉所有写位，保留读/执行位与属主/组/其他的访问类别。
    let mode = (metadata.permissions().mode() & !0o222) | 0o400;
    set_mode(path, mode)
}

#[cfg(unix)]
fn set_dir_read_only(path: &Path) -> Result<(), SnapshotError> {
    use std::os::unix::fs::PermissionsExt;
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("读取元数据", path, error))?;
    // 目录需要读与进入权限；同样只移除写位，避免把私有目录放宽为 0555。
    let mode = (metadata.permissions().mode() & !0o222) | 0o500;
    set_mode(path, mode)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), SnapshotError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| io_error("设置只读权限", path, error))
}

#[cfg(not(unix))]
fn set_read_only(path: &Path) -> Result<(), SnapshotError> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| io_error("读取元数据", path, error))?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).map_err(|error| io_error("设置只读权限", path, error))
}

#[cfg(not(unix))]
fn set_dir_read_only(path: &Path) -> Result<(), SnapshotError> {
    set_read_only(path)
}

/// 递归恢复写权限；只用于删除自己创建的临时目录或快照。
fn make_tree_writable(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if metadata.is_dir() { 0o700 } else { 0o600 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
        let mut permissions = metadata.permissions();
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)?;
    }

    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            make_tree_writable(&entry?.path())?;
        }
    }
    Ok(())
}

fn remove_tree(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    make_tree_writable(path)?;
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn remove_file_force(path: &Path) -> io::Result<()> {
    match make_tree_writable(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

struct Cleanup {
    path: PathBuf,
    armed: bool,
}

impl Cleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

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

fn unique_suffix() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!(
        "{}-{}-{}",
        std::process::id(),
        nanos,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp_dir() -> TempDir {
        TempDir::create("chapter-09-test").expect("临时目录应能创建")
    }

    fn workspace(temp: &TempDir) -> PathBuf {
        let path = temp.path().join("workspace");
        fs::create_dir_all(path.join("src")).expect("工作区应能创建");
        fs::write(path.join("src/main.rs"), "fn main() {}\n").expect("文件应能写入");
        fs::write(path.join("README.md"), "候选工作区\n").expect("文件应能写入");
        path
    }

    fn snapshot_root(temp: &TempDir) -> PathBuf {
        let path = temp.path().join("snapshots");
        fs::create_dir_all(&path).expect("快照根应能创建");
        path
    }

    #[test]
    fn keeps_modified_and_untracked_files_and_excludes_metadata() {
        let temp = temp_dir();
        let source = workspace(&temp);
        // 在“编码结束”后修改并新增文件：它们都在磁盘上，快照必须包含。
        fs::write(source.join("src/main.rs"), "fn main() { changed }\n").unwrap();
        fs::write(source.join("notes.txt"), "untracked\n").unwrap();
        fs::create_dir_all(source.join(".git")).unwrap();
        fs::write(source.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::create_dir_all(source.join("target/debug")).unwrap();
        fs::write(source.join("target/debug/build.log"), "artifact\n").unwrap();

        let root = snapshot_root(&temp);
        let snapshot = Snapshot::create(&source, &root, "candidate-1").unwrap();
        let paths: Vec<&str> = snapshot
            .manifest()
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();
        assert_eq!(paths, ["README.md", "notes.txt", "src/main.rs"]);
        assert_eq!(
            fs::read_to_string(snapshot.path().join("notes.txt")).unwrap(),
            "untracked\n"
        );
        assert_eq!(
            fs::read_to_string(snapshot.path().join("src/main.rs")).unwrap(),
            "fn main() { changed }\n"
        );
        assert!(!snapshot.path().join(".git").exists());
        assert!(!snapshot.path().join("target").exists());
        assert!(snapshot.verify().is_ok());
        snapshot.remove().unwrap();
    }

    #[test]
    fn rejects_symlinks_instead_of_copying_outside_content() {
        let temp = temp_dir();
        let source = workspace(&temp);
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, "outside\n").unwrap();
        std::os::unix::fs::symlink(&outside, source.join("link.txt")).unwrap();

        let root = snapshot_root(&temp);
        let error = match Snapshot::create(&source, &root, "candidate-1") {
            Ok(snapshot) => {
                snapshot.remove().unwrap();
                panic!("符号链接应被拒绝");
            }
            Err(error) => error,
        };
        assert!(matches!(error, SnapshotError::Symlink(_)));
        assert!(!root.join("candidate-1").exists());
        assert!(!root.join("candidate-1.manifest.json").exists());
    }

    #[test]
    fn frozen_snapshot_rejects_writes_and_removals() {
        let temp = temp_dir();
        let source = workspace(&temp);
        let root = snapshot_root(&temp);
        let snapshot = Snapshot::create(&source, &root, "candidate-1").unwrap();

        assert!(fs::write(snapshot.path().join("src/main.rs"), "changed").is_err());
        assert!(fs::write(snapshot.path().join("src/injected.rs"), "new").is_err());
        assert!(fs::remove_file(snapshot.path().join("src/main.rs")).is_err());
        assert!(snapshot.verify().is_ok());

        snapshot.remove().unwrap();
        assert!(!root.join("candidate-1").exists());
    }

    #[test]
    fn verify_detects_content_changes_additions_and_removals() {
        let temp = temp_dir();
        let source = workspace(&temp);
        let root = snapshot_root(&temp);
        let snapshot = Snapshot::create(&source, &root, "candidate-1").unwrap();

        // 模拟一个有能力 chmod 的进程：冻结不是安全沙箱。
        make_tree_writable(snapshot.path()).unwrap();
        fs::write(snapshot.path().join("src/main.rs"), "fn niam() {}\n").unwrap();
        fs::write(snapshot.path().join("added.txt"), "added\n").unwrap();
        fs::remove_file(snapshot.path().join("README.md")).unwrap();

        match snapshot.verify() {
            Err(SnapshotError::Inconsistent {
                changed,
                added,
                removed,
                writable,
            }) => {
                assert!(changed.contains(&"src/main.rs".to_string()));
                assert!(added.contains(&"added.txt".to_string()));
                assert!(removed.contains(&"README.md".to_string()));
                assert!(!writable.is_empty());
            }
            other => panic!("应检测到不一致，实际：{other:?}"),
        }

        snapshot.remove().unwrap();
    }

    #[test]
    fn verify_detects_permission_tampering_without_content_changes() {
        let temp = temp_dir();
        let source = workspace(&temp);
        let root = snapshot_root(&temp);
        let snapshot = Snapshot::create(&source, &root, "candidate-1").unwrap();

        let file = snapshot.path().join("README.md");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();

        match snapshot.verify() {
            Err(SnapshotError::Inconsistent { writable, .. }) => {
                assert!(writable.contains(&"README.md".to_string()));
            }
            other => panic!("应检测到权限松动，实际：{other:?}"),
        }

        snapshot.remove().unwrap();
    }

    #[test]
    fn source_change_during_copy_is_rejected() {
        let temp = temp_dir();
        let source = workspace(&temp);
        let root = snapshot_root(&temp);

        let result = Snapshot::create_internal(
            &source,
            &root,
            "candidate-1",
            &SnapshotOptions::default(),
            Some(&mut |path: &Path| {
                fs::write(path.join("src/main.rs"), "written during copy\n").unwrap();
            }),
        );

        match result {
            Err(SnapshotError::SourceChanged { changed, .. }) => {
                assert!(changed.contains(&"src/main.rs".to_string()));
            }
            Ok(snapshot) => {
                snapshot.remove().unwrap();
                panic!("复制期间源文件变化应被拒绝");
            }
            Err(other) => panic!("应返回 SourceChanged，实际：{other:?}"),
        }
        assert!(!root.join("candidate-1").exists());
        assert!(!root.join("candidate-1.manifest.json").exists());
    }

    #[test]
    fn same_content_produces_same_digest_different_id() {
        let temp = temp_dir();
        let source = workspace(&temp);
        let root = snapshot_root(&temp);

        let first = Snapshot::create(&source, &root, "candidate-1").unwrap();
        let first_digest = first.digest().to_string();
        first.remove().unwrap();

        let second = Snapshot::create(&source, &root, "candidate-2").unwrap();
        assert_eq!(first_digest, second.digest());
        second.remove().unwrap();
    }

    #[test]
    fn existing_id_is_not_overwritten() {
        let temp = temp_dir();
        let source = workspace(&temp);
        let root = snapshot_root(&temp);
        let snapshot = Snapshot::create(&source, &root, "candidate-1").unwrap();

        match Snapshot::create(&source, &root, "candidate-1") {
            Err(SnapshotError::AlreadyExists(_)) => {}
            other => panic!("重复 id 应被拒绝，实际：{other:?}"),
        }

        snapshot.remove().unwrap();
    }

    #[test]
    fn invalid_ids_are_rejected_before_touching_the_filesystem() {
        let temp = temp_dir();
        let source = workspace(&temp);
        let root = snapshot_root(&temp);
        for id in ["", ".", "..", "../escape", "a/b", "a\\b"] {
            match Snapshot::create(&source, &root, id) {
                Err(SnapshotError::InvalidId(_)) => {}
                other => panic!("id {id:?} 应被拒绝，实际：{other:?}"),
            }
        }
    }

    #[test]
    fn open_reloads_manifest_and_rejects_tampering() {
        let temp = temp_dir();
        let source = workspace(&temp);
        let root = snapshot_root(&temp);
        let snapshot = Snapshot::create(&source, &root, "candidate-1").unwrap();
        snapshot.remove().unwrap();

        let snapshot = Snapshot::create(&source, &root, "candidate-1b").unwrap();
        let reopened = Snapshot::open(&root, "candidate-1b").unwrap();
        assert_eq!(reopened.digest(), snapshot.digest());
        assert!(reopened.verify().is_ok());

        // 篡改清单：恢复写权限后改掉摘要。
        make_tree_writable(&snapshot.manifest_path()).unwrap();
        let mut manifest = snapshot.manifest().clone();
        manifest.digest = "0".repeat(64);
        fs::write(
            snapshot.manifest_path(),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        match Snapshot::open(&root, "candidate-1b") {
            Err(SnapshotError::Manifest(_)) => {}
            other => panic!("篡改清单应被拒绝，实际：{other:?}"),
        }

        snapshot.remove().unwrap();
    }

    #[test]
    fn temp_dir_drop_cleans_a_frozen_snapshot() {
        let temp = temp_dir();
        let path = temp.path().to_path_buf();
        let source = workspace(&temp);
        let root = snapshot_root(&temp);
        let snapshot = Snapshot::create(&source, &root, "candidate-1").unwrap();
        drop(snapshot);

        drop(temp);
        assert!(!path.exists());
    }
}
