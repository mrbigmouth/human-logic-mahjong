# SPDX-License-Identifier: AGPL-3.0-or-later
"""Mortal 的 mjai 機器人（mj 的 `mjai:` 參賽者用）。

本檔會載入 Mortal（AGPL-3.0-or-later）的程式碼，所以本檔也以 AGPL-3.0-or-later 授權；
專案的其他部分只透過 mjai 協定（標準輸入／輸出）和它溝通，不受影響。

通訊格式與 mjai.app 相同：每行讀入一個 JSON 陣列（上次回應之後的所有事件），
依序交給 libriichi 的 Bot，回應最後一個事件的反應（沒有反應時為 {"type":"none"}）。

擴充：[{"type":"mj_peek","events":[...]}] 詢問「假設接著發生這些事件，會怎麼做」，回答後狀態不變
（mj 的旁觀模式用它取得立直時要打的牌）。做法是用另一個 Bot 重播本場到目前為止的事件
（重播時用不跑模型的替身引擎），再讓真的模型回答最後的假設事件。

用法：
  mortal_bot.py s42                 # 內建名稱：s42、hhim8826（權重放在 data/mortal/ 下，見 README.md）
  mortal_bot.py --weights 檔案.pth   # 其他權重
  --threads N                       # PyTorch 執行緒數（預設 1；同時開很多行程時用 1 最有效率）
  --q-values                        # 回應附上 meta.q_values（關閉 quick eval，較慢）
"""
import argparse
import hashlib
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DATA = ROOT / 'data' / 'mortal'
MODELS = {
    # 名稱: (權重檔, SHA-256)
    's42': (DATA / 's42' / 'weights' / 's42-inference.pth',
            'e5f6955b6f19007fd65315e53291b36994493046bc118b991b959e9dc596d660'),
    'hhim8826': (DATA / 'hhim8826' / 'mortal.pth',
                 'ec679c428766af25fe2c0520938fffc7047f6c8016e3930f3b6c61fdabe48559'),
}


def load_engine(path: Path, sha256: str | None, quick_eval: bool):
    import torch
    from engine import MortalEngine
    from model import Brain, DQN

    if sha256 is not None:
        with path.open('rb') as f:
            if hashlib.file_digest(f, 'sha256').hexdigest() != sha256:
                raise SystemExit(f'權重檔的 SHA-256 不符：{path}')
    state = torch.load(path, map_location='cpu', weights_only=True)
    if 'config' in state:
        # 一般的 Mortal 檢查點：版本與網路大小記在 config 中。
        cfg = state['config']
        version = cfg['control'].get('version', 1)
        channels, blocks = cfg['resnet']['conv_channels'], cfg['resnet']['num_blocks']
    else:
        # S42：只有推理張量（v4、192 通道 × 40 層）。
        version, channels, blocks = 4, 192, 40
    brain = Brain(version=version, conv_channels=channels, num_blocks=blocks).eval()
    dqn = DQN(version=version).eval()
    brain.load_state_dict(state['mortal'], strict=True)
    dqn.load_state_dict(state['current_dqn'], strict=True)
    return MortalEngine(
        brain, dqn, is_oracle=False, version=version, device=torch.device('cpu'),
        enable_amp=False, enable_quick_eval=quick_eval,
        enable_rule_based_agari_guard=True, name=path.stem,
    )


class Engine:
    """包裝真的引擎；`live` 為 False 時不跑模型，回傳第一個合法動作（只用於重播局面）。"""

    def __init__(self, real):
        self.real = real
        self.live = True

    def __getattr__(self, name):
        return getattr(self.real, name)

    def react_batch(self, obs, masks, invisible_obs):
        if self.live:
            return self.real.react_batch(obs, masks, invisible_obs)
        import numpy as np
        actions = [int(np.flatnonzero(m)[0]) for m in masks]
        q = [[0.0 if b else float('-inf') for b in m] for m in masks]
        return actions, q, masks, [True] * len(masks)


def peek(engine: Engine, player_id: int, history: list[str], events: list[dict]):
    from libriichi.mjai import Bot
    bot = Bot(engine, player_id)
    engine.live = False
    try:
        for e in history:
            bot.react(e)
    finally:
        engine.live = True
    reaction = None
    for e in events:
        reaction = bot.react(json.dumps(e))
    return reaction


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('model', nargs='?', choices=sorted(MODELS))
    ap.add_argument('--weights', type=Path)
    ap.add_argument('--threads', type=int, default=1)
    ap.add_argument('--q-values', action='store_true')
    ap.add_argument('--mortal-dir', type=Path, default=DATA / 'src' / 'mortal')
    args = ap.parse_args()
    if (args.model is None) == (args.weights is None):
        ap.error('請指定一個內建名稱或 --weights')
    path, sha = MODELS[args.model] if args.model else (args.weights, None)

    # Mortal 的 mortal/ 目錄（model.py、engine.py、libriichi.so）。
    sys.path.insert(0, str(args.mortal_dir))
    os.environ.setdefault('OMP_NUM_THREADS', str(args.threads))
    import torch
    torch.set_num_threads(args.threads)
    from libriichi.mjai import Bot

    engine = Engine(load_engine(path, sha, quick_eval=not args.q_values))
    bot, player_id, history = None, 0, []
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        events = json.loads(line)
        if isinstance(events, dict):
            events = [events]
        reaction = None
        if events and events[0]['type'] == 'mj_peek':
            if bot is not None:
                reaction = peek(engine, player_id, history, events[0]['events'])
            print(reaction or '{"type":"none"}', flush=True)
            continue
        for e in events:
            if e['type'] == 'start_game':
                player_id = e['id']
                bot, history = Bot(engine, player_id), []
            raw = json.dumps(e)
            history.append(raw)
            reaction = bot.react(raw) if bot is not None else None
        print(reaction or '{"type":"none"}', flush=True)


if __name__ == '__main__':
    main()
