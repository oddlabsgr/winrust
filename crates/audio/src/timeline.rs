//! Maps "frames played by the device" to tracks and positions.
//!
//! The decode thread records where each track starts in the device frame stream; the
//! callback counts frames actually played. Comparing the two tells us which track is
//! audible, where in it we are, and when it ended.

use crate::types::TrackInfo;
use std::collections::VecDeque;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum TimelineEvent {
    Started { info: TrackInfo, gapless: bool },
    Ended,
}

#[derive(Clone, Debug)]
struct Segment {
    start_frame: u64,
    base: Duration,
    info: TrackInfo,
    gapless: bool,
}

#[derive(Debug)]
pub struct Timeline {
    device_rate: u32,
    current: Option<Segment>,
    upcoming: VecDeque<Segment>,
    end_frame: Option<u64>,
}

impl Timeline {
    pub fn new(device_rate: u32) -> Self {
        Timeline {
            device_rate,
            current: None,
            upcoming: VecDeque::new(),
            end_frame: None,
        }
    }

    /// A track whose first frame will be device frame `start_frame`.
    pub fn push_track(&mut self, start_frame: u64, info: TrackInfo, gapless: bool) {
        self.upcoming.push_back(Segment {
            start_frame,
            base: Duration::ZERO,
            info,
            gapless,
        });
    }

    /// The last pushed track ends at device frame `end_frame` (exclusive).
    pub fn set_end(&mut self, end_frame: u64) {
        self.end_frame = Some(end_frame);
    }

    /// After a seek the current track restarts at `start_frame` from position `base`.
    pub fn reposition(&mut self, start_frame: u64, base: Duration) {
        self.upcoming.clear();
        self.end_frame = None;
        if let Some(seg) = self.current.as_mut() {
            seg.start_frame = start_frame;
            seg.base = base;
        }
    }

    pub fn clear(&mut self) {
        self.current = None;
        self.upcoming.clear();
        self.end_frame = None;
    }

    pub fn current(&self) -> Option<&TrackInfo> {
        self.current.as_ref().map(|s| &s.info)
    }

    /// Advances to `played` device frames and reports what changed.
    pub fn poll(&mut self, played: u64) -> Vec<TimelineEvent> {
        let mut events = Vec::new();
        while self
            .upcoming
            .front()
            .is_some_and(|s| s.start_frame <= played)
        {
            let seg = self.upcoming.pop_front().expect("checked front");
            events.push(TimelineEvent::Started {
                info: seg.info.clone(),
                gapless: seg.gapless,
            });
            self.current = Some(seg);
        }
        if self.upcoming.is_empty() && self.end_frame.is_some_and(|end| played >= end) {
            self.end_frame = None;
            if self.current.take().is_some() {
                events.push(TimelineEvent::Ended);
            }
        }
        events
    }

    /// Position inside the audible track.
    pub fn position(&self, played: u64) -> Option<Duration> {
        let seg = self.current.as_ref()?;
        let frames = played.saturating_sub(seg.start_frame);
        Some(seg.base + Duration::from_secs_f64(frames as f64 / f64::from(self.device_rate)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Source;

    fn info(name: &str) -> TrackInfo {
        TrackInfo {
            source: Source::File(name.into()),
            kbps: None,
            sample_rate: 44_100,
            channels: 2,
            duration: None,
            seekable: true,
        }
    }

    #[test]
    fn track_starts_when_its_first_frame_plays() {
        let mut t = Timeline::new(1_000);
        t.push_track(10, info("a"), false);
        assert!(t.poll(9).is_empty());
        assert_eq!(
            t.poll(10),
            vec![TimelineEvent::Started {
                info: info("a"),
                gapless: false
            }]
        );
        assert_eq!(t.position(510), Some(Duration::from_millis(500)));
    }

    #[test]
    fn gapless_handover_then_end() {
        let mut t = Timeline::new(1_000);
        t.push_track(0, info("a"), false);
        t.push_track(1_000, info("b"), true);
        t.set_end(1_500);
        t.poll(0);
        assert!(t.poll(999).is_empty());
        assert_eq!(
            t.poll(1_000),
            vec![TimelineEvent::Started {
                info: info("b"),
                gapless: true
            }]
        );
        assert_eq!(t.position(1_250), Some(Duration::from_millis(250)));
        assert_eq!(t.poll(1_500), vec![TimelineEvent::Ended]);
        assert_eq!(t.position(1_600), None);
        assert!(t.poll(2_000).is_empty(), "end reported once");
    }

    #[test]
    fn end_waits_for_pending_tracks() {
        let mut t = Timeline::new(1_000);
        t.push_track(0, info("a"), false);
        t.set_end(100);
        // Callback jumped past both the start and the end in one poll.
        assert_eq!(
            t.poll(200),
            vec![
                TimelineEvent::Started {
                    info: info("a"),
                    gapless: false
                },
                TimelineEvent::Ended
            ]
        );
    }

    #[test]
    fn reposition_after_seek() {
        let mut t = Timeline::new(1_000);
        t.push_track(0, info("a"), false);
        t.poll(0);
        t.reposition(400, Duration::from_secs(60));
        assert_eq!(t.position(400), Some(Duration::from_secs(60)));
        assert_eq!(t.position(900), Some(Duration::from_millis(60_500)));
    }
}
