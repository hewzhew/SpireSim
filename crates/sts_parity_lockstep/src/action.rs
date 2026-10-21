use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSurfaceV1 {
    Neow,
    Map,
    Combat,
    CombatReward,
    CardReward,
    CombatDiscovery,
    CombatCodex,
    Event,
    Shop,
    Campfire,
    Treasure,
    TreasureReward,
    BossRelic,
    HandSelect,
    GridSelect,
    DoubleBoss,
    Act3Victory,
    TrueVictory,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SemanticScalarV1 {
    Text(String),
    Integer(i64),
    Boolean(bool),
}

/// A stable choice inside the current shared action space.
///
/// `id` is a game/domain identity, never a Java UUID or a Rust collection
/// index. `occurrence` distinguishes genuinely repeated choices.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticChoiceV1 {
    pub kind: String,
    pub id: String,
    pub occurrence: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, SemanticScalarV1>,
}

impl SemanticChoiceV1 {
    pub fn new(kind: impl Into<String>, id: impl Into<String>, occurrence: u32) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            occurrence,
            attributes: BTreeMap::new(),
        }
    }

    fn validate(&self) -> Result<(), String> {
        validate_token("choice kind", &self.kind)?;
        validate_token("choice ID", &self.id)?;
        if self.attributes.len() > 32 {
            return Err("semantic choice has more than 32 attributes".to_owned());
        }
        for key in self.attributes.keys() {
            validate_token("choice attribute", key)?;
        }
        Ok(())
    }
}

pub fn natural_event_option_v1(
    event_id: &str,
    screen: i64,
    option_index: u32,
) -> Result<SemanticChoiceV1, String> {
    if screen < 0 {
        return Err("event option screen must be non-negative".to_owned());
    }
    let mut choice = SemanticChoiceV1::new("event_option", event_id, option_index);
    choice
        .attributes
        .insert("screen".to_owned(), SemanticScalarV1::Integer(screen));
    choice.validate()?;
    Ok(choice)
}

/// An event-owned offer is a blueprint, not an existing master-deck instance.
/// Its ordinal belongs to the typed offer page; no cross-runtime UUID binding
/// is created until the selected acquisition has actually completed.
pub fn natural_event_card_offer_v1(
    event_id: &str,
    screen: i64,
    offer_index: u32,
    card_id: &str,
    upgrades: u8,
) -> Result<SemanticChoiceV1, String> {
    if event_id != "thelibrary" || screen != 1 || offer_index >= 20 {
        return Err("unsupported event card offer page".to_owned());
    }
    let mut choice = SemanticChoiceV1::new("event_card_offer", event_id, offer_index);
    choice
        .attributes
        .insert("screen".to_owned(), SemanticScalarV1::Integer(screen));
    choice.attributes.insert(
        "card_id".to_owned(),
        SemanticScalarV1::Text(card_id.to_owned()),
    );
    choice.attributes.insert(
        "upgrades".to_owned(),
        SemanticScalarV1::Integer(i64::from(upgrades)),
    );
    choice.validate()?;
    Ok(choice)
}

pub fn natural_campfire_option_v1(id: &str) -> Result<SemanticChoiceV1, String> {
    if !matches!(id, "rest" | "smith" | "recall" | "dig") {
        return Err(format!("unsupported natural campfire option {id:?}"));
    }
    let choice = SemanticChoiceV1::new("campfire_option", id, 0);
    choice.validate()?;
    Ok(choice)
}

/// One source-proven master-deck card on a shared selection surface.
///
/// The stable source binding is the identity. Runtime UUIDs, Rust lineages,
/// collection positions, and localized names deliberately stay in the two
/// endpoint adapters.
pub fn natural_deck_card_choice_v1(
    binding: &str,
    card_id: &str,
    upgrades: u8,
) -> Result<SemanticChoiceV1, String> {
    if !binding.starts_with("natural_run/") {
        return Err("deck-card choice has no natural-run source binding".to_owned());
    }
    let mut choice = SemanticChoiceV1::new("deck_card", binding, 0);
    choice.attributes.insert(
        "card_id".to_owned(),
        SemanticScalarV1::Text(card_id.to_owned()),
    );
    choice.attributes.insert(
        "upgrades".to_owned(),
        SemanticScalarV1::Integer(i64::from(upgrades)),
    );
    choice.validate()?;
    Ok(choice)
}

/// One source-proven combat card on the current hand-selection surface.
///
/// The source binding remains stable across the Java UUID / Rust lineage
/// split.  Zone position and localized display text are deliberately not part
/// of the shared identity.
pub fn natural_hand_card_choice_v1(
    binding: &str,
    card_id: &str,
    upgrades: u8,
) -> Result<SemanticChoiceV1, String> {
    if !binding.starts_with("natural_run/") {
        return Err("hand-card choice has no natural-run source binding".to_owned());
    }
    let mut choice = SemanticChoiceV1::new("hand_card", binding, 0);
    choice.attributes.insert(
        "card_id".to_owned(),
        SemanticScalarV1::Text(card_id.to_owned()),
    );
    choice.attributes.insert(
        "upgrades".to_owned(),
        SemanticScalarV1::Integer(i64::from(upgrades)),
    );
    choice.validate()?;
    Ok(choice)
}

