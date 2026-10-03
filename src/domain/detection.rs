use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ConfigError {
    #[error("threshold must be finite and in the range (0, 1]")]
    Threshold,
    #[error("min-duration must be finite and greater than zero")]
    MinDuration,
    #[error("merge-gap must be finite and non-negative")]
    MergeGap,
}

#[derive(Debug, Clone, Copy)]
pub struct DetectionConfig {
    threshold: f64,
    min_duration: f64,
    merge_gap: f64,
}

impl DetectionConfig {
    pub fn new(threshold: f64, min_duration: f64, merge_gap: f64) -> Result<Self, ConfigError> {
        if !threshold.is_finite() || threshold <= 0.0 || threshold > 1.0 {
            return Err(ConfigError::Threshold);
        }
        if !min_duration.is_finite() || min_duration <= 0.0 {
            return Err(ConfigError::MinDuration);
        }
        if !merge_gap.is_finite() || merge_gap < 0.0 {
            return Err(ConfigError::MergeGap);
        }
        Ok(Self {
            threshold,
            min_duration,
            merge_gap,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MotionSample {
    /// The change occurred between this time minus sample_period and this time.
    pub end_seconds: f64,
    pub change_ratio: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MotionEvent {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub peak_change_ratio: f64,
}

pub fn detect_events(
    samples: &[MotionSample],
    sample_period: f64,
    duration: f64,
    config: DetectionConfig,
) -> Vec<MotionEvent> {
    let mut intervals: Vec<MotionEvent> = Vec::new();

    for sample in samples {
        if sample.change_ratio < config.threshold {
            continue;
        }
        let end = sample.end_seconds.min(duration);
        let start = (sample.end_seconds - sample_period).max(0.0).min(end);
        if let Some(last) = intervals.last_mut()
            && start - last.end_seconds <= config.merge_gap + 1e-9
        {
            last.end_seconds = end;
            last.peak_change_ratio = last.peak_change_ratio.max(sample.change_ratio);
            continue;
        }
        intervals.push(MotionEvent {
            start_seconds: start,
            end_seconds: end,
            peak_change_ratio: sample.change_ratio,
        });
    }

    intervals
        .into_iter()
        .filter(|event| event.end_seconds - event.start_seconds + 1e-9 >= config.min_duration)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_motion_across_a_short_quiet_gap_and_keeps_peak_change() {
        let config = DetectionConfig::new(0.05, 0.4, 0.4).unwrap();
        let samples = [
            MotionSample {
                end_seconds: 1.0,
                change_ratio: 0.1,
            },
            MotionSample {
                end_seconds: 1.2,
                change_ratio: 0.2,
            },
            MotionSample {
                end_seconds: 1.8,
                change_ratio: 0.15,
            },
        ];

        let events = detect_events(&samples, 0.2, 3.0, config);

        assert_eq!(events.len(), 1);
        assert!((events[0].start_seconds - 0.8).abs() < 1e-9);
        assert!((events[0].end_seconds - 1.8).abs() < 1e-9);
        assert_eq!(events[0].peak_change_ratio, 0.2);
    }

    #[test]
    fn rejects_isolated_change_and_quiet_samples() {
        let config = DetectionConfig::new(0.05, 0.4, 0.2).unwrap();
        let samples = [
            MotionSample {
                end_seconds: 1.0,
                change_ratio: 0.08,
            },
            MotionSample {
                end_seconds: 2.0,
                change_ratio: 0.01,
            },
        ];

        assert!(detect_events(&samples, 0.2, 3.0, config).is_empty());
    }

    #[test]
    fn rejects_non_finite_or_out_of_range_settings() {
        assert_eq!(
            DetectionConfig::new(f64::NAN, 0.4, 0.6).unwrap_err(),
            ConfigError::Threshold
        );
        assert_eq!(
            DetectionConfig::new(1.1, 0.4, 0.6).unwrap_err(),
            ConfigError::Threshold
        );
        assert_eq!(
            DetectionConfig::new(0.01, 0.0, 0.6).unwrap_err(),
            ConfigError::MinDuration
        );
        assert_eq!(
            DetectionConfig::new(0.01, 0.4, -0.1).unwrap_err(),
            ConfigError::MergeGap
        );
    }
}
