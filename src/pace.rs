use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::models::UsageSection;

const DAY_SECONDS: f64 = 86_400.0;
const FIVE_HOUR_SECONDS: f64 = 5.0 * 60.0 * 60.0;
const SAMPLE_INTERVAL_SECONDS: i64 = 15 * 60;
const RECENT_WINDOW_SECONDS: i64 = 72 * 60 * 60;
const MINIMUM_OBSERVED_SECONDS: i64 = 24 * 60 * 60;
const MAX_SAMPLES: usize = 400;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Level {
    #[default]
    Normal,
    Caution,
    Warning,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Assessment {
    pub level: Level,
    pub budget_per_day: Option<f64>,
    pub recent_per_day: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct History {
    samples: Vec<Sample>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct Sample {
    timestamp: i64,
    used: f64,
    reset: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StoredHistory {
    #[serde(default)]
    samples: Vec<Sample>,
}

impl History {
    pub fn load() -> Self {
        let Some(path) = history_path() else {
            return Self::default();
        };

        let Ok(contents) = fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(stored) = serde_json::from_str::<StoredHistory>(&contents) else {
            return Self::default();
        };

        let mut samples: Vec<_> = stored
            .samples
            .into_iter()
            .filter(|sample| sample.used.is_finite())
            .collect();
        samples.sort_by_key(|sample| sample.timestamp);
        trim_samples(&mut samples);

        Self { samples }
    }

    pub fn record(&mut self, section: &UsageSection, now: SystemTime) {
        let Some(sample) = Sample::from_section(section, now) else {
            return;
        };

        let Some(last) = self.samples.last().copied() else {
            self.samples.push(sample);
            self.save();
            return;
        };

        let Some(elapsed) = sample.timestamp.checked_sub(last.timestamp) else {
            return;
        };
        if elapsed < 0 {
            return;
        }

        let starts_new_history = sample.reset != last.reset || sample.used < last.used;
        if starts_new_history {
            self.samples.clear();
        }

        let should_record = starts_new_history
            || self
                .samples
                .last()
                .map(|previous| {
                    sample
                        .timestamp
                        .checked_sub(previous.timestamp)
                        .is_some_and(|seconds| seconds >= SAMPLE_INTERVAL_SECONDS)
                })
                .unwrap_or(true);

        if should_record {
            self.samples.push(sample);
            trim_samples(&mut self.samples);
            self.save();
        }
    }

    pub fn weekly(&self, section: &UsageSection, now: SystemTime) -> Assessment {
        let Some(current) = Sample::from_section(section, now) else {
            return Assessment::default();
        };

        let mut generation: Vec<_> = self
            .samples
            .iter()
            .copied()
            .filter(|sample| sample.timestamp <= current.timestamp && sample.used.is_finite())
            .collect();
        generation.sort_by_key(|sample| sample.timestamp);

        // A reset or a decreasing percentage is a new baseline. Keep only the
        // suffix after the most recent boundary so an old cycle cannot affect
        // the current prediction.
        let mut generation_start = 0;
        for index in 1..generation.len() {
            if generation[index].reset != generation[index - 1].reset
                || generation[index].used < generation[index - 1].used
            {
                generation_start = index;
            }
        }
        if generation_start > 0 {
            generation = generation.split_off(generation_start);
        }

        // The current observation may arrive before record() is called. Treat
        // a boundary here exactly as record() would, without mutating disk.
        if generation
            .last()
            .is_some_and(|last| current.reset != last.reset || current.used < last.used)
        {
            generation.clear();
        }

        let cutoff = current.timestamp.saturating_sub(RECENT_WINDOW_SECONDS);
        generation
            .retain(|sample| sample.timestamp < current.timestamp && sample.reset == current.reset);
        generation.push(current);
        // A long offline interval contributes its average rate, not invented
        // day-by-day activity. Interpolate only inside two real observations.
        if let Some(index) = generation
            .iter()
            .position(|sample| sample.timestamp >= cutoff)
        {
            if index > 0 {
                let previous = generation[index - 1];
                let next = generation[index];
                let fraction = (cutoff - previous.timestamp) as f64
                    / (next.timestamp - previous.timestamp) as f64;
                let boundary = Sample {
                    timestamp: cutoff,
                    used: previous.used + (next.used - previous.used) * fraction,
                    reset: current.reset,
                };
                generation.drain(..index);
                if generation[0].timestamp != cutoff {
                    generation.insert(0, boundary);
                }
            }
        }

        let recent_per_day = recent_per_day(&generation);
        let budget_per_day = budget_per_day(section, now);
        let level = prediction_level(budget_per_day, recent_per_day);

        Assessment {
            level,
            budget_per_day,
            recent_per_day,
        }
    }

    fn save(&self) {
        let Some(path) = history_path() else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }

        let stored = StoredHistory {
            samples: self.samples.clone(),
        };
        let Ok(contents) = serde_json::to_vec(&stored) else {
            return;
        };
        let _ = fs::write(path, contents);
    }
}

pub fn five_hour(section: &UsageSection, now: SystemTime) -> Level {
    if !valid_percentage(section.percentage) {
        return Level::Normal;
    }

    let Some(reset) = section.resets_at else {
        return Level::Normal;
    };
    let Ok(remaining_time) = reset.duration_since(now) else {
        return Level::Normal;
    };
    if remaining_time.is_zero() || remaining_time.as_secs_f64() > FIVE_HOUR_SECONDS {
        return Level::Normal;
    }

    let remaining = 100.0 - section.percentage;
    if !remaining.is_finite() {
        return Level::Normal;
    }
    if remaining <= 0.0 {
        return Level::Warning;
    }

    let remaining = remaining.min(100.0);
    let ratio = remaining / (remaining_time.as_secs_f64() / FIVE_HOUR_SECONDS * 100.0);
    if !ratio.is_finite() {
        return Level::Normal;
    }

    if ratio < 0.75 {
        Level::Warning
    } else if ratio < 1.0 {
        Level::Caution
    } else {
        Level::Normal
    }
}

fn recent_per_day(samples: &[Sample]) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }

    let first = samples.first()?;
    let last = samples.last()?;
    let elapsed = last.timestamp.checked_sub(first.timestamp)?;
    if elapsed < MINIMUM_OBSERVED_SECONDS {
        return None;
    }

    let increase = last.used - first.used;
    if !increase.is_finite() || increase < 0.0 {
        return None;
    }

    let pace = increase * DAY_SECONDS / elapsed as f64;
    pace.is_finite().then_some(pace)
}

fn budget_per_day(section: &UsageSection, now: SystemTime) -> Option<f64> {
    if !valid_percentage(section.percentage) {
        return None;
    }

    let reset = section.resets_at?;
    let seconds_until_reset = reset.duration_since(now).ok()?.as_secs_f64();
    if !seconds_until_reset.is_finite() || seconds_until_reset <= 0.0 {
        return None;
    }

    let remaining = (100.0 - section.percentage).clamp(0.0, 100.0);
    let budget = remaining / (seconds_until_reset / DAY_SECONDS);
    budget.is_finite().then_some(budget)
}

fn prediction_level(budget: Option<f64>, recent: Option<f64>) -> Level {
    let (Some(budget), Some(recent)) = (budget, recent) else {
        return Level::Normal;
    };
    if !budget.is_finite() || !recent.is_finite() {
        return Level::Normal;
    }

    if recent > budget * 1.25 {
        Level::Warning
    } else if recent > budget {
        Level::Caution
    } else {
        Level::Normal
    }
}

impl Sample {
    fn from_section(section: &UsageSection, now: SystemTime) -> Option<Self> {
        if !valid_percentage(section.percentage) {
            return None;
        }

        Some(Self {
            timestamp: system_time_seconds(now)?,
            used: section.percentage,
            reset: section.resets_at.and_then(system_time_seconds),
        })
    }
}

fn valid_percentage(value: f64) -> bool {
    value.is_finite() && (0.0..=100.0).contains(&value)
}

fn history_path() -> Option<std::path::PathBuf> {
    Some(
        dirs::data_local_dir()?
            .join("CodexUsage")
            .join("pace-history.json"),
    )
}

fn system_time_seconds(time: SystemTime) -> Option<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).ok(),
        Err(error) => {
            let seconds = i64::try_from(error.duration().as_secs()).ok()?;
            Some(-seconds)
        }
    }
}

