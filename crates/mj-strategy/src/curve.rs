use serde::{Deserialize, Serialize};

/// Piecewise-linear response curve mapping a raw consideration value to a utility.
/// Inputs outside the first/last point are clamped. An empty curve is the identity.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Curve(pub Vec<[f64; 2]>);

impl Curve {
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

    pub fn is_sorted(&self) -> bool {
        self.0.windows(2).all(|w| w[0][0] <= w[1][0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_when_empty() {
        assert_eq!(Curve::default().apply(3.5), 3.5);
    }

    #[test]
    fn interpolates_and_clamps() {
        let c = Curve(vec![[0.0, 0.0], [10.0, 1.0], [20.0, 1.0]]);
        assert_eq!(c.apply(-5.0), 0.0);
        assert_eq!(c.apply(5.0), 0.5);
        assert_eq!(c.apply(15.0), 1.0);
        assert_eq!(c.apply(99.0), 1.0);
    }
}
