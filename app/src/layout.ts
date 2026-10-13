// 牌桌版面：依可用空間決定牌桌邊長（CSS 變數 --B）。對局與牌譜檢視共用。

import { useEffect, useState, type RefObject } from "react";

/** 自己的手牌疊進牌桌下緣的比例（與 styles.css 的 `.hand-wrap` 一致）。 */
export const HAND_OVERLAP = 0.09;

/**
 * 牌桌邊長（px）：牌桌區域扣掉自己手牌區的實際高度後，取寬高中較小的。
 * 自己的手牌疊在牌桌下緣的空白帶上（[`HAND_OVERLAP`] × 邊長，那裡原本只有自己的牌河以下的空位）。
 * 手牌的大小又依牌桌邊長而定，所以兩者都監看大小變化，幾次之內就會穩定。
 */
export function useBoardSize(
  area: RefObject<HTMLDivElement | null>,
  hand: RefObject<HTMLDivElement | null>,
  /** 牌桌已經畫出來（之前是「準備中」畫面，還沒有可以量的元素） */
  ready: boolean,
): number {
  const [size, setSize] = useState(600);
  useEffect(() => {
    const el = area.current;
    const h = hand.current;
    if (!el || !h) return;
    const update = () => {
      const style = getComputedStyle(el);
      const pad = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
      const gap = parseFloat(style.rowGap) || 0;
      const w = el.clientWidth - 16;
      const free = (el.clientHeight - pad - gap - h.offsetHeight) / (1 - HAND_OVERLAP);
      const next = Math.max(280, Math.floor(Math.min(w, free)));
      setSize((old) => (Math.abs(old - next) > 1 ? next : old));
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    ro.observe(h);
    return () => ro.disconnect();
  }, [area, hand, ready]);
  return size;
}
