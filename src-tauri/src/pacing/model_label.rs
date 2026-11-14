//! The two reads pacing makes of a model id and an effort level - the port of
//! `bareModel` and `effortStep` from `lib/model/model-label.ts`. Nothing here
//! names a model for a person; that stays with the frontend.

const CLAUDE_PREFIX: &str = "claude-";
const PROVIDER_SEPARATOR: char = '/';

/// Each model family's effort levels, lowest first - the ladder its bars climb.
const CLAUDE_EFFORT_LADDER: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
const OPENAI_EFFORT_LADDER: [&str; 5] = ["minimal", "low", "medium", "high", "xhigh"];

/// Where an effort level sits on its ladder: `step` of `steps`, 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffortStep {
    pub step: u32,
    pub steps: u32,
}

/// The model whose effort level is being placed. `ladder` is the model's own
/// levels from its agent's catalog, when known - it wins; otherwise the
/// family's usual ladder stands in.
#[derive(Debug, Clone, Copy)]
pub struct EffortQuery<'a> {
    pub model: &'a str,
    pub effort: &'a str,
    pub ladder: &'a [String],
}

/// The id without a `provider/` prefix or a trailing `[1m]` context variant -
/// the form the agents' catalogs list.
#[must_use]
pub fn bare_model(model: &str) -> &str {
    let start = model
        .rfind(PROVIDER_SEPARATOR)
        .map_or(0, |position| position + 1);
    let unprefixed = &model[start..];
    strip_context_variant(unprefixed)
}

/// Drops a trailing `[...]` with at least one character inside, as the
/// TypeScript regex `\[([^\]]+)\]$` does: the leftmost `[` with no `]` between
/// it and the final one.
fn strip_context_variant(id: &str) -> &str {
    let Some(body) = id.strip_suffix(']') else {
        return id;
    };

    let tail_start = body.rfind(']').map_or(0, |position| position + 1);
    let Some(open) = body[tail_start..]
        .find('[')
        .map(|position| tail_start + position)
    else {
        return id;
    };

    let has_content = open + 1 < body.len();
    if has_content {
        &id[..open]
    } else {
        id
    }
}

/// An effort level's place on its model's ladder, or `None` for a level no
/// ladder knows.
#[must_use]
pub fn effort_step(query: EffortQuery<'_>) -> Option<EffortStep> {
    let (ladder_length, index) = effort_on_ladder(query)?;
    Some(EffortStep {
        step: u32::try_from(index).ok()? + 1,
        steps: u32::try_from(ladder_length).ok()?,
    })
}

/// The ladder an effort level sits on (its length) and where: the model's own
/// ladder when known, else its family's - Claude's first for a Claude model,
/// `OpenAI`'s first for any other.
fn effort_on_ladder(query: EffortQuery<'_>) -> Option<(usize, usize)> {
    let EffortQuery {
        model,
        effort,
        ladder,
    } = query;
    let is_claude_model = bare_model(model).starts_with(CLAUDE_PREFIX);
    let family_ladders: [&[&str]; 2] = if is_claude_model {
        [&CLAUDE_EFFORT_LADDER, &OPENAI_EFFORT_LADDER]
    } else {
        [&OPENAI_EFFORT_LADDER, &CLAUDE_EFFORT_LADDER]
    };
    let level = effort.to_lowercase();

    let own: Vec<&str> = ladder.iter().map(String::as_str).collect();
    let has_own_ladder = !own.is_empty();
    let candidates = std::iter::once(own.as_slice())
        .filter(|_| has_own_ladder)
        .chain(family_ladders);
    for candidate in candidates {
        if let Some(index) = candidate.iter().position(|known| *known == level) {
            return Some((candidate.len(), index));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(model: &str, effort: &str, ladder: &[&str]) -> Option<EffortStep> {
        let ladder: Vec<String> = ladder.iter().map(ToString::to_string).collect();
        effort_step(EffortQuery {
            model,
            effort,
            ladder: &ladder,
        })
    }

    #[test]
    fn bare_model_drops_the_provider_and_the_context_variant() {
        assert_eq!(bare_model("claude-opus-5-5[1m]"), "claude-opus-5-5");
        assert_eq!(bare_model("anthropic/claude-opus-5"), "claude-opus-5");
        assert_eq!(bare_model("gpt-5.6-sol"), "gpt-5.6-sol");
        assert_eq!(bare_model("odd[]"), "odd[]");
        assert_eq!(bare_model("odd[a]b[1m]"), "odd[a]b");
    }

    #[test]
    fn a_claude_model_reads_the_claude_ladder_first() {
        assert_eq!(
            step("claude-opus-5-5", "max", &[]),
            Some(EffortStep { step: 5, steps: 5 })
        );
        assert_eq!(
            step("claude-opus-5-5", "minimal", &[]),
            Some(EffortStep { step: 1, steps: 5 })
        );
    }

    #[test]
    fn another_model_reads_the_openai_ladder_first() {
        assert_eq!(
            step("gpt-5.6", "low", &[]),
            Some(EffortStep { step: 2, steps: 5 })
        );
    }

    #[test]
    fn the_models_own_ladder_wins() {
        assert_eq!(
            step("claude-opus-5-5", "high", &["low", "high"]),
            Some(EffortStep { step: 2, steps: 2 })
        );
    }

    #[test]
    fn an_effort_no_ladder_knows_has_no_step() {
        assert_eq!(step("claude-opus-5-5", "turbo", &[]), None);
    }
}
