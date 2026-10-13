// 音效：用 Web Audio 即時合成（不需要音效檔，也沒有授權問題）。
// 瀏覽器要求使用者先和頁面互動過才能發聲；開局按鈕就是一次互動，之後都能播放。

export type SoundKind = "riichi" | "call" | "win";

let ctx: AudioContext | null = null;

function audio(): AudioContext | null {
  try {
    ctx ??= new AudioContext();
    if (ctx.state === "suspended") void ctx.resume();
    return ctx;
  } catch {
    return null;
  }
}

/** 一個音：頻率（Hz）、開始時間與長度（秒）、波形、音量。 */
function tone(ac: AudioContext, freq: number, start: number, dur: number, type: OscillatorType, gain: number) {
  const osc = ac.createOscillator();
  const g = ac.createGain();
  osc.type = type;
  osc.frequency.value = freq;
  const t = ac.currentTime + start;
  // 快速起音、指數衰減，聽起來像敲擊。
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(gain, t + 0.01);
  g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
  osc.connect(g).connect(ac.destination);
  osc.start(t);
  osc.stop(t + dur + 0.02);
}

/** 播放音效；`volume` 0–1。 */
export function play(kind: SoundKind, volume: number) {
  if (volume <= 0) return;
  const ac = audio();
  if (!ac) return;
  const v = 0.25 * volume;
  switch (kind) {
    // 立直：明亮的上行兩音
    case "riichi":
      tone(ac, 880, 0, 0.25, "triangle", v);
      tone(ac, 1318.5, 0.12, 0.45, "triangle", v);
      break;
    // 鳴牌：短促的低音敲擊
    case "call":
      tone(ac, 330, 0, 0.15, "square", v * 0.5);
      tone(ac, 494, 0.06, 0.18, "triangle", v);
      break;
    // 和牌：上行琶音
    case "win":
      [523.3, 659.3, 784, 1046.5].forEach((f, i) => tone(ac, f, i * 0.09, 0.5, "triangle", v));
      break;
  }
}
