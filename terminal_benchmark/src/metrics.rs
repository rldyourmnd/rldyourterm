// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IterationStats {
    pub min_nanos: u128,
    pub median_nanos: u128,
    pub p95_nanos: u128,
    pub max_nanos: u128,
    pub mean_nanos: u128,
    pub total_nanos: u128,
}

impl IterationStats {
    pub fn from_durations(durations: &[Duration]) -> Self {
        assert!(
            !durations.is_empty(),
            "iteration stats require at least one sample"
        );
        let mut samples: Vec<u128> = durations.iter().map(Duration::as_nanos).collect();
        samples.sort_unstable();
        let total_nanos = samples.iter().copied().sum::<u128>();
        let mean_nanos = total_nanos / samples.len() as u128;
        Self {
            min_nanos: samples[0],
            median_nanos: percentile_nearest_rank(&samples, 50),
            p95_nanos: percentile_nearest_rank(&samples, 95),
            max_nanos: samples[samples.len() - 1],
            mean_nanos,
            total_nanos,
        }
    }
}

fn percentile_nearest_rank(samples: &[u128], percentile: usize) -> u128 {
    let len = samples.len();
    let rank = ((percentile * len).saturating_add(99)) / 100;
    let index = rank.saturating_sub(1).min(len.saturating_sub(1));
    samples[index]
}

#[cfg(test)]
mod tests {
    use super::IterationStats;
    use std::time::Duration;

    #[test]
    fn stats_compute_expected_percentiles() {
        let durations = [1u64, 2, 3, 4, 5]
            .into_iter()
            .map(Duration::from_nanos)
            .collect::<Vec<_>>();
        let stats = IterationStats::from_durations(&durations);
        assert_eq!(stats.min_nanos, 1);
        assert_eq!(stats.median_nanos, 3);
        assert_eq!(stats.p95_nanos, 5);
        assert_eq!(stats.max_nanos, 5);
        assert_eq!(stats.mean_nanos, 3);
        assert_eq!(stats.total_nanos, 15);
    }
}
