"""點數計算的差異測試：用獨立實作的 Python `mahjong` 函式庫，重新計算 `gen_wins` 產生的和牌。

    cargo run --release -p mj-rules --example gen_wins -- 20000 1 > /tmp/wins.jsonl
    tools/oracle/.venv/bin/python tools/oracle/check_scoring.py /tmp/wins.jsonl

比對項目：是否能和牌（有沒有役）、番數、符數、總支付點數。
不一致的手牌依本專案判定的役種組合分組列出。
"""

import json
import sys
from collections import Counter, defaultdict

from mahjong.hand_calculating.hand import HandCalculator
from mahjong.hand_calculating.hand_config import HandConfig, OptionalRules
from mahjong.meld import Meld

# 花色字母 → 牌種編號起點（m 萬、p 筒、s 索、z 字）
KINDS = {"m": 0, "p": 9, "s": 18, "z": 27}
# 136 格式中赤五萬／赤五筒／赤五索的編號（每種牌的第一張）
RED_COPY = {4: 16, 13: 52, 22: 88}
# 對應天鳳規則：有食斷、有赤寶牌、沒有雙倍役滿（國士十三面等不加倍）、沒有切上滿貫、
# 副露平和型 30 符、平和自摸 20 符。
OPTIONS = OptionalRules(
    has_open_tanyao=True,
    has_aka_dora=True,
    has_double_yakuman=False,
    kiriage=False,
    fu_for_open_pinfu=True,
    fu_for_pinfu_tsumo=False,
)


def parse(s):
    """解析緊湊記法（如 "123m0p"）為牌種編號 0–33；0 視為 5。"""
    out, pending = [], []
    for c in s:
        if c.isdigit():
            pending.append(int(c))
        else:
            out += [KINDS[c] + (5 if n == 0 else n) - 1 for n in pending]
            pending = []
    return out


class Allocator:
    """配發不重複的 136 格式牌編號（每種牌 4 張：編號 種類×4 ~ 種類×4+3）。
    除非指定要赤五，否則不會配到赤五那張。"""

    def __init__(self):
        self.used = set()

    def take(self, kind, red=False):
        if red:
            tid = RED_COPY[kind]
            assert tid not in self.used
            self.used.add(tid)
            return tid
        for tid in range(kind * 4, kind * 4 + 4):
            if tid not in self.used and tid != RED_COPY.get(kind):
                self.used.add(tid)
                return tid
        raise ValueError(f"no copy left of kind {kind}")


