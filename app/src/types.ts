// 與 Rust 端（crates/mj-app）的 JSON 對應的型別。牌一律是字串：5m、0p（赤五筒）、7z（中）。

export type Seat = 0 | 1 | 2 | 3;

export interface MeldView {
  kind: "chi" | "pon" | "daiminkan" | "ankan" | "kakan";
  tiles: string[];
  called: string | null;
  from: Seat | null;
}

export interface RiverTile {
  pai: string;
  tsumogiri: boolean;
  riichi: boolean;
  called: boolean;
}

export interface SeatView {
  hand: string[];
  hand_count: number;
  drawn: string | null;
  has_drawn: boolean;
  melds: MeldView[];
  river: RiverTile[];
  riichi: boolean;
  revealed: boolean;
  /** 檢視牌譜時公開所有人的手牌（hand、drawn 是實際的牌） */
  open?: boolean;
  wind: number;
}

export interface TableView {
  me: Seat;
  round: string;
  round_wind: number;
  kyoku: number;
  honba: number;
  riichi_sticks: number;
  scores: [number, number, number, number];
  dora_indicators: string[];
  wall_left: number;
  seats: [SeatView, SeatView, SeatView, SeatView];
  turn: Seat | null;
  last_discard: [Seat, number] | null;
}

export type EventView =
  | { type: "start_game"; text: string }
  | { type: "start_kyoku"; text: string }
  | { type: "draw"; seat: Seat; pai: string | null; rinshan: boolean; text: string }
  | { type: "discard"; seat: Seat; pai: string; tsumogiri: boolean; riichi: boolean; text: string }
  | { type: "call"; seat: Seat; meld: MeldView; text: string }
  | { type: "riichi_accepted"; seat: Seat; text: string }
  | { type: "dora"; indicator: string; text: string }
  | {
      type: "win";
      winner: Seat;
      from: Seat;
      hand: string[];
      win_tile: string;
      yaku: [string, number][];
      dora: number;
      ura: number;
      aka: number;
      han: number;
      fu: number;
      limit: string;
      ura_indicators: string[];
      deltas: number[];
      text: string;
    }
  | {
      type: "ryuukyoku";
      reason: string;
      tenpai: boolean[];
      nagashi: Seat[];
      deltas: number[];
      text: string;
    }
  | { type: "end_kyoku"; scores: number[]; text: string }
  | { type: "end_game"; scores: number[]; text: string };

export interface Frame {
  event: EventView;
  table: TableView;
  /** 自動代打的動作（立直後只能摸切時） */
  auto?: boolean;
}

export type ActionType =
  | "discard"
  | "riichi"
  | "chi"
  | "pon"
  | "daiminkan"
  | "ankan"
  | "kakan"
  | "tsumo"
  | "ron"
  | "kyuushu"
  | "pass";

export interface ActionView {
  index: number;
  type: ActionType;
  pai: string | null;
  tsumogiri: boolean;
  consumed: string[];
  label: string;
}

export interface HintDecision {
  kind: string;
  /** 決策種類的代號（實驗用） */
  kind_key: DecisionKind;
  options: string[];
  chosen: number;
  notes: string[];
}

export interface Efficiency {
  pai: string;
  shanten: number;
  tiles: string[];
  ukeire: number;
}

export interface Hint {
  action: number | null;
  label: string;
  decisions: HintDecision[];
  /** 要打牌時，每種打法的牌效（由好到壞） */
  efficiency: Efficiency[];
  /** 要打牌時，離和牌最近的幾條路線（向聽數 ≤ 2 時才有） */
  routes: WinRoute[];
}

/** 一條和牌路線。 */
export interface WinRoute {
  /** 要打掉的牌（第一張是現在要打的） */
  discards: string[];
  /** 還要摸到的牌 */
  draws: string[];
  /** 和了時的門前手牌（含和牌張） */
  hand: string[];
  win_tile: string;
  /** 役（名稱、番數）；無役時為空 */
  yaku: [string, number][];
  han: number;
  fu: number;
  limit: string;
  /** 榮和點數（無役為 0） */
  points: number;
  /** 點數假設立直 */
  riichi: boolean;
  /** 相對機會 */
  chance: number;
}

export interface Ask {
  id: number;
  frame: number;
  actions: ActionView[];
  hint: Hint | null;
}