/// A source-proven combat card selected from the draw pile.
pub fn natural_draw_card_choice_v1(
    binding: &str,
    card_id: &str,
    upgrades: u8,
) -> Result<SemanticChoiceV1, String> {
    let mut choice = natural_hand_card_choice_v1(binding, card_id, upgrades)?;
    choice.kind = "draw_card".to_owned();
    choice.validate()?;
    Ok(choice)
}

/// A source-proven combat card selected from the discard pile.
pub fn natural_discard_card_choice_v1(
    binding: &str,
    card_id: &str,
    upgrades: u8,
) -> Result<SemanticChoiceV1, String> {
    let mut choice = natural_hand_card_choice_v1(binding, card_id, upgrades)?;
    choice.kind = "discard_card".to_owned();
    choice.validate()?;
    Ok(choice)
}

/// A source-proven combat card selected from the exhaust pile.
pub fn natural_exhaust_card_choice_v1(
    binding: &str,
    card_id: &str,
    upgrades: u8,
) -> Result<SemanticChoiceV1, String> {
    let mut choice = natural_hand_card_choice_v1(binding, card_id, upgrades)?;
    choice.kind = "exhaust_card".to_owned();
    choice.validate()?;
    Ok(choice)
}

/// Enumerate the canonical, source-order-preserving selections exposed by an
/// atomic selection surface.
///
/// Java accepts UUIDs in caller order, but materializing every permutation of
/// a ten-card hand would turn one parameterized UI action into millions of
/// lockstep actions. The shared control protocol deliberately authorizes the
/// deterministic subsequence for each selected set. This retains every card
/// set while bounding a combat-hand surface at 2^10 actions.
pub fn canonical_selection_indices_v1(
    item_count: usize,
    allowed_counts: impl IntoIterator<Item = usize>,
) -> Result<Vec<Vec<usize>>, String> {
    if item_count > 10 {
        return Err("selection surface contains more than ten items".to_owned());
    }
    let allowed_counts = allowed_counts.into_iter().collect::<Vec<_>>();
    if allowed_counts.is_empty()
        || allowed_counts.iter().any(|count| *count > item_count)
        || !allowed_counts.windows(2).all(|pair| pair[0] < pair[1])
        || allowed_counts
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            != allowed_counts.len()
    {
        return Err("selection surface has invalid allowed counts".to_owned());
    }

    fn append_combinations(
        item_count: usize,
        remaining: usize,
        next: usize,
        selected: &mut Vec<usize>,
        output: &mut Vec<Vec<usize>>,
    ) {
        if remaining == 0 {
            output.push(selected.clone());
            return;
        }
        let last_start = item_count - remaining;
        for index in next..=last_start {
            selected.push(index);
            append_combinations(item_count, remaining - 1, index + 1, selected, output);
            selected.pop();
        }
    }

    let mut output = Vec::new();
    let mut selected = Vec::new();
    for count in allowed_counts {
        append_combinations(item_count, count, 0, &mut selected, &mut output);
    }
    Ok(output)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticActionV1 {
    Select {
        surface: DecisionSurfaceV1,
        choice: SemanticChoiceV1,
    },
    SubmitSelection {
        surface: DecisionSurfaceV1,
        choices: Vec<SemanticChoiceV1>,
    },
    PlayCard {
        card_binding: String,
        card_id: String,
        target: Option<SemanticChoiceV1>,
    },
    UsePotion {
        slot: u8,
        potion_id: String,
        target: Option<SemanticChoiceV1>,
    },
    DiscardPotion {
        slot: u8,
        potion_id: String,
    },
    EndTurn,
    Proceed {
        surface: DecisionSurfaceV1,
    },
    Cancel {
        surface: DecisionSurfaceV1,
    },
}

impl SemanticActionV1 {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Select { choice, .. } => choice.validate(),
            Self::SubmitSelection { choices, .. } => {
                for choice in choices {
                    choice.validate()?;
                }
                Ok(())
            }
            Self::PlayCard {
                card_binding,
                card_id,
                target,
            } => {
                validate_token("card binding", card_binding)?;
                validate_token("card ID", card_id)?;
                if let Some(target) = target {
                    target.validate()?;
                }
                Ok(())
            }
            Self::UsePotion {
                potion_id, target, ..
            } => {
                validate_token("potion ID", potion_id)?;
                if let Some(target) = target {
                    target.validate()?;
                }
                Ok(())
            }
            Self::DiscardPotion { potion_id, .. } => validate_token("potion ID", potion_id),
            Self::EndTurn | Self::Proceed { .. } | Self::Cancel { .. } => Ok(()),
        }
    }

    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Select { .. } => "select",
            Self::SubmitSelection { .. } => "submit_selection",
            Self::PlayCard { .. } => "play_card",
            Self::UsePotion { .. } => "use_potion",
            Self::DiscardPotion { .. } => "discard_potion",
            Self::EndTurn => "end_turn",
            Self::Proceed { .. } => "proceed",
            Self::Cancel { .. } => "cancel",
        }
    }
}

fn validate_token(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(format!("{label} is not a valid semantic token"));
    }
    Ok(())
}