fn trim_samples(samples: &mut Vec<Sample>) {
    if samples.len() > MAX_SAMPLES {
        let remove = samples.len() - MAX_SAMPLES;
        samples.drain(..remove);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::UsageSection;
    use std::time::Duration;

    const HOUR: u64 = 60 * 60;
    const DAY: u64 = 24 * HOUR;

    fn section(percentage: f64, resets_at: Option<SystemTime>) -> UsageSection {
        UsageSection {
            percentage,
            resets_at,
        }
    }

    fn sample(timestamp: SystemTime, used: f64, reset: Option<SystemTime>) -> Sample {
        Sample {
            timestamp: system_time_seconds(timestamp).unwrap(),
            used,
            reset: reset.and_then(system_time_seconds),
        }
    }

    #[test]
    fn five_hour_levels_cover_boundaries_and_invalid_resets() {
        let now = UNIX_EPOCH + Duration::from_secs(200 * HOUR);
        let in_five_hours = now + Duration::from_secs(5 * HOUR);

        assert_eq!(
            five_hour(&section(20.0, Some(in_five_hours)), now),
            Level::Caution
        );
        assert_eq!(
            five_hour(
                &section(60.0, Some(now + Duration::from_secs(4 * HOUR))),
                now
            ),
            Level::Warning
        );
        assert_eq!(
            five_hour(&section(100.0, Some(in_five_hours)), now),
            Level::Warning
        );
        assert_eq!(five_hour(&section(20.0, None), now), Level::Normal);
        assert_eq!(
            five_hour(&section(20.0, Some(now - Duration::from_secs(HOUR))), now),
            Level::Normal
        );
    }

    #[test]
    fn weekly_without_history_keeps_normal_level() {
        let now = UNIX_EPOCH + Duration::from_secs(200 * HOUR);
        let assessment = History::default().weekly(
            &section(20.0, Some(now + Duration::from_secs(7 * DAY))),
            now,
        );

        assert_eq!(assessment.level, Level::Normal);
        assert_eq!(assessment.recent_per_day, None);
        assert!(assessment.budget_per_day.is_some());
    }

    #[test]
    fn weekly_uses_elapsed_time_and_restarts_after_a_usage_drop() {
        let now = UNIX_EPOCH + Duration::from_secs(200 * HOUR);
        let reset = now + Duration::from_secs(5 * DAY);
        let mut history = History::default();
        history.samples = vec![
            sample(now - Duration::from_secs(48 * HOUR), 20.0, Some(reset)),
            sample(now - Duration::from_secs(24 * HOUR), 30.0, Some(reset)),
        ];

        let steady = history.weekly(&section(40.0, Some(reset)), now);
        assert_eq!(steady.level, Level::Normal);
        assert_eq!(steady.recent_per_day, Some(10.0));

        history.samples[1].used = 50.0;
        let after_drop = history.weekly(&section(20.0, Some(reset)), now);
        assert_eq!(after_drop.level, Level::Normal);
        assert_eq!(after_drop.recent_per_day, None);
    }

    #[test]
    fn weekly_does_not_bridge_a_reset() {
        let now = UNIX_EPOCH + Duration::from_secs(200 * HOUR);
        let old_reset = now + Duration::from_secs(2 * DAY);
        let new_reset = now + Duration::from_secs(7 * DAY);
        let mut history = History::default();
        history.samples = vec![
            sample(now - Duration::from_secs(48 * HOUR), 20.0, Some(old_reset)),
            sample(now - Duration::from_secs(24 * HOUR), 40.0, Some(old_reset)),
        ];

        let assessment = history.weekly(&section(5.0, Some(new_reset)), now);
        assert_eq!(assessment.level, Level::Normal);
        assert_eq!(assessment.recent_per_day, None);
    }

    #[test]
    fn recent_window_forgets_early_spending_and_averages_offline_gaps() {
        let now = UNIX_EPOCH + Duration::from_secs(10 * DAY);
        let reset = now + Duration::from_secs(2 * DAY);
        let history = History {
            samples: vec![
                sample(now - Duration::from_secs(4 * DAY), 0.0, Some(reset)),
                sample(now - Duration::from_secs(3 * DAY), 50.0, Some(reset)),
            ],
        };
        let slow = history.weekly(&section(56.0, Some(reset)), now);
        assert_eq!(slow.recent_per_day, Some(2.0));
        assert_eq!(slow.level, Level::Normal);
        let offline = History {
            samples: vec![sample(now - Duration::from_secs(4 * DAY), 0.0, Some(reset))],
        };
        let fast = offline.weekly(&section(80.0, Some(reset)), now);
        assert_eq!(fast.recent_per_day, Some(20.0));
        assert_eq!(fast.budget_per_day, Some(10.0));
        assert_eq!(fast.level, Level::Warning);
        assert_eq!(prediction_level(Some(10.0), Some(11.0)), Level::Caution);
    }
}
