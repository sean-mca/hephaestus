//! Post-processing utilities for inference output.
//!
//! Provides numerically stable softmax and argmax functions
//! used by pipeline implementations to convert raw model logits
//! into human-readable predictions.

/// Compute a numerically stable softmax over `logits`.
///
/// Subtracts the maximum value before exponentiation to prevent
/// overflow with large logit values.
///
/// # Panics
///
/// Panics if `logits` is empty.
pub(crate) fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|&x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.iter().map(|&e| e / sum).collect()
}

/// Return the index and value of the maximum element in `probs`.
///
/// Ties are broken by first occurrence (lowest index wins).
///
/// # Panics
///
/// Panics if `probs` is empty.
pub(crate) fn argmax_with_score(probs: &[f32]) -> (usize, f32) {
    probs
        .iter()
        .enumerate()
        .fold((0, f32::NEG_INFINITY), |(max_idx, max_val), (idx, &val)| {
            if val > max_val {
                (idx, val)
            } else {
                (max_idx, max_val)
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_softmax_basic() {
        // Arrange
        let logits = [1.0_f32, 2.0, 3.0];

        // Act
        let probs = softmax(&logits);

        // Assert
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6, "softmax must sum to 1.0, got {sum}");
        assert!(probs[2] > probs[1]);
        assert!(probs[1] > probs[0]);
    }

    #[test]
    fn test_softmax_uniform() {
        // Arrange
        let logits = [1.0_f32, 1.0, 1.0, 1.0];

        // Act
        let probs = softmax(&logits);

        // Assert
        for &p in &probs {
            assert!(
                (p - 0.25).abs() < 1e-6,
                "equal logits must produce equal probabilities, got {p}"
            );
        }
    }

    #[test]
    fn test_softmax_large_values() {
        // Arrange -- values large enough to overflow naive exp()
        let logits = [1000.0_f32, 1001.0];

        // Act
        let probs = softmax(&logits);

        // Assert -- must not produce NaN or Inf
        for &p in &probs {
            assert!(p.is_finite(), "softmax produced non-finite value: {p}");
        }
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6, "softmax must sum to 1.0, got {sum}");
        assert!(probs[1] > probs[0], "larger logit must have higher probability");
    }

    #[test]
    fn test_softmax_single() {
        // Arrange
        let logits = [42.0_f32];

        // Act
        let probs = softmax(&logits);

        // Assert
        assert_eq!(probs.len(), 1);
        assert!((probs[0] - 1.0).abs() < 1e-6, "single element softmax must be 1.0");
    }

    #[test]
    fn test_argmax_basic() {
        // Arrange
        let probs = [0.1_f32, 0.7, 0.2];

        // Act
        let (idx, score) = argmax_with_score(&probs);

        // Assert
        assert_eq!(idx, 1);
        assert!((score - 0.7).abs() < 1e-6);
    }

    #[test]
    fn test_argmax_first_wins() {
        // Arrange -- tied values; first occurrence should win
        let probs = [0.5_f32, 0.5, 0.3];

        // Act
        let (idx, score) = argmax_with_score(&probs);

        // Assert
        assert_eq!(idx, 0, "first occurrence should win on tie");
        assert!((score - 0.5).abs() < 1e-6);
    }
}