def check(rec):
    """重新計算一筆和牌，回傳 (是否一致, 本專案結果, 函式庫結果, 函式庫錯誤訊息)。"""
    alloc = Allocator()
    reds = {parse(r)[0] for r in rec["reds"]}
    melds, all_tiles = [], []
    # 副露轉成函式庫的 Meld 格式（Kakan = 加槓 SHOUMINKAN；暗槓 opened=False）。
    for m in rec["melds"]:
        ids = [alloc.take(k) for k in parse("".join(m["tiles"]))]
        all_tiles += ids
        kind = m["kind"]
        if kind == "Chi":
            melds.append(Meld(Meld.CHI, ids, opened=True, called_tile=ids[0]))
        elif kind == "Pon":
            melds.append(Meld(Meld.PON, ids, opened=True, called_tile=ids[0]))
        elif kind == "Kakan":
            melds.append(Meld(Meld.SHOUMINKAN, ids, opened=True, called_tile=ids[0]))
        else:
            opened = kind == "Daiminkan"
            melds.append(Meld(Meld.KAN, ids, opened=opened, called_tile=ids[0] if opened else None))
    win_kind = parse(rec["win"])[0]
    win_id = None
    for k in parse(rec["concealed"]):
        red = k in reds
        if red:
            reds.discard(k)
        tid = alloc.take(k, red)
        all_tiles.append(tid)
        # 和牌張盡量選非赤五的那張，避免赤五的位置影響聽法判定。
        if k == win_kind and (win_id is None or win_id in RED_COPY.values()):
            win_id = tid

    # 和牌狀況。莊家由 player_wind == 東 推得，函式庫沒有另外的 is_dealer 參數。
    riichi = rec["riichi"]
    config = HandConfig(
        is_tsumo=rec["tsumo"],
        is_riichi=riichi != "None",
        is_daburu_riichi=riichi == "Double",
        is_ippatsu=rec["ippatsu"],
        is_haitei=rec["haitei"],
        is_houtei=rec["houtei"],
        is_rinshan=rec["rinshan"],
        is_chankan=rec["chankan"],
        is_tenhou=rec["first_draw"] and rec["dealer"],
        is_chiihou=rec["first_draw"] and not rec["dealer"],
        player_wind=27 + rec["seat_wind"],
        round_wind=27 + rec["round_wind"],
        options=OPTIONS,
    )
    dora = [alloc_free(k) for k in parse("".join(rec["dora"]))]
    ura = [alloc_free(k) for k in parse("".join(rec["ura"]))]
    r = HandCalculator.estimate_hand_value(
        all_tiles, win_id, melds=melds, dora_indicators=dora, ura_dora_indicators=ura or None, config=config
    )
    # 役滿只比較總點數（兩邊表示番數的方式不同）。
    theirs = None
    if not r.error:
        yakuman = r.han >= 13 or any(y.is_yakuman for y in r.yaku)
        theirs = {
            "han": None if yakuman else r.han,
            "fu": None if yakuman else r.fu,
            "total": r.cost["total"],
            "yaku": sorted(y.name for y in r.yaku),
        }
    ours = rec["ours"]
    if ours is not None:
        ours = {
            "han": None if ours["yakuman"] else ours["han"],
            "fu": None if ours["yakuman"] else ours["fu"],
            "total": ours["total"],
            "yaku": ours["yaku"],
        }
    same = (ours is None) == (theirs is None) and (
        ours is None or all(ours[k] == theirs[k] for k in ("han", "fu", "total"))
    )
    return same, ours, theirs, r.error


def alloc_free(kind):
    """寶牌指示牌只看牌種，用該種牌的任一個編號即可。"""
    return kind * 4 + 3


def main():
    """逐行讀取 gen_wins 的輸出並比對，印出覆蓋到的役種與不一致的例子。"""
    path = sys.argv[1]
    bad = defaultdict(list)
    n = 0
    valid = 0
    yaku_freq = Counter()
    for line in open(path):
        rec = json.loads(line)
        n += 1
        same, ours, theirs, err = check(rec)
        if ours is not None:
            valid += 1
            yaku_freq.update(y.split(":")[0] for y in ours["yaku"])
        if not same:
            key = tuple(sorted(y.split(":")[0] for y in (ours or {}).get("yaku", []))) or ("<none>",)
            bad[key].append((rec, ours, theirs, err))
    total_bad = sum(len(v) for v in bad.values())
    print(f"checked {n} hands ({valid} with yaku), {total_bad} mismatches")
    print("yaku coverage:", ", ".join(f"{k}={v}" for k, v in sorted(yaku_freq.items(), key=lambda kv: -kv[1])))
    for key, items in sorted(bad.items(), key=lambda kv: -len(kv[1]))[:15]:
        rec, ours, theirs, err = items[0]
        print(f"\n[{len(items)}] ours yaku={key}")
        print("  hand:", rec["concealed"], rec["melds"], "win", rec["win"], "tsumo" if rec["tsumo"] else "ron",
              "riichi", rec["riichi"], "seat", rec["seat_wind"], "round", rec["round_wind"],
              "dealer" if rec["dealer"] else "", "dora", rec["dora"], "ura", rec["ura"], "reds", rec["reds"])
        print("  ours:  ", ours)
        print("  theirs:", theirs, err or "")
    sys.exit(1 if total_bad else 0)


if __name__ == "__main__":
    main()
