//! 響應曲線（response curve）。

use serde::{Deserialize, Serialize};

/// 分段線性的響應曲線：把考量的原始值映射成效用值。
/// 以控制點 `[[x, y], ...]` 表示（x 須遞增），點與點之間線性內插；
/// 超出第一個或最後一個點的輸入會被夾住（取端點的值）。沒有控制點時為恆等映射。
///
/// 用途例：進張 0→0、20→0.7、40→1.0，表示進張越多越好，但超過 20 張後邊際效益遞減。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Curve(pub Vec<[f64; 2]>);

impl Curve {
    /// 計算 x 對應的效用值。
    pub fn apply(&self, x: f64) -> f64 {
        let pts = &self.0;
        let (Some(first), Some(last)) = (pts.first(), pts.last()) else {
            return x;
        };
        if x <= first[0] {
            return first[1];
        }
        if x >= last[0] {
            return last[1];
        }
        for w in pts.windows(2) {
            let ([x0, y0], [x1, y1]) = (w[0], w[1]);
            if x <= x1 {
                return if x1 == x0 {
                    y1
                } else {
                    y0 + (y1 - y0) * (x - x0) / (x1 - x0)
                };
            }
        }
        last[1]
    }

    /// 控制點的 x 是否遞增（載入側寫時檢查）。
    pub fn is_sorted(&self) -> bool {
        self.0.windows(2).all(|w| w[0][0] <= w[1][0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 沒有控制點時為恆等映射。
    #[test]
    fn identity_when_empty() {
        assert_eq!(Curve::default().apply(3.5), 3.5);
    }

    /// 線性內插與兩端夾住。
    #[test]
    fn interpolates_and_clamps() {
        let c = Curve(vec![[0.0, 0.0], [10.0, 1.0], [20.0, 1.0]]);
        assert_eq!(c.apply(-5.0), 0.0);
        assert_eq!(c.apply(5.0), 0.5);
        assert_eq!(c.apply(15.0), 1.0);
        assert_eq!(c.apply(99.0), 1.0);
    }
}
