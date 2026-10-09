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
