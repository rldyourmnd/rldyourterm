// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

pub(crate) const DEFAULT_REFRESH_RATE_MILLIHZ: u32 = 60_000;

pub(crate) fn sanitize_refresh_rate_millihz(refresh_rate_millihz: u32) -> u32 {
    match refresh_rate_millihz {
        0 => DEFAULT_REFRESH_RATE_MILLIHZ,
        value => value,
    }
}

pub(crate) fn frame_budget_millis(refresh_rate_millihz: u32) -> u64 {
    let sanitized_refresh_rate = sanitize_refresh_rate_millihz(refresh_rate_millihz);
    let frame_nanos = 1_000_000_000_000_u64 / u64::from(sanitized_refresh_rate);
    let rounded_up_millis = frame_nanos.div_ceil(1_000_000);
    rounded_up_millis.max(1)
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_REFRESH_RATE_MILLIHZ, frame_budget_millis, sanitize_refresh_rate_millihz};

    #[test]
    fn zero_refresh_rate_falls_back_to_default() {
        assert_eq!(
            sanitize_refresh_rate_millihz(0),
            DEFAULT_REFRESH_RATE_MILLIHZ
        );
    }

    #[test]
    fn frame_budget_is_at_least_one_millisecond() {
        assert_eq!(frame_budget_millis(144_000), 7);
        assert_eq!(frame_budget_millis(0), 17);
    }
}
