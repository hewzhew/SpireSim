use super::*;
use serde::Serialize;

const LETTER_OPENER_JAVA_ID: &str = "Letter Opener";

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaParityLetterOpenerComparisonReportV1 {
    pub schema: String,
    pub status: String,
    pub scope: String,
    pub reconstruction: String,
    pub evidence_scope: LiveComparisonEvidenceScopeV1,
    pub source: LiveComparisonSourceV1,
    pub transition: ComparedLetterOpenerTransitionV1,
    pub original_game: LetterOpenerTransitionProjectionV1,
    pub simulator: LetterOpenerTransitionProjectionV1,
    pub claims: LetterOpenerClaimsV1,
    pub not_claimed: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparedLetterOpenerTransitionV1 {
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub action_id: String,
    pub action_kind: String,
    pub java_card_instance_uuid: Option<String>,
    pub java_card_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LetterOpenerTransitionProjectionV1 {
    pub turn_before: u32,
    pub turn_after: u32,
    pub energy_before: i32,
    pub energy_after: i32,
    pub block_before: i32,
    pub block_after: i32,
    pub relic_counter_before: i32,
    pub relic_counter_after: i32,
    pub monster_hp_before: Vec<i32>,
    pub monster_hp_after: Vec<i32>,
    pub zones_before: LetterOpenerZonesProjectionV1,
    pub zones_after: LetterOpenerZonesProjectionV1,
    pub end_turn: Option<EndTurnProjectionV1>,
    pub six_rng_streams_unchanged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LetterOpenerZonesProjectionV1 {
    pub hand: Vec<String>,
    pub draw: Vec<String>,
    pub discard: Vec<String>,
    pub exhaust: Vec<String>,
    pub limbo: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LetterOpenerClaimsV1 {
    pub completed_live_action_identity_verified: bool,
    pub adjacent_stable_transition_verified: bool,
    pub fresh_exact_letter_opener_context_verified: bool,
    pub relic_counter_projection_present: bool,
    pub relic_counter_transition_matches: bool,
    pub card_or_end_turn_semantics_match: bool,
    pub exact_card_zone_order_matches: bool,
    pub player_and_monster_projection_matches: bool,
    pub all_six_rng_streams_match: bool,
}

pub(super) fn has_letter_opener_context(loaded: &LoadedLiveTransition) -> bool {
    loaded.before.parity_internal.relics.len() == 1
        && loaded.before.parity_internal.relics[0].relic_id == LETTER_OPENER_JAVA_ID
}

pub(super) fn letter_opener_comparison_kind(
    report: &JavaParityLetterOpenerComparisonReportV1,
) -> &'static str {
    match report.transition.action_kind.as_str() {
        "play_card" => "letter_opener_skill_count",
        "end_turn" => "letter_opener_turn_reset",
        _ => "unsupported_letter_opener_transition",
    }
}

pub(super) fn compare_letter_opener_loaded(
    loaded: LoadedLiveTransition,
) -> Result<JavaParityLetterOpenerComparisonReportV1, String> {
    validate_letter_opener_context(&loaded)?;
    let action = receipted_action(&loaded)?.clone();
    let (moved, original_game, simulator, scope, reconstruction) = match action.kind.as_str() {
        "play_card" => {
            let moved = validate_letter_opener_defend_transition(&loaded, &action)?;
            let original_game = projection_from_java_frames(&loaded, None)?;
            let simulator = run_letter_opener_defend_transition(&loaded, &moved)?;
            (
                Some(moved),
                original_game,
                simulator,
                "fresh_exact_letter_opener_unupgraded_defend_counter",
                "typed_card_zones_and_letter_opener_counter_v1",
            )
        }
        "end_turn" => {
            validate_letter_opener_end_turn_transition(&loaded, &action)?;
            let original_end = project_end_turn_frames(&loaded.before, &loaded.after)?;
            let before_counter = frame_letter_opener_counter(&loaded.before)?;
            let (simulator_end, simulator_counter) =
                run_cultist_turn_one_end_turn_transition_with_letter_opener(
                    &loaded.before,
                    &loaded.after,
                    Some(before_counter),
                )?;
            let simulator_counter = simulator_counter.ok_or_else(|| {
                "Rust end-turn successor lost the reconstructed Letter Opener".to_owned()
            })?;
            let original_game = projection_from_java_frames(&loaded, Some(original_end))?;
            let simulator = projection_from_java_frames_with_counter(
                &loaded,
                simulator_counter,
                Some(simulator_end),
            )?;
            (
                None,
                original_game,
                simulator,
                "fresh_exact_letter_opener_turn_start_counter_reset",
                "typed_cultist_end_turn_with_letter_opener_counter_v1",
            )
        }
        _ => return Err("Letter Opener comparison received an unsupported action".to_owned()),
    };
    if original_game != simulator {
        return Err("Rust and Java Letter Opener projections disagree".to_owned());
    }

    Ok(JavaParityLetterOpenerComparisonReportV1 {
        schema: "sts_java_parity_letter_opener_comparison_v1".to_owned(),
        status: "matched".to_owned(),
        scope: scope.to_owned(),
        reconstruction: reconstruction.to_owned(),
        evidence_scope: live_comparison_evidence_scope(&loaded)?,
        source: live_comparison_source(&loaded)?,
        transition: ComparedLetterOpenerTransitionV1 {
            from_frame_id: loaded.before.frame_id,
            to_frame_id: loaded.after.frame_id,
            action_id: loaded.receipt.action_id,
            action_kind: action.kind.clone(),
            java_card_instance_uuid: moved
                .as_ref()
                .map(|card| card.instance_uuid.clone()),
            java_card_id: moved.as_ref().map(|card| card.card_id.clone()),
        },
        original_game,
        simulator,
        claims: LetterOpenerClaimsV1 {
            completed_live_action_identity_verified: true,
            adjacent_stable_transition_verified: true,
            fresh_exact_letter_opener_context_verified: true,
            relic_counter_projection_present: true,
            relic_counter_transition_matches: true,
            card_or_end_turn_semantics_match: true,
            exact_card_zone_order_matches: true,
            player_and_monster_projection_matches: true,
            all_six_rng_streams_match: true,
        },
        not_claimed: vec![
            "natural reachability, strategy quality, or training eligibility follows from this constructed transition".to_owned(),
            "arbitrary relic fields are writable; the scenario recipe still selects only a fresh exact relic identity".to_owned(),
            "Letter Opener interactions with Dexterity, Frail, block on enemies, lethal damage, powers, or other relics are covered".to_owned(),
            "other counter relics inherit parity from this Letter Opener comparison".to_owned(),
            "exact Java combat state is restorable as a Rust checkpoint".to_owned(),
        ],
    })
}

fn validate_letter_opener_context(loaded: &LoadedLiveTransition) -> Result<(), String> {
    let (origin, recipe) = live_transition_origin(loaded)?;
    let recipe = recipe
        .ok_or_else(|| "Letter Opener comparison requires a bound scenario recipe".to_owned())?;
    let [ScenarioCapability::Encounter(encounter), ScenarioCapability::PlayerHealth(_), ScenarioCapability::MasterDeck(deck), ScenarioCapability::FreshRelics(relics)] =
        recipe.capabilities.as_slice()
    else {
        return Err(
            "Letter Opener comparison requires encounter, health, deck, and fresh relic capabilities"
                .to_owned(),
        );
    };
    if origin != "constructed_validated"
        || encounter.encounter_id != "Cultist"
        || deck.cards.len() != 3
        || deck
            .cards
            .iter()
            .any(|card| card.card_id != BASE_DEFEND_JAVA_ID || card.upgrades != 0)
        || relics.relics.len() != 1
        || relics.relics[0].relic_id != LETTER_OPENER_JAVA_ID
        || !loaded.before.parity_internal.player_powers.is_empty()
        || !loaded.after.parity_internal.player_powers.is_empty()
        || loaded
            .before
            .public_base
            .combat
            .potions
            .iter()
            .any(|potion| !potion.empty)
        || loaded.before.public_base.combat.potions != loaded.after.public_base.combat.potions
    {
        return Err(
            "Letter Opener comparison is outside its closed constructed context".to_owned(),
        );
    }
    for frame in [&loaded.before, &loaded.after] {
        let [relic] = frame.parity_internal.relics.as_slice() else {
            return Err("Letter Opener comparison requires one projected relic".to_owned());
        };
        if relic.index != 0 || relic.relic_id != LETTER_OPENER_JAVA_ID || relic.counter.is_none() {
            return Err(
                "Letter Opener comparison requires the typed Java relic counter projection"
                    .to_owned(),
            );
        }
    }
    validate_rng_owners(&loaded.before.rng_internal.streams)?;
    validate_rng_owners(&loaded.after.rng_internal.streams)
}

fn validate_letter_opener_defend_transition(
    loaded: &LoadedLiveTransition,
    action: &ActionProjection,
) -> Result<CardProjection, String> {
    let card_uuid = action
        .card_uuid
        .as_deref()
        .ok_or_else(|| "Letter Opener Defend action omitted its Java card UUID".to_owned())?;
    if action.kind != "play_card"
        || action.card_id.as_deref() != Some(BASE_DEFEND_JAVA_ID)
        || action.potion_slot.is_some()
        || action.potion_id.is_some()
        || action.target_monster_instance_id.is_some()
        || action.target_monster_index.is_some()
        || action.target_monster_id.is_some()
    {
        return Err("receipted action is not the typed untargeted Defend".to_owned());
    }
    let before_zones = &loaded.before.parity_internal.zones;
    let after_zones = &loaded.after.parity_internal.zones;
    let moved_index = before_zones
        .hand
        .cards
        .iter()
        .position(|card| card.instance_uuid == card_uuid)
        .ok_or_else(|| "Letter Opener Defend is absent from predecessor hand".to_owned())?;
    let moved = before_zones.hand.cards[moved_index].clone();
    if moved.card_id != BASE_DEFEND_JAVA_ID || moved.upgrades != 0 || moved.cost_for_turn != 1 {
        return Err("Letter Opener comparison requires one ordinary unupgraded Defend".to_owned());
    }
    let mut expected_hand = before_zones.hand.cards.clone();
    expected_hand.remove(moved_index);
    let mut expected_discard = before_zones.discard.cards.clone();
    expected_discard.push(moved.clone());
    if after_zones.hand.cards != expected_hand
        || after_zones.discard.cards != expected_discard
        || after_zones.draw.cards != before_zones.draw.cards
        || after_zones.exhaust.cards != before_zones.exhaust.cards
        || after_zones.limbo.cards != before_zones.limbo.cards
    {
        return Err("Letter Opener Defend changed card zones outside ordinary play".to_owned());
    }
    let before_player = &loaded.before.public_base.combat.player;
    let after_player = &loaded.after.public_base.combat.player;
    let before_counter = frame_letter_opener_counter(&loaded.before)?;
    let after_counter = frame_letter_opener_counter(&loaded.after)?;
    let expected_counter = (before_counter + 1) % 3;
    let trigger_damage = if before_counter == 2 { 5 } else { 0 };
    let [before_monster] = loaded.before.public_base.combat.monsters.as_slice() else {
        return Err("Letter Opener comparison requires one Cultist".to_owned());
    };
    let [after_monster] = loaded.after.public_base.combat.monsters.as_slice() else {
        return Err("Letter Opener comparison requires one Cultist successor".to_owned());
    };
    if !(0..=2).contains(&before_counter)
        || after_counter != expected_counter
        || before_player.energy - 1 != after_player.energy
        || before_player.block + 5 != after_player.block
        || before_player.current_hp != after_player.current_hp
        || before_player.max_hp != after_player.max_hp
        || before_monster.block != 0
        || after_monster.block != 0
        || before_monster.current_hp <= trigger_damage
        || after_monster.current_hp != before_monster.current_hp - trigger_damage
        || after_monster.max_hp != before_monster.max_hp
        || after_monster.dead_or_escaped
        || loaded.before.rng_internal.streams != loaded.after.rng_internal.streams
    {
        return Err(
            "Java Letter Opener counter, Defend, or trigger transition is outside scope".to_owned(),
        );
    }
    Ok(moved)
}

fn validate_letter_opener_end_turn_transition(
    loaded: &LoadedLiveTransition,
    action: &ActionProjection,
) -> Result<(), String> {
    if action.kind != "end_turn"
        || action.action_id != "end_turn"
        || action.card_uuid.is_some()
        || action.card_id.is_some()
        || action.target_monster_instance_id.is_some()
        || action.target_monster_index.is_some()
        || action.target_monster_id.is_some()
        || frame_letter_opener_counter(&loaded.before)? != 2
        || frame_letter_opener_counter(&loaded.after)? != 0
    {
        return Err("Letter Opener end turn is not the typed 2-to-0 reset transition".to_owned());
    }
    validate_cultist_turn_one_original_transition(&loaded.before, &loaded.after)
}

fn run_letter_opener_defend_transition(
    loaded: &LoadedLiveTransition,
    moved: &CardProjection,
) -> Result<LetterOpenerTransitionProjectionV1, String> {
    let (mut combat, identities) = reconstruct_plain_card_combat(&loaded.before)?;
    let mut relic = RelicState::new(RelicId::LetterOpener);
    relic.counter = frame_letter_opener_counter(&loaded.before)?;
    combat.entities.player.add_relic(relic);
    let rust_lineage = identities
        .rust_lineage(moved.instance_uuid.as_str())
        .ok_or_else(|| "Letter Opener Defend UUID is absent from Rust reconstruction".to_owned())?;
    let card_index = combat
        .zones
        .hand
        .iter()
        .position(|card| card.lineage == rust_lineage)
        .ok_or_else(|| "Letter Opener Defend is absent from Rust hand".to_owned())?;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let input = ClientInput::PlayCard {
        card_index,
        target: None,
    };
    let stepper = EngineCombatStepper;
    if !stepper.is_legal_action(&position, &input) {
        return Err("reconstructed Letter Opener Defend is not legal".to_owned());
    }
    let step = stepper.apply_to_stable(
        &position,
        input,
        CombatStepLimits {
            max_engine_steps: MAX_ENGINE_STEPS,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err("Rust Letter Opener Defend did not reach a stable boundary".to_owned());
    }
    let reached = &step.position.combat;
    let zones_after = project_zones(reached, &identities)?;
    if zones_after != loaded.after.parity_internal.zones.as_card_zones()
        || reached.turn.turn_count != loaded.after.public_base.combat.turn
        || reached.turn.energy as i32 != loaded.after.public_base.combat.player.energy
        || reached.entities.player.current_hp != loaded.after.public_base.combat.player.current_hp
        || reached.entities.player.max_hp != loaded.after.public_base.combat.player.max_hp
        || reached.entities.player.block != loaded.after.public_base.combat.player.block
        || reached.entities.monsters.len() != loaded.after.public_base.combat.monsters.len()
        || reached
            .entities
            .monsters
            .iter()
            .zip(&loaded.after.public_base.combat.monsters)
            .any(|(rust, java)| {
                rust.current_hp != java.current_hp
                    || rust.max_hp != java.max_hp
                    || rust.block != java.block
                    || rust.is_dying != java.dead_or_escaped
            })
    {
        return Err("Rust Letter Opener Defend successor differs from Java".to_owned());
    }
    let rust_rng = project_rng_streams(&reached.rng.pool);
    if rust_rng != loaded.after.rng_internal.streams {
        return Err("Rust Letter Opener Defend changed the captured RNG streams".to_owned());
    }
    let counter_after = reached
        .entities
        .player
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::LetterOpener)
        .map(|relic| relic.counter)
        .ok_or_else(|| "Rust Letter Opener Defend lost the relic".to_owned())?;
    Ok(LetterOpenerTransitionProjectionV1 {
        turn_before: loaded.before.public_base.combat.turn,
        turn_after: reached.turn.turn_count,
        energy_before: loaded.before.public_base.combat.player.energy,
        energy_after: reached.turn.energy as i32,
        block_before: loaded.before.public_base.combat.player.block,
        block_after: reached.entities.player.block,
        relic_counter_before: frame_letter_opener_counter(&loaded.before)?,
        relic_counter_after: counter_after,
        monster_hp_before: loaded
            .before
            .public_base
            .combat
            .monsters
            .iter()
            .map(|monster| monster.current_hp)
            .collect(),
        monster_hp_after: reached
            .entities
            .monsters
            .iter()
            .map(|monster| monster.current_hp)
            .collect(),
        zones_before: zones_projection(&loaded.before.parity_internal.zones.as_card_zones()),
        zones_after: zones_projection(&zones_after),
        end_turn: None,
        six_rng_streams_unchanged: rust_rng == loaded.before.rng_internal.streams,
    })
}

fn projection_from_java_frames(
    loaded: &LoadedLiveTransition,
    end_turn: Option<EndTurnProjectionV1>,
) -> Result<LetterOpenerTransitionProjectionV1, String> {
    projection_from_java_frames_with_counter(
        loaded,
        frame_letter_opener_counter(&loaded.after)?,
        end_turn,
    )
}

fn projection_from_java_frames_with_counter(
    loaded: &LoadedLiveTransition,
    counter_after: i32,
    end_turn: Option<EndTurnProjectionV1>,
) -> Result<LetterOpenerTransitionProjectionV1, String> {
    Ok(LetterOpenerTransitionProjectionV1 {
        turn_before: loaded.before.public_base.combat.turn,
        turn_after: loaded.after.public_base.combat.turn,
        energy_before: loaded.before.public_base.combat.player.energy,
        energy_after: loaded.after.public_base.combat.player.energy,
        block_before: loaded.before.public_base.combat.player.block,
        block_after: loaded.after.public_base.combat.player.block,
        relic_counter_before: frame_letter_opener_counter(&loaded.before)?,
        relic_counter_after: counter_after,
        monster_hp_before: loaded
            .before
            .public_base
            .combat
            .monsters
            .iter()
            .map(|monster| monster.current_hp)
            .collect(),
        monster_hp_after: loaded
            .after
            .public_base
            .combat
            .monsters
            .iter()
            .map(|monster| monster.current_hp)
            .collect(),
        zones_before: zones_projection(&loaded.before.parity_internal.zones.as_card_zones()),
        zones_after: zones_projection(&loaded.after.parity_internal.zones.as_card_zones()),
        end_turn,
        six_rng_streams_unchanged: loaded.before.rng_internal.streams
            == loaded.after.rng_internal.streams,
    })
}

fn frame_letter_opener_counter(frame: &StateFrameProjection) -> Result<i32, String> {
    let [relic] = frame.parity_internal.relics.as_slice() else {
        return Err("frame does not contain exactly one Letter Opener".to_owned());
    };
    if relic.index != 0 || relic.relic_id != LETTER_OPENER_JAVA_ID {
        return Err("frame relic identity is not Letter Opener".to_owned());
    }
    relic
        .counter
        .ok_or_else(|| "frame predates the typed relic-counter projection".to_owned())
}

fn zones_projection(zones: &ComparableZones) -> LetterOpenerZonesProjectionV1 {
    let ids = |cards: &[CardProjection]| {
        cards
            .iter()
            .map(|card| card.card_id.clone())
            .collect::<Vec<_>>()
    };
    LetterOpenerZonesProjectionV1 {
        hand: ids(&zones.hand),
        draw: ids(&zones.draw),
        discard: ids(&zones.discard),
        exhaust: ids(&zones.exhaust),
        limbo: ids(&zones.limbo),
    }
}
