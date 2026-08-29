//! CTC greedy decoder for connectionist temporal classification output.
//!
//! Implements the standard CTC greedy decoding algorithm: argmax per
//! timestep, collapse repeated tokens, remove blanks. Used by the
//! [`AsrPipeline`](crate::pipeline::AsrPipeline) for CTC-based models
//! like wav2vec2 and HuBERT.

/// Decode CTC logits into text using greedy decoding.
///
/// Algorithm: for each timestep, take the argmax token. Collapse
/// consecutive repeated tokens, then remove blank tokens. Look up
/// the resulting token indices in the vocabulary to produce the
/// final text string.
///
/// # Arguments
///
/// * `logits` -- Flattened logits array of shape `[num_timesteps, vocab_size]`.
/// * `num_timesteps` -- Number of time steps in the logits.
/// * `vocab_size` -- Size of the vocabulary (number of classes per timestep).
/// * `vocab` -- Vocabulary mapping from index to token string.
/// * `blank_id` -- Index of the CTC blank token.
///
/// # Returns
///
/// Decoded text string with blanks removed and repeats collapsed.
pub fn ctc_greedy_decode(
    logits: &[f32],
    num_timesteps: usize,
    vocab_size: usize,
    vocab: &[String],
    blank_id: usize,
) -> String {
    let mut decoded: Vec<&str> = Vec::new();
    let mut prev_token = blank_id;

    for t in 0..num_timesteps {
        let start = t * vocab_size;
        let end = start + vocab_size;
        let timestep_logits = &logits[start..end];

        // Argmax over the vocabulary dimension.
        let best_idx = timestep_logits
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| idx)
            .unwrap_or(blank_id);

        // Collapse repeats and skip blanks.
        if best_idx != blank_id && best_idx != prev_token {
            if let Some(token) = vocab.get(best_idx) {
                decoded.push(token.as_str());
            }
        }

        prev_token = best_idx;
    }

    decoded.join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build logits where each timestep has a clear argmax at the given index.
    fn make_logits(timestep_labels: &[usize], vocab_size: usize) -> Vec<f32> {
        let mut logits = vec![0.0_f32; timestep_labels.len() * vocab_size];
        for (t, &label) in timestep_labels.iter().enumerate() {
            logits[t * vocab_size + label] = 10.0;
        }
        logits
    }

    #[test]
    fn test_ctc_greedy_decode_basic() {
        // Vocab: 0 = blank, 1 = "h", 2 = "e", 3 = "l"
        let vocab: Vec<String> = vec![
            "<blank>".into(),
            "h".into(),
            "e".into(),
            "l".into(),
        ];
        // Sequence: [1, 1, 0, 2, 2, 0, 3] -> collapse/deblank -> "hel"
        let labels = &[1, 1, 0, 2, 2, 0, 3];
        let logits = make_logits(labels, 4);

        let result = ctc_greedy_decode(&logits, 7, 4, &vocab, 0);
        assert_eq!(result, "hel");
    }

    #[test]
    fn test_ctc_greedy_decode_all_blank() {
        let vocab: Vec<String> = vec!["<blank>".into(), "a".into()];
        let labels = &[0, 0, 0, 0];
        let logits = make_logits(labels, 2);

        let result = ctc_greedy_decode(&logits, 4, 2, &vocab, 0);
        assert_eq!(result, "");
    }

    #[test]
    fn test_ctc_greedy_decode_single_token() {
        let vocab: Vec<String> = vec!["<blank>".into(), "x".into()];
        let labels = &[1];
        let logits = make_logits(labels, 2);

        let result = ctc_greedy_decode(&logits, 1, 2, &vocab, 0);
        assert_eq!(result, "x");
    }

    #[test]
    fn test_ctc_greedy_decode_repeated_different_tokens() {
        // Same token appearing non-consecutively should appear twice.
        let vocab: Vec<String> = vec!["<blank>".into(), "a".into(), "b".into()];
        // [1, 0, 1] -> "a" + blank + "a" -> "aa"
        let labels = &[1, 0, 1];
        let logits = make_logits(labels, 3);

        let result = ctc_greedy_decode(&logits, 3, 3, &vocab, 0);
        assert_eq!(result, "aa");
    }
}
