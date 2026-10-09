//! mjai 介接的端到端測試：實際啟動 `mj mjai-bot` 子行程，坐上 `mj selfplay` 的牌桌。

use std::process::Command;

#[test]
fn external_mjai_process_plays_full_games() {
    let mj = env!("CARGO_BIN_EXE_mj");
    let seats = format!(
        "mjai:{mj} mjai-bot --kind shanten,greedy,tenhou-normal-4p-hanchan/hand-reading,mjai:{mj} mjai-bot --kind tsumogiri"
    );
    let out = Command::new(mj)
        .args(["selfplay", "--games", "4", "--seats", &seats])
        .output()
        .expect("run mj");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(!stderr.contains("mjai："), "{stderr}");
    assert!(stdout.contains("mjai-bot --kind shanten"), "{stdout}");
}

/// `mj agree`：mjai AI 以旁觀模式看牌譜與自我對戰，回應全部能對應到合法動作，並輸出一致率。
#[test]
fn agree_compares_with_mjai_process() {
    let mj = env!("CARGO_BIN_EXE_mj");
    let log = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../mj-paifu/tests/data/2026100601gm-00a9-0000-cdca1e26.xml"
    );
    let ai = format!("{mj} mjai-bot --kind shanten");
    for args in [vec!["agree", log], vec!["agree", "--selfplay", "1"]] {
        let out = Command::new(mj)
            .args(&args)
            .args(["--mjai", &ai, "--jobs", "2"])
            .output()
            .expect("run mj");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stdout}\n{stderr}");
        assert!(!stdout.contains("無法對應"), "{stdout}");
        // 向聽機器人只打牌：何切有一致率，鳴牌時一律不鳴。
        let row = |name: &str| {
            stdout
                .lines()
                .find(|l| l.starts_with(name))
                .unwrap_or_else(|| panic!("沒有「{name}」：{stdout}"))
                .to_string()
        };
        let discard: Vec<String> = row("何切 ").split_whitespace().map(String::from).collect();
        let n: usize = discard[1].parse().unwrap();
        assert!(n > 100, "{stdout}");
        let rate: f64 = discard[2].trim_end_matches('%').parse().unwrap();
        assert!(rate > 30.0, "向聽機器人與策略的何切應該常常相同：{stdout}");
    }
}
