//! 策略原始碼的最後更新時間（M14.5：開局設定的選單依它排序）。
//!
//! 建置時（build.rs 也引入本檔）算一次嵌入執行檔；執行時原始碼目錄還在（開發環境）就重新計算，
//! 改了策略不必重新建置也能看到新的時間（M16 的監看與重建會用到）。

use std::path::Path;
use std::process::Command;
use std::time::UNIX_EPOCH;

/// 策略的原始碼位置與建置時算出的更新時間。
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Source {
    /// 策略目錄（建置時的絕對路徑）
    pub dir: &'static str,
    /// 建置時的最後更新時間（Unix 秒）
    pub built: Option<u64>,
}

#[allow(dead_code)]
impl Source {
    /// 最後更新時間：原始碼目錄還在時重新計算，否則用建置時的值。
    pub fn updated(&self) -> Option<u64> {
        let dir = Path::new(self.dir);
        if dir.is_dir() {
            dir_updated(dir).or(self.built)
        } else {
            self.built
        }
    }
}

fn mtime(p: &Path) -> Option<u64> {
    let t = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(t.duration_since(UNIX_EPOCH).ok()?.as_secs())
}

/// 目錄中所有檔案最晚的修改時間。
fn max_mtime(dir: &Path) -> Option<u64> {
    let mut best = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        let t = if p.is_dir() { max_mtime(&p) } else { mtime(&p) };
        best = best.max(t);
    }
    best
}

/// 目錄的最後更新時間（Unix 秒）：git 最後一次提交這個目錄的時間，與還沒提交的修改檔案的修改時間，取較晚者。
/// 檔案的修改時間在 git clone 後都是 clone 的時間，所以已提交的部分用提交時間。
/// 不在 git 中（或沒有 git）時，用目錄中所有檔案最晚的修改時間。
pub fn dir_updated(dir: &Path) -> Option<u64> {
    let git = |args: &[&str]| -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let Some(root) = git(&["rev-parse", "--show-toplevel"]) else {
        return max_mtime(dir);
    };
    let root = Path::new(root.trim());
    let committed =
        git(&["log", "-1", "--format=%ct", "--", "."]).and_then(|s| s.trim().parse::<u64>().ok());
    // -z：每筆「XY 路徑」以 NUL 分隔（路徑相對於 repo 根目錄，不加引號）；改名時下一筆是原路徑。
    let status = git(&[
        "status",
        "--porcelain",
        "-z",
        "--untracked-files=all",
        "--",
        ".",
    ])?;
    let mut dirty = None;
    let mut entries = status.split('\0');
    while let Some(e) = entries.next() {
        if e.len() < 4 {
            continue;
        }
        if e.starts_with('R') || e.starts_with('C') {
            entries.next();
        }
        dirty = dirty.max(mtime(&root.join(&e[3..])));
    }
    committed.max(dirty).or_else(|| max_mtime(dir))
}
