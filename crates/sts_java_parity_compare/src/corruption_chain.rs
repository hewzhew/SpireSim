use super::*;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use sts_core::content::cards::{get_card_definition, CardType};

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaParityCorruptionChainComparisonReportV1 {
    pub schema: String,
    pub status: String,
    pub scope: String,
    pub reconstruction: String,
    pub evidence_scope: LiveComparisonEvidenceScopeV1,
    pub source: LiveComparisonSourceV1,
    pub transition: ComparedUntargetedCardTransitionV1,
    pub original_game: CorruptionChainProjectionV1,
    pub simulator: CorruptionChainProjectionV1,
    pub claims: CorruptionChainClaimsV1,
    pub not_claimed: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorruptionChainProjectionV1 {
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
    pub zones_before: CorruptionChainZonesV1,
    pub zones_after: CorruptionChainZonesV1,
    pub player_powers_before: Vec<CorruptionChainPowerV1>,
    pub player_powers_after: Vec<CorruptionChainPowerV1>,
    pub generated_cards: Vec<String>,
    pub newly_exhausted_cards: Vec<String>,
    pub selected_card_destination: String,
    pub monsters_unchanged: bool,
    pub six_rng_streams_unchanged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorruptionChainZonesV1 {
    pub draw: Vec<CorruptionChainCardV1>,
    pub discard: Vec<CorruptionChainCardV1>,
    pub exhaust: Vec<CorruptionChainCardV1>,
    pub hand: Vec<CorruptionChainCardV1>,
    pub limbo: Vec<CorruptionChainCardV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorruptionChainCardV1 {
    pub card_id: String,
    pub upgrades: u8,
    pub cost_for_turn: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorruptionChainPowerV1 {
    pub power_id: String,
    pub amount: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorruptionChainClaimsV1 {
    pub completed_live_action_identity_verified: bool,
    pub adjacent_stable_transition_verified: bool,
    pub java_owned_untargeted_card_identity_verified: bool,
    pub exact_corruption_power_transition_verified: bool,
    pub complete_successor_card_identity_reconciled: bool,
    pub exact_card_zone_order_and_turn_costs_match: bool,
    pub energy_and_block_match: bool,
    pub player_hp_and_monsters_match: bool,
    pub unchanged_java_relic_and_potion_context_verified: bool,
    pub all_six_rng_streams_match: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CorruptionChainKind {
    Corruption,
    PowerThrough,
    SecondWind,
}

impl CorruptionChainKind {
    fn from_java_id(card_id: &str) -> Option<Self> {
        match card_id {
            CORRUPTION_JAVA_ID => Some(Self::Corruption),
            POWER_THROUGH_JAVA_ID => Some(Self::PowerThrough),
            SECOND_WIND_JAVA_ID => Some(Self::SecondWind),
            _ => None,
        }
    }

    fn scope(self) -> &'static str {
        match self {
            Self::Corruption => "plain_unupgraded_corruption_power_application",
            Self::PowerThrough => "unupgraded_power_through_under_exact_corruption",
            Self::SecondWind => "unupgraded_second_wind_under_exact_corruption",
        }
    }

    fn comparison_kind(self) -> &'static str {
        match self {
            Self::Corruption => "corruption_power_application",
            Self::PowerThrough => "power_through_under_corruption",
            Self::SecondWind => "second_wind_under_corruption",
        }
    }

    fn expected_cost(self) -> i32 {
        match self {
            Self::Corruption => 3,
            Self::PowerThrough | Self::SecondWind => 0,
        }
    }

    fn expected_destination(self) -> &'static str {
        match self {
            Self::Corruption => "removed_from_card_zones",
            Self::PowerThrough | Self::SecondWind => "exhaust",
        }
    }
}

pub(super) fn corruption_chain_comparison_kind(card_id: &str) -> Option<&'static str> {
    CorruptionChainKind::from_java_id(card_id).map(CorruptionChainKind::comparison_kind)
}

pub(super) fn compare_corruption_chain_loaded(
    loaded: LoadedLiveTransition,
) -> Result<JavaParityCorruptionChainComparisonReportV1, String> {
    let action = receipted_action(&loaded)?;
    let card_id = action
        .card_id
        .as_deref()
        .ok_or_else(|| "Corruption-chain action omitted card ID".to_owned())?;
    let kind = CorruptionChainKind::from_java_id(card_id)
        .ok_or_else(|| "Corruption-chain comparator received an unsupported card".to_owned())?;
    let moved = validate_typed_action(&loaded, action, kind)?;
    validate_context(&loaded, kind)?;

    let original_game = projection_from_java_frames(&loaded, &moved.instance_uuid)?;
    validate_original_semantics(&loaded, &moved, kind, &original_game)?;
    let simulator = run_simulator_transition(&loaded, &moved, kind)?;
    if original_game != simulator {
        return Err("Rust and Java Corruption-chain projections disagree".to_owned());
    }

    Ok(JavaParityCorruptionChainComparisonReportV1 {
        schema: "sts_java_parity_corruption_chain_comparison_v1".to_owned(),
        status: "matched".to_owned(),
        scope: kind.scope().to_owned(),
        reconstruction: "supported_corruption_power_and_ordered_successor_identity_v1"
            .to_owned(),
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
        claims: CorruptionChainClaimsV1 {
            completed_live_action_identity_verified: true,
            adjacent_stable_transition_verified: true,
            java_owned_untargeted_card_identity_verified: true,
            exact_corruption_power_transition_verified: true,
            complete_successor_card_identity_reconciled: true,
            exact_card_zone_order_and_turn_costs_match: true,
            energy_and_block_match: true,
            player_hp_and_monsters_match: true,
            unchanged_java_relic_and_potion_context_verified: true,
            all_six_rng_streams_match: true,
        },
        not_claimed: vec![
            "natural reachability, strategy quality, or training eligibility follows from this constructed transition".to_owned(),
            "Java UUID values equal simulator UUID values; Java identities are retained only as checked cross-runtime handles".to_owned(),
            "other player powers, duplicate Corruption, upgrades, Dexterity, Frail, Feel No Pain, Dark Embrace, Strange Spoon, or turn-boundary behavior are covered".to_owned(),
            "the unchanged Burning Blood and potion inventory are not executed by the minimal Rust reconstruction; they are verified Java context only".to_owned(),
            "other power, generated-card, or mass-exhaust actions inherit parity from this scoped chain".to_owned(),
            "exact Java combat state is restorable as a Rust checkpoint".to_owned(),
        ],
    })
}

fn validate_typed_action(
    loaded: &LoadedLiveTransition,
    action: &ActionProjection,
    kind: CorruptionChainKind,
) -> Result<CardProjection, String> {
    let card_uuid = action
        .card_uuid
        .as_deref()
        .ok_or_else(|| "Corruption-chain action omitted card UUID".to_owned())?;
    if action.kind != "play_card"
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
        .ok_or_else(|| "Corruption-chain card UUID is absent from predecessor hand".to_owned())?;
    if action.card_id.as_deref() != Some(moved.card_id.as_str())
        || CorruptionChainKind::from_java_id(&moved.card_id) != Some(kind)
        || moved.upgrades != 0
        || moved.cost_for_turn != kind.expected_cost()
    {
        return Err(
            "Corruption-chain comparison requires the exact ordinary unupgraded card".to_owned(),
        );
    }
    Ok(moved.clone())
}

fn validate_context(
    loaded: &LoadedLiveTransition,
    kind: CorruptionChainKind,
) -> Result<(), String> {
    let (origin, recipe) = live_transition_origin(loaded)?;
    let recipe = recipe.ok_or_else(|| {
        "Corruption-chain comparison requires a bound constructed scenario".to_owned()
    })?;
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
        || before_relics != ["Burning Blood"]
        || loaded.before.public_base.combat.potions != loaded.after.public_base.combat.potions
        || loaded
            .before
            .public_base
            .combat
            .potions
            .iter()
            .any(|potion| !potion.empty)
        || loaded.before.public_base.combat.turn != loaded.after.public_base.combat.turn
        || loaded.before.public_base.combat.monsters != loaded.after.public_base.combat.monsters
        || loaded.before.public_base.combat.player.current_hp
            != loaded.after.public_base.combat.player.current_hp
        || loaded.before.public_base.combat.player.max_hp
            != loaded.after.public_base.combat.player.max_hp
        || loaded.before.rng_internal.streams.len() != 6
        || loaded.before.rng_internal.streams != loaded.after.rng_internal.streams
    {
        return Err(
            "Corruption-chain comparison requires one plain same-turn constructed context"
                .to_owned(),
        );
    }
    validate_rng_owners(&loaded.before.rng_internal.streams)?;
    match kind {
        CorruptionChainKind::Corruption => {
            if !loaded.before.parity_internal.player_powers.is_empty() {
                return Err(
                    "Corruption application requires no predecessor player powers".to_owned(),
                );
            }
            validate_exact_corruption_player_power(&loaded.after.parity_internal.player_powers)
        }
        CorruptionChainKind::PowerThrough | CorruptionChainKind::SecondWind => {
            validate_exact_corruption_player_power(&loaded.before.parity_internal.player_powers)?;
            validate_exact_corruption_player_power(&loaded.after.parity_internal.player_powers)
        }
    }
}

fn validate_original_semantics(
    loaded: &LoadedLiveTransition,
    moved: &CardProjection,
    kind: CorruptionChainKind,
    projection: &CorruptionChainProjectionV1,
) -> Result<(), String> {
    if projection.energy_after != projection.energy_before - kind.expected_cost()
        || projection.selected_card_destination != kind.expected_destination()
    {
        return Err(
            "original-game Corruption-chain cost or card destination is unexpected".to_owned(),
        );
    }
    match kind {
        CorruptionChainKind::Corruption => {
            if projection.player_block_after != projection.player_block_before
                || !projection.player_powers_before.is_empty()
                || projection.player_powers_after != exact_corruption_power_projection()
                || !projection.generated_cards.is_empty()
                || !projection.newly_exhausted_cards.is_empty()
            {
                return Err(
                    "original-game Corruption application is outside supported semantics"
                        .to_owned(),
                );
            }
        }
        CorruptionChainKind::PowerThrough => {
            if projection.player_block_after != projection.player_block_before + 15
                || projection.player_powers_before != exact_corruption_power_projection()
                || projection.player_powers_after != exact_corruption_power_projection()
                || projection.generated_cards != ["Wound", "Wound"]
                || projection.newly_exhausted_cards != [POWER_THROUGH_JAVA_ID]
            {
                return Err(
                    "original-game Power Through under Corruption is outside supported semantics"
                        .to_owned(),
                );
            }
        }
        CorruptionChainKind::SecondWind => {
            let remaining = loaded
                .before
                .parity_internal
                .zones
                .hand
                .cards
                .iter()
                .filter(|card| card.instance_uuid != moved.instance_uuid)
                .collect::<Vec<_>>();
            let mut non_attack_cards = Vec::new();
            for card in remaining {
                if !is_attack(card)? {
                    non_attack_cards.push(card);
                }
            }
            let non_attacks = non_attack_cards.len();
            if non_attacks == 0
                || projection.player_block_after
                    != projection.player_block_before
                        + 5 * i32::try_from(non_attacks).map_err(|_| {
                            "Second Wind non-Attack count does not fit i32".to_owned()
                        })?
                || projection.player_powers_before != exact_corruption_power_projection()
                || projection.player_powers_after != exact_corruption_power_projection()
                || !projection.generated_cards.is_empty()
                || projection.newly_exhausted_cards.len() != non_attacks + 1
            {
                return Err(
                    "original-game Second Wind under Corruption is outside supported semantics"
                        .to_owned(),
                );
            }
            let expected = non_attack_cards
                .into_iter()
                .map(|card| (card.instance_uuid.clone(), card.card_id.clone()))
                .collect::<BTreeMap<_, _>>();
            let exhausted = newly_exhausted_by_identity(
                &loaded.before.parity_internal.zones.exhaust.cards,
                &loaded.after.parity_internal.zones.exhaust.cards,
            );
            let exhausted_non_selected = exhausted
                .into_iter()
                .filter(|(uuid, _)| uuid != &moved.instance_uuid)
                .collect::<BTreeMap<_, _>>();
            if exhausted_non_selected != expected {
                return Err(
                    "Second Wind did not exhaust exactly the predecessor non-Attacks".to_owned(),
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

fn run_simulator_transition(
    loaded: &LoadedLiveTransition,
    moved: &CardProjection,
    kind: CorruptionChainKind,
) -> Result<CorruptionChainProjectionV1, String> {
    let (combat, mut identities) = match kind {
        CorruptionChainKind::Corruption => reconstruct_plain_card_combat(&loaded.before)?,
        CorruptionChainKind::PowerThrough | CorruptionChainKind::SecondWind => {
            reconstruct_corruption_card_combat(&loaded.before)?
        }
    };
    let selected_rust_lineage = identities
        .rust_lineage(moved.instance_uuid.as_str())
        .ok_or_else(|| "Corruption-chain UUID is absent from reconstructed combat".to_owned())?;
    let card_index = combat
        .zones
        .hand
        .iter()
        .position(|card| card.lineage == selected_rust_lineage)
        .ok_or_else(|| "Corruption-chain card is absent from reconstructed hand".to_owned())?;
    let before_zones = project_zones(&combat, &identities)?;
    let before_powers = project_rust_player_powers(&combat)?;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let input = ClientInput::PlayCard {
        card_index,
        target: None,
    };
    let stepper = EngineCombatStepper;
    if !stepper.is_legal_action(&position, &input) {
        return Err("reconstructed Corruption-chain card play is not legal".to_owned());
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
        return Err("Rust Corruption-chain play did not reach a finite stable boundary".to_owned());
    }
    let reached = &step.position.combat;
    card_birth::bind_card_birth_witness(
        &mut identities,
        &loaded.before,
        &loaded.after,
        &loaded.receipt,
        &observed.card_births,
    )?;
    let after_zones =
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
            "Rust Corruption-chain successor differs in player/monster/RNG state".to_owned(),
        );
    }
    projection_from_parts(
        &loaded.before,
        &before_zones,
        &after_zones,
        before_powers,
        project_rust_player_powers(reached)?,
        reached.turn.turn_count,
        reached.turn.energy as i32,
        reached.entities.player.current_hp,
        reached.entities.player.max_hp,
        reached.entities.player.block,
        &moved.instance_uuid,
        reached.entities.monsters == position.combat.entities.monsters,
        rust_rng == loaded.before.rng_internal.streams,
    )
}

fn projection_from_java_frames(
    loaded: &LoadedLiveTransition,
    moved_uuid: &str,
) -> Result<CorruptionChainProjectionV1, String> {
    let after = &loaded.after.public_base.combat;
    projection_from_parts(
        &loaded.before,
        &loaded.before.parity_internal.zones.as_card_zones(),
        &loaded.after.parity_internal.zones.as_card_zones(),
        project_java_player_powers(&loaded.before.parity_internal.player_powers)?,
        project_java_player_powers(&loaded.after.parity_internal.player_powers)?,
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
    before_zones: &ComparableZones,
    after_zones: &ComparableZones,
    player_powers_before: Vec<CorruptionChainPowerV1>,
    player_powers_after: Vec<CorruptionChainPowerV1>,
    turn_after: u32,
    energy_after: i32,
    hp_after: i32,
    max_hp_after: i32,
    block_after: i32,
    moved_uuid: &str,
    monsters_unchanged: bool,
    rng_unchanged: bool,
) -> Result<CorruptionChainProjectionV1, String> {
    Ok(CorruptionChainProjectionV1 {
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
        zones_before: project_zone_semantics(before_zones),
        zones_after: project_zone_semantics(after_zones),
        player_powers_before,
        player_powers_after,
        generated_cards: generated_card_ids(before_zones, after_zones),
        newly_exhausted_cards: newly_exhausted_card_ids(
            &before_zones.exhaust,
            &after_zones.exhaust,
        ),
        selected_card_destination: selected_card_destination(after_zones, moved_uuid)?,
        monsters_unchanged,
        six_rng_streams_unchanged: rng_unchanged,
    })
}

fn project_zone_semantics(zones: &ComparableZones) -> CorruptionChainZonesV1 {
    CorruptionChainZonesV1 {
        draw: project_cards(&zones.draw),
        discard: project_cards(&zones.discard),
        exhaust: project_cards(&zones.exhaust),
        hand: project_cards(&zones.hand),
        limbo: project_cards(&zones.limbo),
    }
}

fn project_cards(cards: &[CardProjection]) -> Vec<CorruptionChainCardV1> {
    cards
        .iter()
        .map(|card| CorruptionChainCardV1 {
            card_id: card.card_id.clone(),
            upgrades: card.upgrades,
            cost_for_turn: card.cost_for_turn,
        })
        .collect()
}

fn project_java_player_powers(
    powers: &[PowerInternalProjection],
) -> Result<Vec<CorruptionChainPowerV1>, String> {
    if powers.is_empty() {
        return Ok(Vec::new());
    }
    validate_exact_corruption_player_power(powers)?;
    Ok(exact_corruption_power_projection())
}

fn project_rust_player_powers(
    combat: &sts_core::runtime::combat::CombatState,
) -> Result<Vec<CorruptionChainPowerV1>, String> {
    let powers = sts_core::content::powers::store::powers_snapshot_for(combat, 0);
    if powers.is_empty() {
        return Ok(Vec::new());
    }
    if powers.len() != 1
        || powers[0].power_type != PowerId::Corruption
        || powers[0].amount != -1
        || powers[0].instance_id.is_some()
        || powers[0].extra_data != 0
        || powers[0].payload != PowerPayload::None
    {
        return Err(
            "Rust player powers are outside the exact supported Corruption scope".to_owned(),
        );
    }
    Ok(exact_corruption_power_projection())
}

fn exact_corruption_power_projection() -> Vec<CorruptionChainPowerV1> {
    vec![CorruptionChainPowerV1 {
        power_id: "Corruption".to_owned(),
        amount: -1,
    }]
}

fn generated_card_ids(before: &ComparableZones, after: &ComparableZones) -> Vec<String> {
    let known = all_cards(before)
        .into_iter()
        .map(|card| card.instance_uuid.as_str())
        .collect::<BTreeSet<_>>();
    all_cards(after)
        .into_iter()
        .filter(|card| !known.contains(card.instance_uuid.as_str()))
        .map(|card| card.card_id.clone())
        .collect()
}

fn newly_exhausted_card_ids(before: &[CardProjection], after: &[CardProjection]) -> Vec<String> {
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

fn newly_exhausted_by_identity(
    before: &[CardProjection],
    after: &[CardProjection],
) -> BTreeMap<String, String> {
    let known = before
        .iter()
        .map(|card| card.instance_uuid.as_str())
        .collect::<BTreeSet<_>>();
    after
        .iter()
        .filter(|card| !known.contains(card.instance_uuid.as_str()))
        .map(|card| (card.instance_uuid.clone(), card.card_id.clone()))
        .collect()
}

fn all_cards(zones: &ComparableZones) -> Vec<&CardProjection> {
    zones
        .draw
        .iter()
        .chain(&zones.discard)
        .chain(&zones.exhaust)
        .chain(&zones.hand)
        .chain(&zones.limbo)
        .collect()
}

fn selected_card_destination(zones: &ComparableZones, moved_uuid: &str) -> Result<String, String> {
    let destinations = [
        ("draw", &zones.draw),
        ("discard", &zones.discard),
        ("exhaust", &zones.exhaust),
        ("hand", &zones.hand),
        ("limbo", &zones.limbo),
    ]
    .into_iter()
    .filter(|(_, cards)| cards.iter().any(|card| card.instance_uuid == moved_uuid))
    .map(|(name, _)| name)
    .collect::<Vec<_>>();
    match destinations.as_slice() {
        [] => Ok("removed_from_card_zones".to_owned()),
        [destination] => Ok((*destination).to_owned()),
        _ => Err("selected card appears in more than one successor zone".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn power(id: &str, class_name: &str, amount: i32) -> PowerInternalProjection {
        PowerInternalProjection {
            index: 0,
            id: id.to_owned(),
            class_name: class_name.to_owned(),
            amount,
            power_type: "BUFF".to_owned(),
            ritual_skip_first: None,
            ritual_player_controlled: None,
        }
    }

    #[test]
    fn exact_corruption_power_projection_rejects_aliases_and_duplicates() {
        let exact = power(
            "Corruption",
            "com.megacrit.cardcrawl.powers.CorruptionPower",
            -1,
        );
        assert_eq!(
            project_java_player_powers(std::slice::from_ref(&exact)).unwrap(),
            exact_corruption_power_projection()
        );

        let alias = power("Corruption", "example.AliasPower", -1);
        assert!(project_java_player_powers(&[alias]).is_err());
        assert!(project_java_player_powers(&[exact.clone(), exact]).is_err());
    }
}
