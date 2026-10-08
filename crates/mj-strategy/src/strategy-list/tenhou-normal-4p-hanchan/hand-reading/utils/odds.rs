//! 勝算（odds）換算：讀牌預測的調整都寫成「勝算乘上幾倍」。
//!
//! 勝算 = p ÷ (1 − p)。例如機率 20% 的勝算是 0.25；乘上 2 倍變成 0.5，也就是機率 33%。
//! 用勝算調整的好處：幾個線索可以連乘，而且結果永遠落在 0–100% 之間（直接乘機率可能超過 100%）。

/// 把機率 `p` 的勝算乘上 `factor` 倍，回傳新的機率。
pub fn scale(p: f64, factor: f64) -> f64 {
    let p = p.clamp(1e-6, 1.0 - 1e-6);
    let o = p / (1.0 - p) * factor;
    o / (1.0 + o)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_odds() {
        assert!((scale(0.2, 2.0) - 1.0 / 3.0).abs() < 1e-9);
        assert!((scale(0.5, 1.0) - 0.5).abs() < 1e-9);
        assert!(scale(0.9, 10.0) < 1.0);
    }
}
