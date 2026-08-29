//! Mel spectrogram computation for Whisper-compatible audio preprocessing.
//!
//! Wraps the [`mel_spec`] crate to compute log-mel spectrograms from raw
//! audio samples. The output matches the feature extraction pipeline used
//! by Whisper models: STFT with Hann window, mel filterbank projection,
//! log10 scaling with Whisper normalization.

use ndarray::Array2;

use crate::error::CoreError;

/// Compute a Whisper-compatible log-mel spectrogram from raw audio samples.
///
/// Uses the `mel_spec` crate's batch CPU path (`Spectrogram::compute_mel_spectrogram_cpu`)
/// which internally computes STFT frames, applies a mel filterbank, and returns
/// log-normalized mel features per frame.
///
/// # Arguments
///
/// * `samples` -- Mono audio samples (f32, normalized to [-1.0, 1.0]).
/// * `n_fft` -- FFT window size (typically 400 for Whisper at 16kHz).
/// * `hop_length` -- Hop size between consecutive STFT frames (typically 160).
/// * `n_mels` -- Number of mel filter banks (typically 80 or 128).
/// * `sample_rate` -- Audio sample rate in Hz (typically 16000).
///
/// # Returns
///
/// `Array2<f32>` of shape `[n_mels, num_frames]` where each column is one
/// mel frame. Returns an error if the input is too short for even one FFT
/// window.
pub fn compute_mel_spectrogram(
    samples: &[f32],
    n_fft: usize,
    hop_length: usize,
    n_mels: usize,
    sample_rate: u32,
) -> Result<Array2<f32>, CoreError> {
    if samples.len() < n_fft {
        return Err(CoreError::Inference(format!(
            "audio too short for mel spectrogram: {} samples < {} (n_fft)",
            samples.len(),
            n_fft,
        )));
    }

    // mel_spec returns Vec<Vec<f32>> where each inner vec is one frame
    // of n_mels values (the mel_spec MelSpectrogram::add normalizes internally).
    let frames = mel_spec::stft::Spectrogram::compute_mel_spectrogram_cpu(
        samples,
        n_fft,
        hop_length,
        n_mels,
        sample_rate as f64,
    );

    let num_frames = frames.len();
    if num_frames == 0 {
        return Err(CoreError::Inference(
            "mel spectrogram produced zero frames".to_string(),
        ));
    }

    // Transpose from [num_frames, n_mels] to [n_mels, num_frames].
    let mut data = vec![0.0_f32; n_mels * num_frames];
    for (frame_idx, frame) in frames.iter().enumerate() {
        for (mel_idx, &val) in frame.iter().enumerate() {
            data[mel_idx * num_frames + frame_idx] = val;
        }
    }

    Array2::from_shape_vec((n_mels, num_frames), data).map_err(|e| {
        CoreError::Inference(format!("mel spectrogram array construction failed: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mel_spectrogram_shape_from_sine_wave() {
        // Generate a 1-second 440Hz sine wave at 16kHz.
        let sample_rate = 16000_u32;
        let duration_secs = 1.0_f32;
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let samples: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();

        let n_fft = 400;
        let hop_length = 160;
        let n_mels = 80;

        let result = compute_mel_spectrogram(&samples, n_fft, hop_length, n_mels, sample_rate);
        let mel = result.expect("should compute mel spectrogram");

        // Shape should be [n_mels, num_frames].
        assert_eq!(mel.nrows(), n_mels);
        assert!(mel.ncols() > 0, "should have at least one frame");

        // All values should be finite.
        assert!(
            mel.iter().all(|v| v.is_finite()),
            "all mel values should be finite"
        );
    }

    #[test]
    fn test_mel_spectrogram_rejects_short_audio() {
        // Audio shorter than n_fft should error.
        let samples = vec![0.0_f32; 100];
        let result = compute_mel_spectrogram(&samples, 400, 160, 80, 16000);
        assert!(result.is_err());
    }
}
