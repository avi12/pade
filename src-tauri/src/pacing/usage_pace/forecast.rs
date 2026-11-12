//! A paced window's live forecast for the Config panel: when it runs out at its
//! pace, when it would be back on pace, and whether that is over pace now, by the
//! same rule the pacing loop acts on.

use serde::{Deserialize, Serialize};

use super::{
    even_spend_percentage, is_too_far_ahead, on_pace_time, reset_time_of, runway_of,
    utilization_of, OnPaceParams, PacedWindowKind, RateOf, TooFarAheadParams, MINIMUM_RATE_SPAN_MS,
    PACE_FLOOR_PERCENTAGE, RESET_GRACE_MS, WEEKLY_WINDOW_MS,
};
use crate::pacing::units::{EpochMilliseconds, Percentage, SpanMilliseconds};
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
    /// When it would be back on pace if its agents stopped now.
    pub on_pace_at: Option<EpochMilliseconds>,
    /// A weekly window's even spend by now - `None` for a session.
    pub even_percentage: Option<Percentage>,
}

#[derive(Clone, Copy)]
pub struct ForecastOverPaceParams<'a> {
    pub forecast: &'a PaceForecast,
    pub margin: SpanMilliseconds,
    pub now: EpochMilliseconds,
}

/// Whether a forecast is over pace now - what the Config panel's live line asks,
/// by the same rule the pacing loop acts on.
#[must_use]
pub fn is_forecast_over_pace(params: ForecastOverPaceParams<'_>) -> bool {
    let ForecastOverPaceParams {
        forecast,
        margin,
        now,
    } = params;
    let free_at = forecast.reset_at - RESET_GRACE_MS;
    let is_paced_now = utilization_of(&forecast.window) >= PACE_FLOOR_PERCENTAGE && free_at > now;
    let Some(on_pace_at) = forecast.on_pace_at else {
        return false;
    };

    is_paced_now
        && is_too_far_ahead(TooFarAheadParams {
            on_pace_at,
            free_at,
            margin,
            now,
        })
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
}

/// Every paced window's forecast, by kind - for the Config panel's live lines.
/// A session window's comes from its measured burn; a weekly window's from its
/// average spend since it started. The first window of each kind counts.
#[must_use]
pub fn pace_forecasts(params: PaceForecastsParams<'_>) -> PaceForecasts {
    let PaceForecastsParams {
        account,
        now,
        rate_of,
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
}

fn forecast_of(params: ForecastParams<'_>) -> PaceForecast {
    let ForecastParams {
        kind,
        window,
        reset_at,
        now,
        rate_of,
    } = params;
    let free_at = reset_at - RESET_GRACE_MS;
    let rate = match kind {
        PacedWindowKind::Weekly => {
            let elapsed = (now - (reset_at - WEEKLY_WINDOW_MS)).longer(MINIMUM_RATE_SPAN_MS);
            Some(utilization_of(window) / elapsed)
        }
        PacedWindowKind::Session => rate_of(window),
    };
    let even_percentage = match kind {
        PacedWindowKind::Weekly => Some(even_spend_percentage(reset_at, now)),
        PacedWindowKind::Session => None,
    };
    PaceForecast {
        window: window.clone(),
        reset_at,
        run_out_at: rate
            .filter(|rate| rate.is_positive())
            .map(|rate| now + runway_of(window, rate)),
        is_measured: rate.is_some(),
        on_pace_at: on_pace_time(OnPaceParams {
            kind,
            window,
            free_at,
            rate,
        }),
        even_percentage,
    }
}
