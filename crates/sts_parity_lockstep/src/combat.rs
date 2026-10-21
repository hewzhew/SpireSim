use serde::{Deserialize, Serialize};

/// Shared combat state at a stable player-input boundary.
///
/// Version 4 covers every ordinary player decision in a natural combat,
/// including the distinction between a card's combat-persistent cost and its
/// current-turn cost. It remains intentionally closed: endpoint adapters must
/// reject card mutation or power state that cannot be represented here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum NaturalRunCombatV4 {
    Inactive,
    PlayerDecision { state: NaturalCombatDecisionV4 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatDecisionV4 {
    pub turn: u32,
    pub player: NaturalCombatPlayerV4,
    pub zones: NaturalCombatZonesV4,
    pub player_powers: Vec<NaturalCombatPowerV4>,
    pub monsters: Vec<NaturalCombatMonsterV4>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatPlayerV4 {
    pub current_hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub energy: u8,
    /// Sampled for Defect and Watcher, including empty slots and Neutral.
    /// Older class records retain their explicitly orb/stance-free scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_state: Option<NaturalCombatCharacterStateV4>,
    /// Non-Ironclad natural combat records carry the discard-dependent turn
    /// counter. Their adapters require the sampled value, including zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cards_discarded_this_turn: Option<u32>,
    /// Watcher combat-total Mantra gain, retained after spending Mantra to
    /// enter Divinity and read by Brilliance independently of the current power.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mantra_gained_this_combat: Option<i32>,
    /// Mechanical while Surrounded is active; otherwise only presentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing_left: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatCharacterStateV4 {
    pub stance: String,
    pub max_orbs: u8,
    /// Every slot, ordered from the next evoked orb to the last empty slot.
    pub orbs: Vec<NaturalCombatOrbV4>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatOrbV4 {
    pub orb_id: String,
    pub base_passive_amount: i32,
    pub base_evoke_amount: i32,
    pub passive_amount: i32,
    pub evoke_amount: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatZonesV4 {
    /// Ordered from the next card drawn to the bottom of the pile.
    pub draw_next_first: Vec<NaturalCombatCardV4>,
    /// Ordered exactly as the stable hand action surface.
    pub hand: Vec<NaturalCombatCardV4>,
    /// Ordered from the bottom of the Java CardGroup to its top.
    pub discard_bottom_first: Vec<NaturalCombatCardV4>,
    /// Exhaust-pile members ordered by their stable cross-runtime binding.
    ///
    /// Original-game end-turn Ethereal triggers are visited in a JVM-random
    /// order that is not owned by any seeded game RNG. Base-game rules only
    /// observe exhaust membership (or select a specific card), so that
    /// process-local insertion order is deliberately outside the canonical
    /// deterministic state.
    pub exhaust_by_binding: Vec<NaturalCombatCardV4>,
    /// Ordered by the Java CardGroup index. At a supported pending card-play
    /// choice, also includes the independently observed Java cardInUse object
    /// if it is absent from that group; Rust retains that same card in limbo.
    pub limbo: Vec<NaturalCombatCardV4>,
    /// Cards held by living Stasis owners, outside the ordinary card piles.
    /// Ordered by monster occurrence in the shared boundary, then power order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stasis: Vec<NaturalCombatStasisCardV4>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatStasisCardV4 {
    pub owner: NaturalCombatCreatureV4,
    pub card: NaturalCombatCardV4,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatCardV4 {
    /// Stable cross-runtime binding established from the card's typed source.
    pub binding: String,
    pub card_id: String,
    pub upgrades: u8,
    /// Java `AbstractCard.cost`, after combat-persistent cost mutation.
    pub combat_cost: i32,
    /// Java `AbstractCard.costForTurn`, after temporary turn effects.
    pub cost_for_turn: i32,
    /// Setup's one-play waiver survives drawing without changing either cost.
    /// Already-paid limbo cards have no future waiver to compare.
    #[serde(default, skip_serializing_if = "is_false")]
    pub free_to_play_once: bool,
    /// Combat-persistent damage before Strength and target modifiers. Currently
    /// required for Rampage; other damage mutations remain outside this scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_damage: Option<i32>,
    /// Java AbstractCard.misc for Genetic Algorithm and Ritual Dagger growth.
    /// Permanent-deck growth is independently sampled in MasterDeck.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub misc: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatPowerV4 {
    pub power_id: String,
    pub occurrence: u32,
    pub amount: i32,
    pub state: NaturalCombatPowerStateV4,
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NaturalCombatPowerStateV4 {
    Stateless,
    /// Curl Up has fired, but its queued Block/removal is suspended by a choice.
    CurlUpTriggered,
    /// The complete card relation is in the same boundary's zones.stasis.
    Stasis,
    RoundDuration {
        just_applied: bool,
    },
    TurnDuration {
        just_applied: bool,
    },
    Ritual {
        skip_first: bool,
        player_controlled: bool,
    },
    Flight {
        stored_amount: i32,
    },
    Combust {
        hp_loss: i32,
    },
    EchoForm {
        cards_doubled_this_turn: i32,
    },
    Malleable {
        base_power: i32,
    },
    Invincible {
        max_amount: i32,
    },
    Constricted {
        source: NaturalCombatCreatureV4,
    },
    Poison {
        source: NaturalCombatCreatureV4,
    },
}

/// A creature relation resolved from each endpoint's actual object identity.
/// Runtime entity numbers are never compared between endpoints.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NaturalCombatCreatureV4 {
    Player,
    Monster { enemy_id: String, occurrence: u32 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatMonsterV4 {
    pub enemy_id: String,
    pub occurrence: u32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub dead_or_escaped: bool,
    /// Relative position used by Surrounded's damage calculation. A visible
    /// BackAttack marker cannot substitute for this independently observed fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_of_player: Option<bool>,
    pub next_move_id: u8,
    pub move_history: Vec<u8>,
    pub public_intent: NaturalCombatIntentV4,
    pub powers: Vec<NaturalCombatPowerV4>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatIntentV4 {
    pub evidence: NaturalCombatIntentEvidenceV4,
    pub kind: Option<String>,
    pub damage_per_hit: Option<i32>,
    pub hits: Option<u8>,
    pub hidden_reason: Option<String>,
}

impl NaturalCombatIntentV4 {
    /// A terminal monster has no actionable next intent. Java can retain or
    /// recompute its last visible preview while death animations settle, and
    /// Rust can retain the pre-death preview; neither belongs in parity once
    /// both endpoints agree that the monster is dead or escaped.
    pub fn terminal_monster() -> Self {
        Self {
            evidence: NaturalCombatIntentEvidenceV4::Hidden,
            kind: None,
            damage_per_hit: None,
            hits: None,
            hidden_reason: Some("terminal_monster".to_owned()),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NaturalCombatIntentEvidenceV4 {
    VisibleExact,
    Hidden,
}

/// Applies the one shared canonical order for an unordered exhaust-pile
/// projection. Duplicate bindings are rejected because they would make the
/// cross-runtime card identity relation ambiguous.
pub fn canonicalize_exhaust_by_binding_v4(cards: &mut [NaturalCombatCardV4]) -> Result<(), String> {
    cards.sort_by(|left, right| left.binding.cmp(&right.binding));
    if cards
        .windows(2)
        .any(|pair| pair[0].binding == pair[1].binding)
    {
        return Err("canonical exhaust pile repeats one card binding".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(binding: &str, card_id: &str) -> NaturalCombatCardV4 {
        NaturalCombatCardV4 {
            binding: binding.to_owned(),
            card_id: card_id.to_owned(),
            upgrades: 0,
            combat_cost: -2,
            cost_for_turn: -2,
            free_to_play_once: false,
            base_damage: None,
            misc: None,
        }
    }

    #[test]
    fn exhaust_canonicalization_ignores_only_process_local_group_order() {
        let mut java = vec![card("b", "dazed"), card("a", "ascenders_bane")];
        let mut rust = java.iter().cloned().rev().collect::<Vec<_>>();

        canonicalize_exhaust_by_binding_v4(&mut java).unwrap();
        canonicalize_exhaust_by_binding_v4(&mut rust).unwrap();

        assert_eq!(java, rust);
        assert_eq!(
            java.iter()
                .map(|card| card.binding.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn exhaust_canonicalization_preserves_member_state_and_rejects_identity_aliases() {
        let mut left = vec![card("a", "dazed")];
        let mut right = vec![card("a", "slimed")];
        canonicalize_exhaust_by_binding_v4(&mut left).unwrap();
        canonicalize_exhaust_by_binding_v4(&mut right).unwrap();
        assert_ne!(left, right);

        let mut duplicate = vec![card("a", "dazed"), card("a", "dazed")];
        assert!(canonicalize_exhaust_by_binding_v4(&mut duplicate)
            .unwrap_err()
            .contains("repeats one card binding"));
    }
}
