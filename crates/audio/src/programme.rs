//! A rolling programme cursor: no expanded playlists, decoding, or UI timers.
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug)]
pub enum Interval {
    Songs { min: u32, max: u32 },
    Time { min: f64, max: f64 },
}
#[derive(Clone, Debug)]
pub struct Bumpers {
    pub music_count: usize,
    pub lengths: Vec<f64>,
    pub interval: Interval,
    pub seed: u32,
    pub context: String,
    pub revision: u64,
}
impl Bumpers {
    pub fn valid(&self, count: usize) -> bool {
        self.music_count > 0
            && self.music_count < count
            && count - self.music_count <= crate::MAX_BUMPER_TRACKS
            && self.lengths.len() == count
            && self
                .lengths
                .iter()
                .all(|n| n.is_finite() && *n > 0.0 && *n <= 172800.0)
            && self.context.len() <= 2048
            && match self.interval {
                Interval::Songs { min, max } => min >= 1 && min <= max && max <= 1000,
                Interval::Time { min, max } => {
                    min.is_finite() && max.is_finite() && min >= 6.0 && min <= max && max <= 21600.0
                }
            }
    }
    pub fn fade_between(&self, a: usize, b: usize, requested: f64) -> f64 {
        if a >= self.music_count || b >= self.music_count {
            0.0
        } else {
            requested
                .min(self.lengths[a] / 2.0)
                .min(self.lengths[b] / 2.0)
        }
    }
    pub fn signature(&self, paths: &[std::path::PathBuf], fade: f64) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        paths.hash(&mut hash);
        self.music_count.hash(&mut hash);
        self.revision.hash(&mut hash);
        fade.to_bits().hash(&mut hash);
        // Improved duration measurements for unchanged files must not reset a
        // station's break counter. File revisions are hashed separately.
        match self.interval {
            Interval::Songs { min, max } => {
                0u8.hash(&mut hash);
                min.hash(&mut hash);
                max.hash(&mut hash);
            }
            Interval::Time { min, max } => {
                1u8.hash(&mut hash);
                min.to_bits().hash(&mut hash);
                max.to_bits().hash(&mut hash);
            }
        }
        hash.finish()
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Cursor {
    pub current: usize,
    next_music: usize,
    completed: u32,
    music_seconds: f64,
    target: f64,
    rng: u32,
    previous_bumper: Option<usize>,
}
impl Cursor {
    pub fn previous_bumper(&self) -> Option<usize> {
        self.previous_bumper
    }
    pub fn replace_bumper(&mut self, config: &Bumpers, index: usize) {
        self.current = index;
        self.previous_bumper = Some(index - config.music_count);
    }
    pub fn new(config: &Bumpers, index: usize, offset: f64) -> Self {
        let mut cursor = Self {
            current: index,
            next_music: (index + 1) % config.music_count,
            completed: 0,
            music_seconds: -offset,
            target: 0.0,
            rng: config.seed.max(1),
            previous_bumper: None,
        };
        cursor.reset_target(config);
        cursor
    }
    fn random(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        f64::from(self.rng) / 4294967296.0
    }
    fn reset_target(&mut self, config: &Bumpers) {
        let random = self.random();
        self.target = match config.interval {
            Interval::Songs { min, max } => {
                f64::from(min) + (random * f64::from(max - min + 1)).floor()
            }
            Interval::Time { min, max } => min + random * (max - min),
        };
    }
    pub fn following(&self, config: &Bumpers, fade: f64) -> Self {
        let mut next = self.clone();
        if self.current >= config.music_count {
            next.current = self.next_music;
            next.next_music = (next.current + 1) % config.music_count;
            next.completed = 0;
            next.music_seconds = 0.0;
            next.reset_target(config);
            return next;
        }
        let after = self.music_seconds + config.lengths[self.current];
        let next_boundary = after - config.fade_between(self.current, self.next_music, fade)
            + config.lengths[self.next_music];
        let due = match config.interval {
            Interval::Songs { .. } => f64::from(self.completed + 1) >= self.target,
            Interval::Time { .. } => {
                after >= self.target
                    || (self.target - after).abs() <= (next_boundary - self.target).abs()
            }
        };
        if due {
            let count = config.lengths.len() - config.music_count;
            let previous = self.previous_bumper.filter(|index| *index < count);
            let choices = if previous.is_some() && count > 1 {
                count - 1
            } else {
                count
            };
            let mut picked = (next.random() * choices as f64).floor() as usize;
            if let Some(previous) = previous.filter(|_| count > 1) {
                if picked >= previous {
                    picked += 1;
                }
            }
            next.current = config.music_count + picked;
            next.previous_bumper = Some(picked);
        } else {
            next.current = self.next_music;
            next.next_music = (next.current + 1) % config.music_count;
            next.completed += 1;
            next.music_seconds = after - config.fade_between(self.current, next.current, fade);
        }
        next
    }
    /// An unreadable song does not count toward an interval. A failed bumper
    /// ends that break attempt, so bad clips cannot create a retry loop.
    pub fn skip(&self, config: &Bumpers, fade: f64) -> Self {
        if self.current >= config.music_count {
            return self.following(config, fade);
        }
        let mut next = self.clone();
        next.current = self.next_music;
        next.next_music = (next.current + 1) % config.music_count;
        next
    }
    pub fn seek(mut self, config: &Bumpers, fade: f64, mut elapsed: f64) -> Option<(Self, f64)> {
        // Bounded even after an unusually long inactive session. Only cursor
        // arithmetic occurs here; no programme-sized allocation is retained.
        for _ in 0..200000 {
            let next = self.following(config, fade);
            let step = config.lengths[self.current]
                - config.fade_between(self.current, next.current, fade);
            if elapsed < step {
                return Some((self, elapsed.max(0.0)));
            }
            elapsed -= step;
            self = next;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(interval: Interval) -> Bumpers {
        Bumpers {
            music_count: 3,
            lengths: vec![180.0, 180.0, 180.0, 12.0, 20.0],
            interval,
            seed: 19,
            context: "all:station".into(),
            revision: 0,
        }
    }
    #[test]
    fn count_interval_spans_music_queue_wrap_and_never_counts_bumpers() {
        let config = config(Interval::Songs { min: 5, max: 5 });
        let mut cursor = Cursor::new(&config, 0, 0.0);
        for expected in [1, 2, 0, 1] {
            cursor = cursor.following(&config, 0.0);
            assert_eq!(cursor.current, expected);
        }
        cursor = cursor.following(&config, 0.0);
        assert!(cursor.current >= 3);
        cursor = cursor.following(&config, 0.0);
        assert_eq!(cursor.current, 2);
        assert_eq!(cursor.completed, 0);
        for _ in 0..4 {
            cursor = cursor.following(&config, 0.0);
            assert!(cursor.current < 3);
        }
        assert!(cursor.following(&config, 0.0).current >= 3);
    }
    #[test]
    fn ten_minutes_chooses_nine_minute_boundary_and_accounts_for_crossfade() {
        let config = config(Interval::Time {
            min: 600.0,
            max: 600.0,
        });
        let mut cursor = Cursor::new(&config, 0, 0.0);
        cursor = cursor.following(&config, 5.0);
        assert_eq!(cursor.current, 1);
        cursor = cursor.following(&config, 5.0);
        assert_eq!(cursor.current, 2);
        let bumper = cursor.following(&config, 5.0);
        assert!(bumper.current >= 3);
        assert_eq!(cursor.music_seconds + 180.0, 530.0);
        assert_eq!(
            config.fade_between(cursor.current, bumper.current, 5.0),
            0.0
        );
    }
    #[test]
    fn targets_are_redrawn_within_range_and_clips_do_not_repeat() {
        let config = config(Interval::Songs { min: 2, max: 5 });
        let mut cursor = Cursor::new(&config, 0, 0.0);
        let mut previous = None;
        let mut gaps = std::collections::HashSet::new();
        let mut songs = 0;
        for _ in 0..2000 {
            if cursor.current < 3 {
                songs += 1;
            } else {
                assert!((2..=5).contains(&songs));
                gaps.insert(songs);
                songs = 0;
                assert_ne!(previous, Some(cursor.current));
                previous = Some(cursor.current);
            }
            cursor = cursor.following(&config, 0.0);
        }
        assert!(gaps.len() > 1);
    }
    #[test]
    fn virtual_time_can_land_inside_a_bumper_and_resume_without_restart() {
        let config = config(Interval::Songs { min: 2, max: 2 });
        let initial = Cursor::new(&config, 0, 0.0);
        let (cursor, seconds) = initial.clone().seek(&config, 0.0, 365.0).unwrap();
        assert!(cursor.current >= 3);
        assert_eq!(seconds, 5.0);
        let (later, later_seconds) = cursor.seek(&config, 0.0, seconds + 200.0).unwrap();
        let (direct, direct_seconds) = initial.seek(&config, 0.0, 565.0).unwrap();
        assert_eq!(later.current, direct.current);
        assert_eq!(later_seconds, direct_seconds);
    }
    #[test]
    fn first_partial_song_counts_only_remaining_listening_time() {
        let config = config(Interval::Time {
            min: 600.0,
            max: 600.0,
        });
        let mut cursor = Cursor::new(&config, 0, 170.0);
        for _ in 0..3 {
            cursor = cursor.following(&config, 0.0);
            assert!(cursor.current < 3);
        }
        assert!(cursor.following(&config, 0.0).current >= 3);
    }
}
