use super::detection::MotionEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightSegment {
    pub start_frame: u64,
    pub end_frame: u64,
}

#[derive(Debug)]
pub struct HighlightPlan {
    pub segments: Vec<HighlightSegment>,
    pub event_start_seconds: Vec<f64>,
}

pub fn plan_highlight(
    events: &[MotionEvent],
    duration_seconds: f64,
    padding_seconds: f64,
    frame_rate: u64,
) -> HighlightPlan {
    let last_frame = (duration_seconds * frame_rate as f64).ceil() as u64;
    let mut segments: Vec<HighlightSegment> = Vec::new();

    for event in events {
        let start =
            ((event.start_seconds - padding_seconds).max(0.0) * frame_rate as f64).floor() as u64;
        let end = ((event.end_seconds + padding_seconds).min(duration_seconds) * frame_rate as f64)
            .ceil() as u64;
        if let Some(last) = segments.last_mut()
            && start <= last.end_frame
        {
            last.end_frame = last.end_frame.max(end);
        } else {
            segments.push(HighlightSegment {
                start_frame: start,
                end_frame: end,
            });
        }
    }

    let mut event_start_seconds = Vec::with_capacity(events.len());
    let mut segment_index = 0;
    let mut elapsed_frames = 0;
    for event in events {
        let frame = ((event.start_seconds * frame_rate as f64).floor() as u64)
            .min(last_frame.saturating_sub(1));
        while frame >= segments[segment_index].end_frame {
            elapsed_frames +=
                segments[segment_index].end_frame - segments[segment_index].start_frame;
            segment_index += 1;
        }
        let segment = segments[segment_index];
        event_start_seconds.push(
            (elapsed_frames + frame.saturating_sub(segment.start_frame)) as f64 / frame_rate as f64,
        );
    }

    HighlightPlan {
        segments,
        event_start_seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(start_seconds: f64, end_seconds: f64) -> MotionEvent {
        MotionEvent {
            start_seconds,
            end_seconds,
            peak_change_ratio: 0.1,
        }
    }

    #[test]
    fn overlapping_padding_uses_each_source_frame_once() {
        let events = [event(2.0, 3.0), event(3.5, 4.5), event(8.0, 9.0)];

        let plan = plan_highlight(&events, 10.0, 1.0, 15);

        assert_eq!(
            plan.segments,
            [
                HighlightSegment {
                    start_frame: 15,
                    end_frame: 83,
                },
                HighlightSegment {
                    start_frame: 105,
                    end_frame: 150,
                },
            ]
        );
        assert!((plan.event_start_seconds[0] - 1.0).abs() < 1e-9);
        assert!((plan.event_start_seconds[1] - 37.0 / 15.0).abs() < 1e-9);
        assert!((plan.event_start_seconds[2] - 83.0 / 15.0).abs() < 1e-9);
    }

    #[test]
    fn no_events_have_no_segments_or_seek_positions() {
        let plan = plan_highlight(&[], 10.0, 1.0, 15);

        assert!(plan.segments.is_empty());
        assert!(plan.event_start_seconds.is_empty());
    }
}
