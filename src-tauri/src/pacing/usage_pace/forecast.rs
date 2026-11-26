//! A paced window's live forecast for the Config panel: when it runs out at its
//! pace, the share of its spend the account may keep, and whether that is over
//! pace now, by the same rule the pacing loop acts on.

use serde::{Deserialize, Serialize};

use crate::pacing::units::{EpochMilliseconds, Fraction, Percentage};
use crate::pacing::usage_pace::{
    paced_window, reset_time_of, utilization_of, PaceCheck, PacedWindowKind, RateOf,
    MINIMUM_RATE_SPAN_MS, SLOW_DOWN_KEEP, WEEKLY_WINDOW_MS,
};
use crate::usage::{AccountUsage, UsageWindow};

/// One paced window's live line for the Config panel (`pace_forecasts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaceForecast {
    pub window: UsageWindow,
    pub reset_at: EpochMilliseconds,
    /// When it runs out at its pace, or `None` while that isn't measured (or
    /// nothing is being spent).
    pub run_out_at: Option<EpochMilliseconds>,
    /// Whether enough working time is behind the pace to judge it.
    pub is_measured: bool,
    /// The share of its spend the account may keep (`usage_pace`), `None` while
    /// its pace isn't measured or the window is about to reset.
    pub keep: Option<Fraction>,
}

/// Whether a forecast is over pace now - what the Config panel's live line asks,
/// by the same rule the pacing loop acts on.
#[must_use]
pub fn is_forecast_over_pace(forecast: &PaceForecast) -> bool {
    forecast.keep.is_some_and(|keep| keep < SLOW_DOWN_KEEP)
}

/// Every paced window's forecast, by kind.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PaceForecasts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<PaceForecast>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly: Option<PaceForecast>,
}

impl PaceForecasts {
    fn slot(&mut self, kind: PacedWindowKind) -> &mut Option<PaceForecast> {
        match kind {
            PacedWindowKind::Session => &mut self.session,
            PacedWindowKind::Weekly => &mut self.weekly,
        }
    }
}

#[derive(Clone, Copy)]
pub struct PaceForecastsParams<'a> {
    pub account: Option<&'a AccountUsage>,
    pub now: EpochMilliseconds,
    pub rate_of: RateOf<'a>,
    /// A window's spend over about the last day, nights included.
    pub daily_rate_of: RateOf<'a>,
}

/// Every paced window's forecast, by kind - for the Config panel's live lines.
/// A session window's comes from its measured burn; a weekly window's from its
/// spend over the last day, or its average since it started without a day of
/// readings. The first window of each kind counts.
#[must_use]
pub fn pace_forecasts(params: PaceForecastsParams<'_>) -> PaceForecasts {
    let PaceForecastsParams {
        account,
        now,
        rate_of,
        daily_rate_of,
    } = params;
    let mut forecasts = PaceForecasts::default();
    let windows = account.map_or(&[][..], |account| account.windows.as_slice());
    for window in windows {
        let Some(kind) = PacedWindowKind::from_window_kind(window.kind) else {
            continue;
        };
        let Some(reset_at) = reset_time_of(window) else {
            continue;
        };

        let slot = forecasts.slot(kind);
        let is_forecastable = slot.is_none() && reset_at > now;
        if !is_forecastable {
            continue;
        }

        *slot = Some(forecast_of(ForecastParams {
            kind,
            window,
            reset_at,
            now,
            rate_of,
            daily_rate_of,
        }));
    }

    forecasts
}

#[derive(Clone, Copy)]
struct ForecastParams<'a> {
    kind: PacedWindowKind,
    window: &'a UsageWindow,
    reset_at: EpochMilliseconds,
    now: EpochMilliseconds,
    rate_of: RateOf<'a>,
    daily_rate_of: RateOf<'a>,
}

/// A session window runs out at its measured burn; a weekly window at its
/// spend over the last day, nights and breaks included - so an early burst
/// pacing has since slowed forecasts no run-out it won't reach - or its
/// average since it started without a day of readings.
fn forecast_of(params: ForecastParams<'_>) -> PaceForecast {
    let ForecastParams {
        kind,
        window,
        reset_at,
        now,
        rate_of,
        daily_rate_of,
    } = params;
    let rate = rate_of(window);
    let run_out_rate = match kind {
        PacedWindowKind::Weekly => daily_rate_of(window).or_else(|| {
            let elapsed = (now - (reset_at - WEEKLY_WINDOW_MS)).longer(MINIMUM_RATE_SPAN_MS);
            Some(utilization_of(window) / elapsed)
        }),
        PacedWindowKind::Session => rate,
    };
    let keep = paced_window(window, now).zip(rate).map(|(paced, rate)| {
        PaceCheck {
            paced,
            window,
            rate,
            now,
        }
        .keep()
    });
    PaceForecast {
        window: window.clone(),
        reset_at,
        run_out_at: run_out_rate
            .filter(|rate| rate.is_positive())
            .map(|rate| now + (Percentage::FULL - utilization_of(window)) / rate),
        is_measured: rate.is_some(),
        keep,
    }
}
