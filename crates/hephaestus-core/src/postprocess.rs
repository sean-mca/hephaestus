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

/// Compute masked mean pooling over token embeddings.
///
/// Multiplies each token's embedding by its attention mask value (1 for
/// real tokens, 0 for padding), sums across the token dimension, and
/// divides by the sum of the mask. This produces a single pooled vector
/// of length `hidden_dim` that excludes padding tokens.
///
/// # Arguments
///
/// - `token_embeddings` -- flattened `(seq_len, hidden_dim)` tensor.
/// - `attention_mask` -- `(seq_len,)` vector with 1 for real tokens, 0 for padding.
/// - `hidden_dim` -- size of each token's embedding vector.
///
/// # Panics
///
/// Panics if `token_embeddings.len() != attention_mask.len() * hidden_dim`.
pub(crate) fn mean_pool(
    token_embeddings: &[f32],
    attention_mask: &[i64],
    hidden_dim: usize,
) -> Vec<f32> {
    let seq_len = attention_mask.len();
    assert_eq!(
        token_embeddings.len(),
        seq_len * hidden_dim,
        "token_embeddings length must equal seq_len * hidden_dim"
    );

    let mut pooled = vec![0.0_f32; hidden_dim];
    let mut mask_sum = 0.0_f32;

    for t in 0..seq_len {
        let mask_val = attention_mask[t] as f32;
        mask_sum += mask_val;
        for d in 0..hidden_dim {
            pooled[d] += token_embeddings[t * hidden_dim + d] * mask_val;
        }
    }

    let denom = mask_sum.max(1e-9);
    for val in &mut pooled {
        *val /= denom;
    }
    pooled
}

/// L2-normalize a vector to unit length in-place.
///
/// Computes the L2 norm (Euclidean length) and divides each element
/// by it. The norm is clamped to a minimum of `1e-12` to avoid
/// division by zero for zero vectors.
///
/// # Arguments
///
/// - `v` -- mutable slice to normalize in-place.
pub(crate) fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    for x in v.iter_mut() {
        *x /= norm;
    }
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

    #[test]
    fn test_mean_pool_excludes_padding() {
        // Arrange -- 3 tokens, hidden_dim=2, last token is padding (mask=0)
        let embeddings = [1.0_f32, 2.0, 3.0, 4.0, 100.0, 200.0];
        let mask = [1_i64, 1, 0];
        let hidden_dim = 2;

        // Act
        let pooled = mean_pool(&embeddings, &mask, hidden_dim);

        // Assert -- mean of first two tokens only: [(1+3)/2, (2+4)/2] = [2.0, 3.0]
        assert_eq!(pooled.len(), hidden_dim);
        assert!(
            (pooled[0] - 2.0).abs() < 1e-6,
            "expected 2.0, got {}",
            pooled[0]
        );
        assert!(
            (pooled[1] - 3.0).abs() < 1e-6,
            "expected 3.0, got {}",
            pooled[1]
        );
    }

    #[test]
    fn test_mean_pool_single_token() {
        // Arrange -- single real token, hidden_dim=3
        let embeddings = [0.5_f32, 1.5, 2.5];
        let mask = [1_i64];
        let hidden_dim = 3;

        // Act
        let pooled = mean_pool(&embeddings, &mask, hidden_dim);

        // Assert -- mean of one token is the token itself
        assert!((pooled[0] - 0.5).abs() < 1e-6);
        assert!((pooled[1] - 1.5).abs() < 1e-6);
        assert!((pooled[2] - 2.5).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_unit_vector() {
        // Arrange
        let mut v = [3.0_f32, 4.0];

        // Act
        l2_normalize(&mut v);

        // Assert -- L2 norm should be ~1.0
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-6,
            "L2 norm should be 1.0, got {norm}"
        );
        // 3/5 = 0.6, 4/5 = 0.8
        assert!((v[0] - 0.6).abs() < 1e-6, "expected 0.6, got {}", v[0]);
        assert!((v[1] - 0.8).abs() < 1e-6, "expected 0.8, got {}", v[1]);
    }

    #[test]
    fn test_l2_normalize_zero_vector() {
        // Arrange -- all zeros, should not panic or produce NaN
        let mut v = [0.0_f32, 0.0, 0.0];

        // Act
        l2_normalize(&mut v);

        // Assert -- values should remain near zero (divided by 1e-12 clamp)
        for &x in &v {
            assert!(x.is_finite(), "l2_normalize produced non-finite value: {x}");
        }
    }
}
