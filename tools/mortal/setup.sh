#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# 安裝 Mortal 執行環境與權重（可重複執行；已完成的步驟會略過）。
#   tools/mortal/setup.sh            # 安裝
#   tools/mortal/setup.sh --rebuild  # 重建 venv 與 libriichi（例如 CPU 架構改變後）
# 需要：uv（pipx install uv）、Rust（cargo）、git、curl。
set -eu
DIR=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$DIR/../.." && pwd)
DATA="$ROOT/data/mortal"
UV=$(command -v uv || echo "$HOME/.local/bin/uv")
MORTAL_COMMIT=0cff2b52982be5b1163aa9a62fb01f03ce91e0d2   # S42 使用的上游版本

if [ "${1:-}" = "--rebuild" ]; then
    rm -rf "$DIR/.venv" "$DATA/src/mortal/libriichi.so" "$DATA/src/target"
fi

# 1. Python 3.12＋CPU 版 PyTorch
if [ ! -x "$DIR/.venv/bin/python" ]; then
    "$UV" python install 3.12
    "$UV" venv -p 3.12 "$DIR/.venv"
    "$UV" pip install -p "$DIR/.venv/bin/python" --index-url https://download.pytorch.org/whl/cpu torch numpy
fi

# 2. Mortal 原始碼（AGPL-3.0-or-later）與 libriichi
if [ ! -d "$DATA/src/.git" ]; then
    git clone https://github.com/Equim-chan/Mortal.git "$DATA/src"
fi
git -C "$DATA/src" checkout -q "$MORTAL_COMMIT"
if [ ! -f "$DATA/src/mortal/libriichi.so" ]; then
    (cd "$DATA/src" && PYO3_PYTHON="$DIR/.venv/bin/python" cargo build -p libriichi --lib --release)
    cp "$DATA/src/target/release/libriichi.so" "$DATA/src/mortal/libriichi.so"
fi

# 3. 權重（檢查 SHA-256）
fetch() { # 網址 檔案 SHA-256
    if [ ! -f "$2" ]; then
        mkdir -p "$(dirname "$2")"
        curl -fL -o "$2.part" "$1" && mv "$2.part" "$2"
    fi
    echo "$3  $2" | sha256sum -c -
}
fetch https://huggingface.co/banxiaweiliang/Mortal-S42/resolve/main/weights/s42-inference.pth \
    "$DATA/s42/weights/s42-inference.pth" e5f6955b6f19007fd65315e53291b36994493046bc118b991b959e9dc596d660
fetch https://huggingface.co/hhim8826/mortal3-b40c192/resolve/main/mortal.pth \
    "$DATA/hhim8826/mortal.pth" ec679c428766af25fe2c0520938fffc7047f6c8016e3930f3b6c61fdabe48559

# 4. 冒煙測試：S42 對三家貪婪機器人打一場
echo "安裝完成。測試：cargo run --release -p mj-cli -- selfplay --games 1 --seats \"mjai:$DIR/run.sh s42,greedy,greedy,greedy\""
