//! Mirrors `packages/agent/src/harness/compaction/compaction.ts:246-250`
//! (`shouldCompact`) and the `CompactionSettings`/`DEFAULT_COMPACTION_SETTINGS`
//! re-export.
//!
//! The struct + constant themselves live in [`crate::types`] (declared there so
//! `AgentHarnessOptions`, which predates this module, can carry the field
//! without a forward dependency). This module owns only the threshold check and
//! re-exports the struct for a stable `evo_harness::compaction::settings` path.

// Re-export so `super::mod` can surface them as `compaction::CompactionSettings`.
pub use crate::types::{CompactionSettings, DEFAULT_COMPACTION_SETTINGS};

/// The lowest share of the window a trigger percent may be set to.
pub const MIN_TRIGGER_PERCENT: u8 = 30;
/// The highest share of the window a trigger percent may be set to.
pub const MAX_TRIGGER_PERCENT: u8 = 95;

impl CompactionSettings {
    /// The context size (tokens) above which a chat compacts by itself, for a
    /// model with `window` tokens of context.
    ///
    /// With a `trigger_percent`: that share of the window, but never under
    /// twice `keep_recent_tokens` (a compaction keeps that much, so it would
    /// free nothing) and never over the window. Without one: the original
    /// rule, the window minus `reserve_tokens`.
    pub fn threshold(&self, window: i64) -> i64 {
        match self.trigger_percent {
            Some(percent) => {
                let percent = i64::from(percent.clamp(MIN_TRIGGER_PERCENT, MAX_TRIGGER_PERCENT));
                (window * percent / 100)
                    .max(self.keep_recent_tokens.saturating_mul(2))
                    .min(window)
            }
            None => window - self.reserve_tokens,
        }
    }
}

/// Return whether context usage exceeds the configured compaction threshold.
/// Mirrors TS `shouldCompact(contextTokens, contextWindow, settings)`:
/// `if (!settings.enabled) return false; return contextTokens > contextWindow -
/// settings.reserveTokens;`.
///
/// `context_tokens` and `context_window` are `i64` (token counts). The
/// subtraction is plain (NOT saturating): when `reserve_tokens > context_window`
/// the threshold goes negative, so ANY `context_tokens >= 0` trips compaction —
/// exactly the TS behavior. Realistic token counts (millions) cannot overflow
/// `i64`, so plain subtraction is safe.
pub fn should_compact(
    context_tokens: i64,
    context_window: i64,
    settings: &CompactionSettings,
) -> bool {
    if !settings.enabled {
        return false;
    }
    context_tokens > settings.threshold(context_window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_never_compacts() {
        let s = CompactionSettings {
            enabled: false,
            reserve_tokens: 1000,
            keep_recent_tokens: 500,
            trigger_percent: None,
        };
        assert!(!should_compact(9_999_999, 10_000, &s));
    }

    #[test]
    fn trips_above_threshold() {
        let s = CompactionSettings {
            enabled: true,
            reserve_tokens: 1000,
            keep_recent_tokens: 500,
            trigger_percent: None,
        };
        // window 10000 - reserve 1000 = 9000 threshold.
        assert!(!should_compact(9000, 10000, &s));
        assert!(should_compact(9001, 10000, &s));
        assert!(should_compact(99999, 10000, &s));
    }

    #[test]
    fn reserve_exceeding_window_trips_on_any_usage() {
        // Mirrors TS: threshold = contextWindow - reserve = 10000 - 20000 = -10000,
        // and `contextTokens > -10000` is true for every non-negative context_tokens,
        // so compaction trips even at zero usage.
        let s = CompactionSettings {
            enabled: true,
            reserve_tokens: 20_000,
            keep_recent_tokens: 500,
            trigger_percent: None,
        };
        assert!(should_compact(0, 10_000, &s));
        assert!(should_compact(1, 10_000, &s));
    }

    fn with_percent(percent: Option<u8>, keep_recent_tokens: i64) -> CompactionSettings {
        CompactionSettings {
            enabled: true,
            reserve_tokens: 1000,
            keep_recent_tokens,
            trigger_percent: percent,
        }
    }

    #[test]
    fn a_percent_is_a_share_of_the_window() {
        let s = with_percent(Some(60), 500);
        assert_eq!(s.threshold(200_000), 120_000);
        assert!(!should_compact(120_000, 200_000, &s));
        assert!(should_compact(120_001, 200_000, &s));
    }

    #[test]
    fn without_a_percent_the_threshold_is_window_minus_reserve() {
        assert_eq!(with_percent(None, 500).threshold(10_000), 9_000);
    }

    #[test]
    fn a_percent_outside_the_range_is_clamped() {
        assert_eq!(with_percent(Some(5), 0).threshold(100_000), 30_000);
        assert_eq!(with_percent(Some(100), 0).threshold(100_000), 95_000);
    }

    #[test]
    fn the_threshold_never_drops_under_twice_keep_recent() {
        // 30 % of 32k is 9.6k, under 2 x 20k: a compaction would free nothing.
        assert_eq!(with_percent(Some(30), 20_000).threshold(32_000), 32_000);
        assert_eq!(with_percent(Some(30), 5_000).threshold(32_000), 10_000);
    }

    #[test]
    fn disabled_never_compacts_with_a_percent() {
        let mut s = with_percent(Some(30), 500);
        s.enabled = false;
        assert!(!should_compact(9_999_999, 10_000, &s));
    }

    #[test]
    fn defaults_match_ts() {
        assert_eq!(DEFAULT_COMPACTION_SETTINGS.enabled, true);
        assert_eq!(DEFAULT_COMPACTION_SETTINGS.reserve_tokens, 16384);
        assert_eq!(DEFAULT_COMPACTION_SETTINGS.keep_recent_tokens, 20000);
    }
}
