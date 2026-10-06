"""向聽數的差異測試：與 Python `mahjong` 函式庫的計算結果比對。

    cargo build --release -p mj-calc --example shanten_stdin
    tools/oracle/.venv/bin/python tools/oracle/check_shanten.py 100000
"""

import random
import subprocess
import sys

from mahjong.shanten import Shanten

SUITS = "mpsz"


def to_str(counts):
    """34 格牌數表 → 緊湊記法（例如 "123m55z"），交給 Rust 程式解析。"""
    out = ""
    for s in range(4):
        digits = "".join(str(n + 1) * counts[s * 9 + n] for n in range(9 if s < 3 else 7))
        if digits:
            out += digits + SUITS[s]
    return out


def random_hand(rng, size):
    """產生一手隨機牌（13 或 14 張）。"""
    wall = [k for k in range(34) for _ in range(4)]
    # 一半的機率只用一兩種花色，讓聽牌、一向聽等低向聽手牌更常出現。
    if rng.random() < 0.5:
        keep = set(rng.sample(range(4), rng.choice([1, 2])))
        wall = [k for k in wall if k // 9 in keep or (k >= 27 and 3 in keep)]
    counts = [0] * 34
    for k in rng.sample(wall, size):
        counts[k] += 1
    return counts


def main():
    """產生 n 手隨機牌，同時交給 Rust（shanten_stdin）與函式庫計算，比對結果。"""
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 100000
    rng = random.Random(1)
    hands = [random_hand(rng, rng.choice([13, 14])) for _ in range(n)]
    proc = subprocess.run(
        ["target/release/examples/shanten_stdin"],
        input="\n".join(to_str(h) for h in hands) + "\n",
        capture_output=True,
        text=True,
        check=True,
    )
    ours = [int(x) for x in proc.stdout.split()]
    bad = [(h, o) for h, o in zip(hands, ours) if Shanten.calculate_shanten(h) != o]
    hist = {}
    for o in ours:
        hist[o] = hist.get(o, 0) + 1
    print(f"checked {n} hands, {len(bad)} mismatches; shanten histogram {dict(sorted(hist.items()))}")
    for h, o in bad[:10]:
        print("  ", to_str(h), "ours", o, "theirs", Shanten.calculate_shanten(h))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
