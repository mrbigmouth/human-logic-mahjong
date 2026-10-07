//! 掃描 src/strategy-list/，自動產生模組宣告與策略清單，讓策略編輯者新增檔案時不必寫 `mod`。
//!
//! 目錄或檔名中的 `-` 會轉成 `_`（Rust 模組名稱不能有 `-`），例如
//! `strategy-list/tenhou-normal-4p-hanchan/decision/discard.rs` →
//! `crate::strategy_list::tenhou_normal_4p_hanchan::decision::discard`。
//!
//! 每個策略目錄必須有 `main.rs`（`pub fn register(book: &mut Book<Memory>)`）
//! 與 `memory.rs`（`pub struct Memory`，需實作 `Default`）。

use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

fn ident(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{s}")
    } else {
        s
    }
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("讀取 {} 失敗：{e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    v.sort();
    v
}

/// 產生一個目錄的模組內容：.rs 檔 → `#[path] pub mod`，子目錄 → 巢狀 `pub mod`。
fn emit_dir(out: &mut String, dir: &Path, depth: usize) {
    let indent = "    ".repeat(depth);
    for p in sorted_entries(dir) {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            writeln!(out, "{indent}pub mod {} {{", ident(&name)).unwrap();
            emit_dir(out, &p, depth + 1);
            writeln!(out, "{indent}}}").unwrap();
        } else if name.ends_with(".rs") {
            let stem = name.trim_end_matches(".rs");
            writeln!(out, "{indent}#[path = {:?}]", p.display().to_string()).unwrap();
            writeln!(out, "{indent}pub mod {};", ident(stem)).unwrap();
        }
    }
}

fn main() {
    let root =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/strategy-list");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut out = String::from(
        "/// 自動產生：src/strategy-list/ 的模組樹。\n#[allow(clippy::module_inception)]\npub mod strategy_list {\n",
    );
    let mut strategies = Vec::new();
    for dir in sorted_entries(&root).into_iter().filter(|p| p.is_dir()) {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        for required in ["main.rs", "memory.rs"] {
            assert!(dir.join(required).exists(), "策略 {name} 缺少 {required}");
        }
        // 每個檔案與目錄都要讓 cargo 監看，新增檔案時才會重新產生。
        watch(&dir);
        writeln!(out, "    pub mod {} {{", ident(&name)).unwrap();
        emit_dir(&mut out, &dir, 2);
        writeln!(out, "    }}").unwrap();
        strategies.push(name);
    }
    out.push_str("}\n\n/// 自動產生：所有策略。\npub fn all_strategies() -> Vec<Box<dyn crate::framework::Strategy>> {\n    vec![\n");
    for name in &strategies {
        let m = ident(name);
        writeln!(
            out,
            "        crate::framework::book_strategy::<strategy_list::{m}::memory::Memory>({name:?}, strategy_list::{m}::main::register),"
        )
        .unwrap();
    }
    out.push_str("    ]\n}\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("strategy_list.rs");
    fs::write(dest, out).unwrap();
}

fn watch(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    for p in sorted_entries(dir) {
        if p.is_dir() {
            watch(&p);
        } else {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }
}
