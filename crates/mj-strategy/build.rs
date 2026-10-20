//! 掃描 src/strategy-list/，自動產生模組宣告與策略清單，讓策略編輯者新增檔案時不必寫 `mod`。
//!
//! 目錄結構是 `strategy-list/<規則>/<策略>/`：第一層是規則（例如 `tenhou-normal-4p-hanchan`），
//! 第二層是同一套規則下的各個策略。策略的完整名稱是「規則／策略」。
//!
//! 目錄或檔名中的 `-` 會轉成 `_`（Rust 模組名稱不能有 `-`），例如
//! `strategy-list/tenhou-normal-4p-hanchan/push-fold/decision/discard.rs` →
//! `crate::strategy_list::tenhou_normal_4p_hanchan::push_fold::decision::discard`。
//!
//! 每個規則目錄必須有 `README.md`（規則說明）；每個策略目錄必須有
//! - `README.md`：策略說明（第一行 `# 標題`，接著一段摘要，之後是詳細特色），GUI 選策略時顯示；
//! - `main.rs`：`pub fn register(book: &mut Book<Memory>)`；
//! - `memory.rs`：`pub struct Memory`，需實作 `Default` 與 `Clone`。
//!
//! 策略中所有 `pub const 名稱: Param = Param::new(...)` 會自動列為這個策略的可調參數。

#[path = "src/framework/source.rs"]
mod source;

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
    let mut rules = Vec::new();
    for rule_dir in sorted_entries(&root).into_iter().filter(|p| p.is_dir()) {
        let rule = rule_dir.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            rule_dir.join("README.md").exists(),
            "規則目錄 {rule} 缺少 README.md（規則說明）"
        );
        println!("cargo:rerun-if-changed={}", rule_dir.display());
        rules.push((rule.clone(), rule_dir.join("README.md")));
        writeln!(out, "    pub mod {} {{", ident(&rule)).unwrap();
        for dir in sorted_entries(&rule_dir).into_iter().filter(|p| p.is_dir()) {
            let name = dir.file_name().unwrap().to_string_lossy().to_string();
            for required in ["README.md", "main.rs", "memory.rs"] {
                assert!(
                    dir.join(required).exists(),
                    "策略 {rule}/{name} 缺少 {required}"
                );
            }
            let readme = fs::read_to_string(dir.join("README.md")).unwrap();
            assert!(
                readme.starts_with("# "),
                "策略 {rule}/{name} 的 README.md 第一行必須是「# 標題」"
            );
            // 每個檔案與目錄都要讓 cargo 監看，新增檔案時才會重新產生。
            watch(&dir);
            writeln!(out, "        pub mod {} {{", ident(&name)).unwrap();
            emit_dir(&mut out, &dir, 3);
            writeln!(out, "        }}").unwrap();
            let mut params = Vec::new();
            find_params(&dir, &mut Vec::new(), &mut params);
            let updated = source::dir_updated(&dir);
            strategies.push((rule.clone(), name, dir.clone(), params, updated));
        }
        writeln!(out, "    }}").unwrap();
    }
    out.push_str("}\n\n/// 自動產生：所有策略。\npub fn all_strategies() -> Vec<Box<dyn crate::framework::Strategy>> {\n    vec![\n");
    for (rule, name, dir, params, updated) in &strategies {
        let m = format!("strategy_list::{}::{}", ident(rule), ident(name));
        let full = format!("{rule}/{name}");
        let readme = dir.join("README.md").display().to_string();
        let params: Vec<String> = params.iter().map(|p| format!("{m}::{p}")).collect();
        let source = format!(
            "crate::framework::source::Source {{ dir: {:?}, built: {updated:?} }}",
            dir.display().to_string()
        );
        writeln!(
            out,
            "        crate::framework::book_strategy::<{m}::memory::Memory>({full:?}, include_str!({readme:?}), {m}::main::register, vec![{}], {source}),",
            params.join(", ")
        )
        .unwrap();
    }
    out.push_str("    ]\n}\n\n/// 自動產生：所有規則（名稱, README.md 全文）。\npub fn all_rules() -> Vec<(&'static str, &'static str)> {\n    vec![\n");
    for (rule, readme) in &rules {
        let readme = readme.display().to_string();
        writeln!(out, "        ({rule:?}, include_str!({readme:?})),").unwrap();
    }
    out.push_str("    ]\n}\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("strategy_list.rs");
    fs::write(dest, out).unwrap();
}

/// 找出目錄中所有 `pub const 名稱: Param = Param::new(` 宣告，記下模組路徑（例如 `utils::predict::tenpai::BASE`）。
fn find_params(dir: &Path, module: &mut Vec<String>, out: &mut Vec<String>) {
    for p in sorted_entries(dir) {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            module.push(ident(&name));
            find_params(&p, module, out);
            module.pop();
        } else if let Some(stem) = name.strip_suffix(".rs") {
            let text = fs::read_to_string(&p).unwrap();
            for line in text.lines() {
                let Some(rest) = line.trim_start().strip_prefix("pub const ") else {
                    continue;
                };
                let Some((ident_part, ty)) = rest.split_once(':') else {
                    continue;
                };
                if ty.trim_start().starts_with("Param =") || ty.trim() == "Param" {
                    let mut path = module.clone();
                    path.push(ident(stem));
                    path.push(ident_part.trim().to_string());
                    out.push(path.join("::"));
                }
            }
        }
    }
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
