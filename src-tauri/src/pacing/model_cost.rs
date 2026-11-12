//! Which cost tier a model is in - the port of `costTier` from
//! `lib/model/model-cost.ts`. No catalog states a price, so a model's tier is
//! read from the family word in its id, the one thing every vendor keeps
//! stable. What each tier costs lives in `token_price`.

use crate::pacing::model_label::bare_model;

/// The cost tiers, cheapest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CostTier {
    /// Haiku and the mini, nano, flash and lite lines.
    Haiku,
    Sonnet,
    Opus,
    Fable,
}

impl CostTier {
    /// Cheapest tier first - the order the first matching family word wins in.
    pub const ASCENDING: [Self; 4] = [Self::Haiku, Self::Sonnet, Self::Opus, Self::Fable];

    /// The family words that place a model in this tier.
    const fn family_words(self) -> &'static [&'static str] {
        match self {
            Self::Haiku => &["haiku", "mini", "nano", "flash", "lite"],
            Self::Sonnet => &["sonnet"],
            Self::Opus => &["opus"],
            Self::Fable => &["fable"],
        }
    }

    /// A model's cost tier, or `None` when no family word places it.
    #[must_use]
    pub fn of_model(model: &str) -> Option<Self> {
        let id = bare_model(model).to_lowercase();
        Self::ASCENDING
            .into_iter()
            .find(|tier| tier.family_words().iter().any(|word| id.contains(word)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_a_model_by_its_family_word() {
        assert_eq!(
            CostTier::of_model("claude-haiku-5-5"),
            Some(CostTier::Haiku)
        );
        assert_eq!(CostTier::of_model("gpt-5-mini"), Some(CostTier::Haiku));
        assert_eq!(
            CostTier::of_model("claude-sonnet-5-5"),
            Some(CostTier::Sonnet)
        );
        assert_eq!(
            CostTier::of_model("claude-opus-5-5[1m]"),
            Some(CostTier::Opus)
        );
        assert_eq!(
            CostTier::of_model("anthropic/Claude-Fable-5-1"),
            Some(CostTier::Fable)
        );
    }

    #[test]
    fn leaves_an_unknown_model_unplaced() {
        assert_eq!(CostTier::of_model("mystery-model"), None);
    }

    #[test]
    fn tiers_order_from_cheapest_to_dearest() {
        assert!(CostTier::Haiku < CostTier::Fable);
    }
}
