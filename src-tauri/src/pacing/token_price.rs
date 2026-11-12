//! What a model's tokens cost relative to the cheapest tier's - the port of
//! `priceRatio` from `lib/usage/pacing/token-price.ts`. Anthropic's list prices
//! (late 2026) per million input tokens, with Haiku priced at its rate above
//! 100k-token prompts since agent sessions run long contexts.

use crate::pacing::model_cost::CostTier;

/// Dollars per million input tokens for a tier.
const fn input_price(tier: CostTier) -> f64 {
    match tier {
        CostTier::Haiku => 0.5,
        CostTier::Sonnet => 2.0,
        CostTier::Opus => 4.0,
        CostTier::Fable => 10.0,
    }
}

const CHEAPEST_TIER_PRICE: f64 = input_price(CostTier::Haiku);
/// A model no tier places is priced as Sonnet, the middle of the range.
const UNKNOWN_MODEL_PRICE: f64 = input_price(CostTier::Sonnet);

/// How much a model's tokens cost relative to the cheapest tier's.
pub fn price_ratio(model: &str) -> f64 {
    let price = CostTier::of_model(model).map_or(UNKNOWN_MODEL_PRICE, input_price);
    price / CHEAPEST_TIER_PRICE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_each_tier_by_its_long_context_list_price_haiku_first() {
        assert!((price_ratio("claude-haiku-5-5") - 1.0).abs() < f64::EPSILON);
        assert!((price_ratio("claude-sonnet-5-5") - 4.0).abs() < f64::EPSILON);
        assert!((price_ratio("claude-opus-5-5[1m]") - 8.0).abs() < f64::EPSILON);
        assert!((price_ratio("claude-fable-5-1") - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn prices_a_model_no_tier_places_as_sonnet() {
        assert!(
            (price_ratio("mystery-model") - price_ratio("claude-sonnet-5-5")).abs() < f64::EPSILON
        );
    }
}