export interface GameOver {
  scores: number[];
  ranks: number[];
  hands_played: number;
}

export interface Update {
  frames: Frame[];
  next: number;
  ask: Ask | null;
  game_over: GameOver | null;
  error: string | null;
}

export interface StrategyInfo {
  name: string;
  title: string;
  summary: string;
}

export interface SessionConfig {
  seed?: number | null;
  seat: Seat;
  opponents: [string, string, string];
  strategy: string;
  max_hands?: number | null;
}

export interface NewGame {
  seed: number;
  seat: Seat;
  names: string[];
}

export type DecisionKind = "Kyuushu" | "Win" | "Kan" | "Discard" | "Riichi" | "Call" | "Tedashi";

export interface Evaluation {
  /** 各選項的平均結果；沒有評估的選項為 null */
  option_ev: (number | null)[];
  best: number;
  diff: number;
  diff_se: number;
  rollouts: number;
}

export interface Reviewed {
  seat: Seat;
  hand_index: number;
  act_index: number;
  round: string;
  turn: number;
  riichi_opponents: number;
  kind: DecisionKind;
  options: string[];
  strategy: number;
  player: number;
  ai: number | null;
  notes: string[];
  difference: string | null;
  eval: Evaluation | null;
}

export interface GameReview {
  id: string;
  players: string[];
  seats: Seat[];
  metric: "Points" | "Placement";
  decisions: Reviewed[];
  incomparable: number;
  skipped: number;
}

export interface GroupSummary {
  label: string;
  decisions: number;
  agree: number;
  evaluated: number;
  mean_diff: number;
  mean_diff_se: number;
  mistakes: number;
  player_better: number;
}

export interface Summary {
  overall: GroupSummary;
  by_kind: GroupSummary[];
  by_type: GroupSummary[];
}

export type ReviewState =
  | { state: "idle" }
  | { state: "running"; seconds: number }
  | { state: "done"; review: GameReview; summary: Summary; seconds: number }
  | { state: "failed"; error: string };

export interface ReviewConfig {
  rollouts: number;
  metric: "points" | "placement";
  mjai?: string | null;
}

export const KIND_NAMES: Record<DecisionKind, string> = {
  Kyuushu: "九種九牌",
  Win: "和牌與否",
  Kan: "開槓",
  Discard: "何切",
  Riichi: "立直與否",
  Call: "鳴牌",
  Tedashi: "手切摸切",
};

/** 單一決策判為明顯失誤的門檻（與 mj-review 的 MISTAKE_Z 相同）。 */
export const MISTAKE_Z = 3.0;

/** 本專案匯出的牌譜附帶的資訊（mjai `start_game` 的 `mj_meta`）。 */
export interface LogMeta {
  seed?: number | null;
  seat?: Seat | null;
  strategy?: string | null;
}

/** 讀入的牌譜的概要。 */
export interface LogInfo {
  id: string;
  names: [string, string, string, string];
  hands: number;
  final_scores: [number, number, number, number];
  meta: LogMeta | null;
  events: number;
}

/** 某個座位視角的全部幀；asks = [詢問編號, 詢問當時的幀數]。 */
export interface LogFrames {
  frames: Frame[];
  asks: [number, number][];
}

/** 反事實實驗的對象：哪一次詢問的哪個決策。 */
export interface ExperimentTarget {
  source: "game" | "log";
  /** 讀入的牌譜：哪一家 */
  seat?: Seat;
  /** 詢問編號（第幾次詢問 + 1） */
  ask: number;
  kind: DecisionKind;
  options: string[];
  /** 策略的選擇（比較基準） */
  reference: number;
  /** 玩家的選擇（復盤時） */
  player?: number;
}

/** 一個選項的實驗結果。 */
export interface OptionStats {
  option: number;
  mean: number;
  se: number;
  diff: number;
  diff_se: number;
  win_rate: number;
  deal_in_rate: number;
  avg_rank: number | null;
}

export type ExperimentState =
  | { state: "idle" }
  | { state: "running"; done: number; total: number; seconds: number }
  | {
      state: "done";
      source: "game" | "log";
      seat: Seat;
      ask: number;
      kind: DecisionKind;
      reference: number;
      metric: "points" | "placement";
      worlds: number;
      cancelled: boolean;
      results: OptionStats[];
      seconds: number;
    }
  | { state: "failed"; error: string };
