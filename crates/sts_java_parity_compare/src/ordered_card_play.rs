use super::*;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use sts_core::content::cards::{get_card_definition, CardType};

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaParityOrderedCardPlayComparisonReportV1 {
    pub schema: String,
    pub status: String,
    pub scope: String,
    pub reconstruction: String,
    pub evidence_scope: LiveComparisonEvidenceScopeV1,
    pub source: LiveComparisonSourceV1,
    pub transition: ComparedUntargetedCardTransitionV1,
    pub original_game: OrderedCardPlayProjectionV1,
    pub simulator: OrderedCardPlayProjectionV1,
    pub claims: OrderedCardPlayClaimsV1,
    pub not_claimed: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparedUntargetedCardTransitionV1 {
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub action_id: String,
    pub java_instance_uuid: String,
    pub java_card_id: String,
    pub upgrades: u8,
    pub cost_for_turn: i32,
    pub action_parameters_empty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrderedCardPlayProjectionV1 {
    pub turn_before: u32,
    pub turn_after: u32,
    pub energy_before: i32,
    pub energy_after: i32,
    pub player_hp_before: i32,
    pub player_hp_after: i32,
    pub player_max_hp_before: i32,
    pub player_max_hp_after: i32,
    pub player_block_before: i32,
    pub player_block_after: i32,
    pub hand_before: Vec<String>,
    pub hand_after: Vec<String>,
    pub draw_before: Vec<String>,
    pub draw_after: Vec<String>,
    pub discard_before: Vec<String>,
    pub discard_after: Vec<String>,
    pub exhaust_before: Vec<String>,
    pub exhaust_after: Vec<String>,
    pub limbo_before: Vec<String>,
    pub limbo_after: Vec<String>,
    pub generated_cards: Vec<String>,
    pub newly_exhausted_cards: Vec<String>,
    pub moved_card_reached_discard: bool,
    pub monsters_unchanged: bool,
    pub six_rng_streams_unchanged: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrderedCardPlayClaimsV1 {
    pub completed_live_action_identity_verified: bool,
    pub adjacent_stable_transition_verified: bool,
    pub java_owned_untargeted_card_identity_verified: bool,
    pub plain_constructed_context_verified: bool,
    pub successor_card_identity_reconciled_by_typed_birth_witness: bool,
    pub exact_card_zone_order_matches: bool,
    pub energy_and_block_match: bool,
    pub player_hp_and_monsters_match: bool,
    pub unchanged_java_relic_and_potion_context_verified: bool,
    pub all_six_rng_streams_match: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OrderedCardKind {
    PowerThrough,
    SecondWind,
}

impl OrderedCardKind {
    fn from_java_id(card_id: &str) -> Option<Self> {
        match card_id {
            POWER_THROUGH_JAVA_ID => Some(Self::PowerThrough),
            SECOND_WIND_JAVA_ID => Some(Self::SecondWind),
            _ => None,
        }
    }

    fn scope(self) -> &'static str {
        match self {
            Self::PowerThrough => "plain_unupgraded_power_through_status_generation",
            Self::SecondWind => "plain_unupgraded_second_wind_mass_exhaust",
        }
    }

    pub(super) fn comparison_kind(self) -> &'static str {
        match self {
            Self::PowerThrough => "power_through_status_generation",
            Self::SecondWind => "second_wind_mass_exhaust",
        }
    }
}

pub(super) fn ordered_card_comparison_kind(card_id: &str) -> Option<&'static str> {
    OrderedCardKind::from_java_id(card_id).map(OrderedCardKind::comparison_kind)
}

pub(super) fn compare_ordered_card_play_loaded(
    loaded: LoadedLiveTransition,
) -> Result<JavaParityOrderedCardPlayComparisonReportV1, String> {
    let action = receipted_action(&loaded)?;
    let card_id = action
        .card_id
        .as_deref()
        .ok_or_else(|| "ordered card comparison action omitted card ID".to_owned())?;
    let kind = OrderedCardKind::from_java_id(card_id)
        .ok_or_else(|| "ordered card comparison received an unsupported card".to_owned())?;
    let moved = validate_typed_untargeted_action(&loaded, action, kind)?;
    validate_plain_context(&loaded)?;
    validate_original_card_semantics(&loaded, &moved, kind)?;

    let original_game = projection_from_java_frames(&loaded, &moved.instance_uuid);
    let simulator = run_ordered_card_play_transition(&loaded, &moved, kind)?;
    if original_game != simulator {
        return Err("Rust and Java ordered card-play projections disagree".to_owned());
    }

    Ok(JavaParityOrderedCardPlayComparisonReportV1 {
        schema: "sts_java_parity_ordered_card_play_comparison_v1".to_owned(),
        status: "matched".to_owned(),
        scope: kind.scope().to_owned(),
        reconstruction: "plain_combat_with_ordered_successor_card_identity_v1".to_owned(),
        evidence_scope: live_comparison_evidence_scope(&loaded)?,
        source: live_comparison_source(&loaded)?,
        transition: ComparedUntargetedCardTransitionV1 {
            from_frame_id: loaded.before.frame_id,
            to_frame_id: loaded.after.frame_id,
            action_id: loaded.receipt.action_id,
            java_instance_uuid: moved.instance_uuid,
            java_card_id: moved.card_id,
            upgrades: moved.upgrades,
            cost_for_turn: moved.cost_for_turn,
            action_parameters_empty: true,
        },
        original_game,
        simulator,
        claims: OrderedCardPlayClaimsV1 {
            completed_live_action_identity_verified: true,
            adjacent_stable_transition_verified: true,
            java_owned_untargeted_card_identity_verified: true,
            plain_constructed_context_verified: true,
            successor_card_identity_reconciled_by_typed_birth_witness: true,
            exact_card_zone_order_matches: true,
            energy_and_block_match: true,
            player_hp_and_monsters_match: true,
            unchanged_java_relic_and_potion_context_verified: true,
            all_six_rng_streams_match: true,
        },
        not_claimed: vec![
            "natural reachability, strategy quality, or training eligibility follows from this constructed transition".to_owned(),
            "Java UUID values equal simulator lineage values; generated identities are related only by the typed birth witness".to_owned(),
            "upgrades, Dexterity, Frail, Corruption, Feel No Pain, Dark Embrace, Strange Spoon, or other card/relic/power modifiers are covered".to_owned(),
            "the unchanged Burning Blood and potion inventory are not executed by the minimal Rust reconstruction; they are verified Java context only".to_owned(),
            "other generated-card or mass-exhaust actions inherit parity from these two scoped cards".to_owned(),
            "exact Java combat state is restorable as a Rust checkpoint".to_owned(),
        ],
    })
}

fn validate_typed_untargeted_action(
    loaded: &LoadedLiveTransition,
    action: &ActionProjection,
    kind: OrderedCardKind,
) -> Result<CardProjection, String> {
    let card_uuid = action
        .card_uuid
        .as_deref()
        .ok_or_else(|| "typed ordered card play omitted card UUID".to_owned())?;
    let expected_id = match kind {
        OrderedCardKind::PowerThrough => POWER_THROUGH_JAVA_ID,
        OrderedCardKind::SecondWind => SECOND_WIND_JAVA_ID,
    };
    if action.kind != "play_card"
        || action.card_id.as_deref() != Some(expected_id)
        || action.potion_slot.is_some()
        || action.potion_id.is_some()
        || action.potion_potency.is_some()
        || action.target_monster_instance_id.is_some()
        || action.target_monster_index.is_some()
        || action.target_monster_id.is_some()
        || !loaded
            .receipt
            .parameters
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
    {
        return Err("receipted action is not the expected typed untargeted card play".to_owned());
    }
    let moved = loaded
        .before
        .public_base
        .combat
        .hand
        .iter()
        .find(|card| card.instance_uuid == card_uuid)
        .ok_or_else(|| "typed ordered card UUID is absent from predecessor hand".to_owned())?;
    if moved.card_id != expected_id || moved.upgrades != 0 || moved.cost_for_turn != 1 {
        return Err("ordered card comparison requires one ordinary unupgraded card".to_owned());
    }
    Ok(moved.clone())
}

fn validate_plain_context(loaded: &LoadedLiveTransition) -> Result<(), String> {
    let (origin, recipe) = live_transition_origin(loaded)?;
    let recipe = recipe.ok_or_else(|| {
        "ordered card comparison requires a bound constructed scenario".to_owned()
    })?;
    let expected_relics = ["Burning Blood"];
    let before_relics = loaded
        .before
        .parity_internal
        .relics
        .iter()
        .map(|relic| relic.relic_id.as_str())
        .collect::<Vec<_>>();
    if origin != "constructed_validated"
        || recipe.capabilities.len() != 3
        || recipe.capabilities.iter().any(|capability| {
            matches!(
                capability,
                ScenarioCapability::FreshRelics(_) | ScenarioCapability::FreshPotions(_)
            )
        })
        || loaded
            .before
            .parity_internal
            .relics
            .iter()
            .enumerate()
            .any(|(index, relic)| relic.index != index)
        || loaded.before.parity_internal.relics != loaded.after.parity_internal.relics
        || before_relics != expected_relics
        || !loaded.before.parity_internal.player_powers.is_empty()
        || !loaded.after.parity_internal.player_powers.is_empty()
        || loaded.before.public_base.combat.potions != loaded.after.public_base.combat.potions
        || loaded
            .before
            .public_base
            .combat
            .potions
            .iter()
            .any(|potion| !potion.empty)
        || loaded.before.public_base.combat.monsters != loaded.after.public_base.combat.monsters
        || loaded.before.public_base.combat.player.current_hp
            != loaded.after.public_base.combat.player.current_hp
        || loaded.before.public_base.combat.player.max_hp
            != loaded.after.public_base.combat.player.max_hp
        || loaded.before.rng_internal.streams.len() != 6
        || loaded.before.rng_internal.streams != loaded.after.rng_internal.streams
    {
        return Err(
            "ordered card comparison requires one plain modifier-free same-turn context".to_owned(),
        );
    }
    validate_rng_owners(&loaded.before.rng_internal.streams)
}

fn validate_original_card_semantics(
    loaded: &LoadedLiveTransition,
    moved: &CardProjection,
    kind: OrderedCardKind,
) -> Result<(), String> {
    let before = &loaded.before.parity_internal.zones;
    let after = &loaded.after.parity_internal.zones;
    let before_player = &loaded.before.public_base.combat.player;
    let after_player = &loaded.after.public_base.combat.player;
    let mut expected_discard = before.discard.cards.clone();
    expected_discard.push(moved.clone());
    if after.discard.cards != expected_discard
        || after.draw.cards != before.draw.cards
        || after.limbo.cards != before.limbo.cards
        || after_player.energy != before_player.energy - 1
    {
        return Err("ordered card play changed common zones or energy outside scope".to_owned());
    }

    match kind {
        OrderedCardKind::PowerThrough => {
            let remaining = before
                .hand
                .cards
                .iter()
                .filter(|card| card.instance_uuid != moved.instance_uuid)
                .cloned()
                .collect::<Vec<_>>();
            if after.hand.cards.len() != remaining.len() + 2
                || after.hand.cards[..remaining.len()] != remaining
                || after.hand.cards[remaining.len()..].iter().any(|card| {
                    card.card_id != "Wound" || card.upgrades != 0 || card.cost_for_turn != -2
                })
                || after.exhaust.cards != before.exhaust.cards
                || after_player.block != before_player.block + 15
            {
                return Err(
                    "Power Through is not the plain +15 block and two ordered hand Wounds transition"
                        .to_owned(),
                );
            }
            let created = generated_cards(before, &after.as_card_zones());
            if created != ["Wound", "Wound"] {
                return Err("Power Through did not create exactly two fresh Wounds".to_owned());
            }
        }
        OrderedCardKind::SecondWind => {
            let remaining = before
                .hand
                .cards
                .iter()
                .filter(|card| card.instance_uuid != moved.instance_uuid)
                .cloned()
                .collect::<Vec<_>>();
            let mut attacks = Vec::new();
            let mut non_attacks = BTreeMap::new();
            for card in remaining {
                if is_attack(&card)? {
                    attacks.push(card);
                } else {
                    non_attacks.insert(card.instance_uuid.clone(), card);
                }
            }
            let new_exhaust = after
                .exhaust
                .cards
                .get(before.exhaust.cards.len()..)
                .ok_or_else(|| {
                    "Second Wind removed cards from the prior exhaust pile".to_owned()
                })?;
            let exhausted_by_uuid = new_exhaust
                .iter()
                .map(|card| (card.instance_uuid.clone(), card.clone()))
                .collect::<BTreeMap<_, _>>();
            if non_attacks.is_empty()
                || after.hand.cards != attacks
                || !after.exhaust.cards.starts_with(&before.exhaust.cards)
                || exhausted_by_uuid != non_attacks
                || !generated_cards(before, &after.as_card_zones()).is_empty()
                || after_player.block
                    != before_player.block
                        + 5 * i32::try_from(non_attacks.len())
                            .map_err(|_| "Second Wind exhaust count does not fit i32".to_owned())?
            {
                return Err(
                    "Second Wind is not the plain exhaust-all-non-attacks and +5 block each transition"
                        .to_owned(),
                );
            }
        }
    }
    Ok(())
}

fn is_attack(card: &CardProjection) -> Result<bool, String> {
    let card_id = *build_java_id_map()
        .get(card.card_id.as_str())
        .ok_or_else(|| format!("simulator has no Java card ID mapping for {}", card.card_id))?;
    Ok(get_card_definition(card_id).card_type == CardType::Attack)
}

fn run_ordered_card_play_transition(
    loaded: &LoadedLiveTransition,
    moved: &CardProjection,
    _kind: OrderedCardKind,
) -> Result<OrderedCardPlayProjectionV1, String> {
    let (combat, mut identities) = reconstruct_plain_card_combat(&loaded.before)?;
    let selected_rust_lineage = identities
        .rust_lineage(moved.instance_uuid.as_str())
        .ok_or_else(|| "ordered card UUID is absent from reconstructed combat".to_owned())?;
    let card_index = combat
        .zones
        .hand
        .iter()
        .position(|card| card.lineage == selected_rust_lineage)
        .ok_or_else(|| "ordered card is absent from reconstructed hand".to_owned())?;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let input = ClientInput::PlayCard {
        card_index,
        target: None,
    };
    let stepper = EngineCombatStepper;
    if !stepper.is_legal_action(&position, &input) {
        return Err("reconstructed ordered card play is not legal in the Rust stepper".to_owned());
    }
    let observed = sts_core::sim::combat::apply_combat_input_to_stable_with_card_births_v1(
        &position,
        input,
        CombatStepLimits {
            max_engine_steps: MAX_ENGINE_STEPS,
            deadline: None,
        },
        sts_core::runtime::combat::CombatCardBirthCause::CardPlay {
            producer_lineage: selected_rust_lineage,
        },
    );
    let step = observed.step;
    if step.truncated || step.timed_out {
        return Err("Rust ordered card play did not reach a finite stable boundary".to_owned());
    }
    let reached = &step.position.combat;
    card_birth::bind_card_birth_witness(
        &mut identities,
        &loaded.before,
        &loaded.after,
        &loaded.receipt,
        &observed.card_births,
    )?;
    let projected =
        identities.bind_complete_successor_zones(reached, &loaded.after.parity_internal.zones)?;
    let after = &loaded.after.public_base.combat;
    let monsters_match = reached.entities.monsters.len() == after.monsters.len()
        && reached
            .entities
            .monsters
            .iter()
            .zip(&after.monsters)
            .all(|(rust, java)| {
                rust.current_hp == java.current_hp
                    && rust.max_hp == java.max_hp
                    && rust.block == java.block
            });
    let rust_rng = project_rng_streams(&reached.rng.pool);
    if reached.turn.turn_count != after.turn
        || reached.turn.energy as i32 != after.player.energy
        || reached.entities.player.current_hp != after.player.current_hp
        || reached.entities.player.max_hp != after.player.max_hp
        || reached.entities.player.block != after.player.block
        || !monsters_match
        || rust_rng != loaded.after.rng_internal.streams
    {
        return Err(
            "Rust ordered card successor does not match captured player/monster/RNG state"
                .to_owned(),
        );
    }
    Ok(projection_from_parts(
        &loaded.before,
        &projected,
        reached.turn.turn_count,
        reached.turn.energy as i32,
        reached.entities.player.current_hp,
        reached.entities.player.max_hp,
        reached.entities.player.block,
        &moved.instance_uuid,
        reached.entities.monsters == position.combat.entities.monsters,
        rust_rng == loaded.before.rng_internal.streams,
    ))
}

fn projection_from_java_frames(
    loaded: &LoadedLiveTransition,
    moved_uuid: &str,
) -> OrderedCardPlayProjectionV1 {
    let after = &loaded.after.public_base.combat;
    projection_from_parts(
        &loaded.before,
        &loaded.after.parity_internal.zones.as_card_zones(),
        after.turn,
        after.player.energy,
        after.player.current_hp,
        after.player.max_hp,
        after.player.block,
        moved_uuid,
        loaded.before.public_base.combat.monsters == after.monsters,
        loaded.before.rng_internal.streams == loaded.after.rng_internal.streams,
    )
}

#[allow(clippy::too_many_arguments)]
fn projection_from_parts(
    before: &StateFrameProjection,
    after_zones: &ComparableZones,
    turn_after: u32,
    energy_after: i32,
    hp_after: i32,
    max_hp_after: i32,
    block_after: i32,
    moved_uuid: &str,
    monsters_unchanged: bool,
    rng_unchanged: bool,
) -> OrderedCardPlayProjectionV1 {
    let before_zones = before.parity_internal.zones.as_card_zones();
    OrderedCardPlayProjectionV1 {
        turn_before: before.public_base.combat.turn,
        turn_after,
        energy_before: before.public_base.combat.player.energy,
        energy_after,
        player_hp_before: before.public_base.combat.player.current_hp,
        player_hp_after: hp_after,
        player_max_hp_before: before.public_base.combat.player.max_hp,
        player_max_hp_after: max_hp_after,
        player_block_before: before.public_base.combat.player.block,
        player_block_after: block_after,
        hand_before: card_ids(&before_zones.hand),
        hand_after: card_ids(&after_zones.hand),
        draw_before: card_ids(&before_zones.draw),
        draw_after: card_ids(&after_zones.draw),
        discard_before: card_ids(&before_zones.discard),
        discard_after: card_ids(&after_zones.discard),
        exhaust_before: card_ids(&before_zones.exhaust),
        exhaust_after: card_ids(&after_zones.exhaust),
        limbo_before: card_ids(&before_zones.limbo),
        limbo_after: card_ids(&after_zones.limbo),
        generated_cards: generated_cards(&before.parity_internal.zones, after_zones),
        newly_exhausted_cards: newly_exhausted_cards(&before_zones.exhaust, &after_zones.exhaust),
        moved_card_reached_discard: after_zones
            .discard
            .iter()
            .any(|card| card.instance_uuid == moved_uuid),
        monsters_unchanged,
        six_rng_streams_unchanged: rng_unchanged,
    }
}

fn card_ids(cards: &[CardProjection]) -> Vec<String> {
    cards.iter().map(|card| card.card_id.clone()).collect()
}

fn generated_cards(before: &ParityZonesProjection, after: &ComparableZones) -> Vec<String> {
    let known = all_parity_cards(before)
        .into_iter()
        .map(|card| card.instance_uuid.as_str())
        .collect::<BTreeSet<_>>();
    all_comparable_cards(after)
        .into_iter()
        .filter(|card| !known.contains(card.instance_uuid.as_str()))
        .map(|card| card.card_id.clone())
        .collect()
}

fn newly_exhausted_cards(before: &[CardProjection], after: &[CardProjection]) -> Vec<String> {
    let known = before
        .iter()
        .map(|card| card.instance_uuid.as_str())
        .collect::<BTreeSet<_>>();
    after
        .iter()
        .filter(|card| !known.contains(card.instance_uuid.as_str()))
        .map(|card| card.card_id.clone())
        .collect()
}

fn all_parity_cards(zones: &ParityZonesProjection) -> Vec<&CardProjection> {
    zones
        .draw
        .cards
        .iter()
        .chain(&zones.hand.cards)
        .chain(&zones.discard.cards)
        .chain(&zones.exhaust.cards)
        .chain(&zones.limbo.cards)
        .collect()
}

fn all_comparable_cards(zones: &ComparableZones) -> Vec<&CardProjection> {
    zones
        .draw
        .iter()
        .chain(&zones.hand)
        .chain(&zones.discard)
        .chain(&zones.exhaust)
        .chain(&zones.limbo)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::cards::CardId;

    fn card(uuid: &str, card_id: &str, cost: i32) -> CardProjection {
        CardProjection {
            instance_uuid: uuid.to_owned(),
            card_id: card_id.to_owned(),
            upgrades: 0,
            cost_for_turn: cost,
        }
    }

    fn zone(cards: Vec<CardProjection>) -> OrderedZoneProjection {
        OrderedZoneProjection {
            ordering: "java_group_index_ascending".to_owned(),
            top: "last".to_owned(),
            cards,
        }
    }

    #[test]
    fn successor_identity_binder_allows_only_symmetric_card_removal() {
        let before = ParityZonesProjection {
            draw: zone(Vec::new()),
            discard: zone(Vec::new()),
            exhaust: zone(Vec::new()),
            hand: zone(vec![
                card("power", CORRUPTION_JAVA_ID, 3),
                card("strike", BASE_STRIKE_JAVA_ID, 1),
            ]),
            limbo: zone(Vec::new()),
        };
        let expected = ParityZonesProjection {
            draw: zone(Vec::new()),
            discard: zone(Vec::new()),
            exhaust: zone(Vec::new()),
            hand: zone(vec![card("strike", BASE_STRIKE_JAVA_ID, 1)]),
            limbo: zone(Vec::new()),
        };
        let mut combat = sts_core::test_support::blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 2)];
        let mut identities = compile_captured_scenario_bindings(&before).unwrap();
        assert_eq!(
            identities
                .bind_complete_successor_zones(&combat, &expected)
                .unwrap(),
            expected.as_card_zones()
        );

        let mut java_retains_removed = expected;
        java_retains_removed
            .discard
            .cards
            .push(card("power", CORRUPTION_JAVA_ID, 3));
        let mut identities = compile_captured_scenario_bindings(&before).unwrap();
        assert!(identities
            .bind_complete_successor_zones(&combat, &java_retains_removed)
            .is_err());
    }
}
