use crate::runtime::action::{Action, ActionInfo};
use crate::runtime::combat::CombatState;
use crate::state::core::{ClientInput, EngineState};
use crate::state::selection::{EngineDiagnostic, EngineDiagnosticClass, EngineDiagnosticSeverity};
use smallvec::SmallVec;

use super::targeting;

mod combat_processing;
mod diagnostics;
mod discovery;
mod victory;

mod pending_choice_resolution;
use diagnostics::record_engine_diagnostic;
pub(crate) use diagnostics::with_suppressed_engine_warnings;

#[cfg(test)]
use discovery::{
    any_color_attack_pool_sorted, generate_discovery_choices, generate_foreign_influence_choices,
};
#[cfg(test)]
use victory::settle_victory_if_ready;

use pending_choice_resolution::resolve_pending_choice;

pub(crate) const COMBAT_ENGINE_PHASE_PROFILE_COUNT: usize = 13;

#[derive(Clone, Copy, Debug)]
pub(crate) enum CombatEngineProfilePhaseV1 {
    DiscardHand,
    MonsterPreTurn,
    MonsterTurnSetup,
    MonsterMoveResolution,
    MonsterDuringTurnPowers,
    MonsterActionDamageRoute,
    MonsterActionPowerRoute,
    MonsterActionCardRoute,
    MonsterActionSpawnRoute,
    MonsterActionOrbRoute,
    MonsterActionUnhandledRoute,
    MonsterEndRound,
    PlayerTurnStart,
}

impl CombatEngineProfilePhaseV1 {
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::DiscardHand => 0,
            Self::MonsterPreTurn => 1,
            Self::MonsterTurnSetup => 2,
            Self::MonsterMoveResolution => 3,
            Self::MonsterDuringTurnPowers => 4,
            Self::MonsterActionDamageRoute => 5,
            Self::MonsterActionPowerRoute => 6,
            Self::MonsterActionCardRoute => 7,
            Self::MonsterActionSpawnRoute => 8,
            Self::MonsterActionOrbRoute => 9,
            Self::MonsterActionUnhandledRoute => 10,
            Self::MonsterEndRound => 11,
            Self::PlayerTurnStart => 12,
        }
    }
}

pub(crate) trait CombatEnginePhaseProfiler {
    type Marker;

    fn begin(&mut self, phase: CombatEngineProfilePhaseV1) -> Self::Marker;
    fn end(&mut self, phase: CombatEngineProfilePhaseV1, marker: Self::Marker);
}

#[derive(Default)]
pub(crate) struct NoopCombatEnginePhaseProfiler;

impl CombatEnginePhaseProfiler for NoopCombatEnginePhaseProfiler {
    type Marker = ();

    #[inline]
    fn begin(&mut self, _phase: CombatEngineProfilePhaseV1) {}

    #[inline]
    fn end(&mut self, _phase: CombatEngineProfilePhaseV1, _marker: Self::Marker) {}
}

pub fn compute_player_turn_start_draw_count(combat_state: &CombatState) -> i32 {
    let mut draw_count: i32 = 5 + combat_state.turn.turn_start_draw_modifier;
    if combat_state
        .entities
        .player
        .has_relic(crate::content::relics::RelicId::SneckoEye)
    {
        draw_count += crate::content::relics::snecko_eye::ADDITIONAL_CARDS_DRAWN_EACH_TURN;
    }
    draw_count
}

/// Java DiscardAtEndOfTurnAction checks these independently of card Retain flags.
pub fn player_skips_ordinary_end_turn_discard(combat_state: &CombatState) -> bool {
    use crate::content::relics::mechanics::{
        relic_end_turn_discard_rule_v1, RelicEndTurnDiscardRuleV1,
    };
    combat_state.entities.player.relics.iter().any(|relic| {
        relic_end_turn_discard_rule_v1(relic.id)
            == Some(RelicEndTurnDiscardRuleV1::SkipOrdinaryDiscard)
    }) || crate::content::powers::store::has_power(
        combat_state,
        0,
        crate::content::powers::PowerId::Equilibrium,
    )
}

pub(crate) fn discard_hand_for_turn_transition(combat_state: &mut CombatState) {
    let skip_discard = player_skips_ordinary_end_turn_discard(combat_state);
    let mut retained = Vec::new();
    let mut discarded = Vec::new();
    for mut card in combat_state.zones.hand.drain(..) {
        if card.retain_override == Some(true) || crate::content::cards::is_self_retain(&card) {
            crate::content::cards::trigger_on_retained(&mut card);
            card.retain_override = None;
            retained.push(card);
        } else {
            discarded.push(card);
        }
    }

    // Retained cards are in Java limbo during all ordinary discard callbacks.
    combat_state.zones.hand = discarded;
    if !skip_discard {
        // Each move refreshes the remaining hand before the next top card is
        // removed; draining the whole group first loses Corruption mutations.
        while let Some(card) = combat_state.zones.hand.pop() {
            crate::engine::action_handlers::cards::move_card_to_discard_pile(card, combat_state);
        }
    }
    combat_state.zones.hand.extend(retained);
}

/// Queue Java `AbstractCard.triggerOnEndOfPlayerTurn()` Ethereal exhausts.
///
/// This is deliberately a separate stage before `discard_hand_for_turn_transition`.
/// End-turn cardQueue plays such as Burn have already resolved when the engine
/// reaches an empty TurnTransition queue, while Java's DiscardAtEndOfTurnAction
/// exhausts non-retained Ethereal cards before its ordinary hand discard.
fn queue_end_of_turn_ethereal_exhausts(combat_state: &mut CombatState) -> bool {
    let ethereal_card_lineages = combat_state
        .zones
        .hand
        .iter()
        .filter(|card| {
            card.retain_override != Some(true)
                && !crate::content::cards::is_self_retain(card)
                && crate::content::cards::is_ethereal(card)
        })
        .map(|card| card.lineage)
        .collect::<Vec<_>>();

    if ethereal_card_lineages.is_empty() {
        return false;
    }

    // The base game shuffles the trigger iteration with java.util.Collections'
    // process-local Random before addToTop. That order is not derived from any
    // saved/game RNG. Preserve deterministic hand order here; one remaining
    // Ethereal card is exact, while multi-card trigger ordering is an explicit
    // original-runtime nondeterminism boundary.
    for card_lineage in ethereal_card_lineages {
        combat_state.queue_action_back(Action::ExhaustCard {
            card_lineage,
            source_pile: crate::state::PileType::Hand,
        });
    }
    // Java queues ordinary end-turn discards before invoking Ethereal hooks.
    // ExhaustSpecificCardAction is then added to the top, while relic hooks
    // such as Dead Branch are added to the bottom. This marker recreates the
    // resulting order: Exhaust -> discard remaining hand -> bottom hooks.
    combat_state.queue_action_back(Action::DiscardHandForTurnTransition);
    true
}

pub fn tick_engine(
    engine_state: &mut EngineState,
    combat_state: &mut CombatState,
    input: Option<ClientInput>,
) -> bool {
    let mut profiler = NoopCombatEnginePhaseProfiler;
    tick_engine_with_profiler(engine_state, combat_state, input, &mut profiler)
}

pub(crate) fn tick_engine_with_profiler<P: CombatEnginePhaseProfiler>(
    engine_state: &mut EngineState,
    combat_state: &mut CombatState,
    input: Option<ClientInput>,
    profiler: &mut P,
) -> bool {
    // Phase 1: pending choice overrides
    if let EngineState::PendingChoice(choice) = engine_state {
        if let Some(cmd) = input {
            if let ClientInput::DiscardPotion(slot) = cmd {
                if crate::sim::combat_action_surface::pending_choice_input_is_legal(
                    choice,
                    combat_state,
                    &ClientInput::DiscardPotion(slot),
                ) {
                    combat_state.entities.potions[slot] = None;
                } else {
                    record_engine_diagnostic(
                        combat_state,
                        EngineDiagnostic {
                            severity: EngineDiagnosticSeverity::Error,
                            class: EngineDiagnosticClass::Broken,
                            message: "Rejected pending-choice potion discard".to_owned(),
                        },
                    );
                }
                return true;
            }
            match resolve_pending_choice(engine_state, combat_state, cmd) {
                Ok(()) => {
                    if !matches!(engine_state, EngineState::PendingChoice(_)) {
                        *engine_state = EngineState::CombatProcessing;
                    }
                }
                Err(err) => record_engine_diagnostic(
                    combat_state,
                    EngineDiagnostic {
                        severity: EngineDiagnosticSeverity::Error,
                        class: EngineDiagnosticClass::Broken,
                        message: format!("Rejected pending-choice input: {err}"),
                    },
                ),
            }
        }
        return true;
    }

    // Phase 2: process input
    if *engine_state == EngineState::CombatPlayerTurn {
        if let Some(cmd) = input {
            match handle_player_turn_input(engine_state, combat_state, cmd) {
                Ok(()) => {
                    // After a card play, actions (damage, block, etc.) are queued.
                    // Transition to CombatProcessing to drain the queue.
                    if combat_state.has_pending_actions()
                        || !combat_state.zones.queued_cards.is_empty()
                    {
                        *engine_state = EngineState::CombatProcessing;
                    }
                }
                Err(err) => {
                    record_engine_diagnostic(
                        combat_state,
                        EngineDiagnostic {
                            severity: EngineDiagnosticSeverity::Error,
                            class: EngineDiagnosticClass::Broken,
                            message: format!("Rejected player-turn input: {err}"),
                        },
                    );
                    return true;
                }
            }
        } else {
            return true;
        }
    }

    // Phase 3: execute action queue
    if *engine_state == EngineState::CombatProcessing {
        return combat_processing::process_combat_processing(engine_state, combat_state, profiler);
    }

    if let Some(keep_running) = victory::settle_victory_if_ready(engine_state, combat_state) {
        return keep_running;
    }

    true
}

fn handle_player_turn_input(
    engine_state: &mut EngineState,
    combat_state: &mut CombatState,
    cmd: ClientInput,
) -> Result<(), &'static str> {
    match cmd {
        ClientInput::PlayCard { card_index, target } => {
            crate::engine::action_handlers::cards::handle_play_card_from_hand(
                card_index,
                target,
                combat_state,
            )
        }

        ClientInput::UsePotion {
            potion_index,
            mut target,
        } => {
            let potion = combat_state
                .entities
                .potions
                .get(potion_index)
                .and_then(|p| p.as_ref())
                .ok_or("Potion index out of range")?;
            if !crate::content::potions::potion_can_use_in_combat_like_java(potion, combat_state) {
                return Err("Potion cannot be used in current combat state");
            }
            let def = crate::content::potions::get_potion_definition(potion.id);
            target = targeting::resolve_target_request(
                combat_state,
                targeting::validation_for_potion_target(def.target_required),
                target,
            )?;
            // Queue UsePotion action; action_handlers.rs performs the actual work.
            combat_state.queue_action_back(Action::UsePotion {
                slot: potion_index,
                target,
            });
            Ok(())
        }

        ClientInput::DiscardPotion(slot) => {
            let potion = combat_state
                .entities
                .potions
                .get(slot)
                .and_then(|p| p.as_ref())
                .ok_or("Potion index out of range")?;
            if !potion.can_discard {
                return Err("Potion cannot be discarded in current combat state");
            }
            combat_state.queue_action_back(Action::DiscardPotion { slot });
            Ok(())
        }

        ClientInput::EndTurn => {
            // Queue Java callEndOfTurnActions equivalent. The marker expands to
            // relics, pre-card powers, orb passives, in-hand card triggers, and
            // stance cleanup. Ordinary powers follow the drained card queue.
            combat_state.queue_action_back(Action::EndTurnTrigger);
            *engine_state = EngineState::CombatProcessing;
            combat_state.begin_turn_transition();
            Ok(())
        }

        _ => Err("Invalid input for player turn"),
    }
}

pub fn queue_actions(
    queue: &mut std::collections::VecDeque<Action>,
    actions: SmallVec<[ActionInfo; 4]>,
) {
    // `ActionInfo` order is the Java call order.  Java `addToTop`
    // inserts at index 0 immediately, so later top insertions run before
    // earlier top insertions.
    for a in actions {
        match a.insertion_mode {
            crate::runtime::action::AddTo::Top => queue.push_front(a.action),
            crate::runtime::action::AddTo::Bottom => queue.push_back(a.action),
        }
    }
}

/// Legacy Java UI preview refresh. Rust engine no longer mutates protocol observation caches here.
pub fn update_monster_intents(_combat_state: &mut CombatState) {}

/// Run tick_engine until it returns to CombatPlayerTurn or game over.
pub fn tick_until_stable_turn(
    es: &mut EngineState,
    cs: &mut CombatState,
    input: ClientInput,
) -> bool {
    // First tick with input
    let alive = tick_engine(es, cs, Some(input));
    if !alive {
        return false;
    }

    // After any input: engine stays at CombatPlayerTurn but actions may be queued.
    // We need to force CombatProcessing to drain the action queue.
    if *es == EngineState::CombatPlayerTurn
        && (cs.has_pending_actions() || !cs.zones.queued_cards.is_empty())
    {
        *es = EngineState::CombatProcessing;
    }

    // Keep ticking until we're back at PlayerTurn (waiting for input), or we're in a PendingChoice
    let mut iterations = 0;
    loop {
        match es {
            EngineState::CombatPlayerTurn => break,
            EngineState::CombatProcessing => {}
            EngineState::PendingChoice(_) => break, // Would need user input
            EngineState::GameOver(_) => return false,
            _ => break, // RewardScreen, etc.
        }
        let alive = tick_engine(es, cs, None);
        if !alive {
            return false;
        }
        iterations += 1;
        if iterations > 1000 {
            record_engine_diagnostic(
                cs,
                EngineDiagnostic {
                    severity: EngineDiagnosticSeverity::Warning,
                    class: EngineDiagnosticClass::Suspicious,
                    message: "tick loop exceeded 1000 iterations".to_string(),
                },
            );
            break;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{
        any_color_attack_pool_sorted, discard_hand_for_turn_transition, generate_discovery_choices,
        generate_foreign_influence_choices, resolve_pending_choice, settle_victory_if_ready,
    };
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::PowerId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::action::{Action, CardDestination, CardRewardPool, DamageInfo, DamageType};
    use crate::runtime::combat::{CombatCard, Power, StanceId};
    use crate::state::core::{
        ClientInput, DiscoveryChoiceState, EngineState, GridSelectFilter, GridSelectReason,
        PendingChoice, PileType,
    };
    use crate::test_support::{blank_test_combat, planned_monster};

    #[test]
    fn class_combat_card_pool_uses_current_player_class_not_ironclad_fallback() {
        let silent_pool = crate::content::cards::class_combat_card_pool_for_type("Silent", None);
        assert!(silent_pool.contains(&CardId::Acrobatics));
        assert!(silent_pool.contains(&CardId::Adrenaline));
        assert!(
            !silent_pool.contains(&CardId::PommelStrike),
            "Discovery/Codex-style class pools must not hard-code Ironclad cards for Silent"
        );

        let ironclad_pool =
            crate::content::cards::class_combat_card_pool_for_type("Ironclad", None);
        assert!(ironclad_pool.contains(&CardId::PommelStrike));
        assert!(!ironclad_pool.contains(&CardId::Acrobatics));
    }

    #[test]
    fn power_potion_uses_reversed_java_source_pool_order() {
        let mut combat_state = blank_test_combat();
        combat_state.rng.card_random_rng = crate::runtime::rng::StsRng::from_state(
            0x8182_FDA8_6E79_9352,
            0x3D32_09D9_FF1E_FF5E,
            0,
        );

        assert_eq!(
            generate_discovery_choices(
                &mut combat_state,
                false,
                Some(crate::content::cards::CardType::Power),
            ),
            vec![CardId::Juggernaut, CardId::Combust, CardId::Berserk]
        );
    }

    #[test]
    fn attack_potion_matches_java_choices_without_healing_cards() {
        let mut combat_state = blank_test_combat();
        // Independent raw Java capture: AttackPotion -> DiscoveryAction(ATTACK).
        // The original pool has 28 cards, excluding Feed and Reaper.
        combat_state.rng.card_random_rng = crate::runtime::rng::StsRng::from_state(
            0x0F66_04AF_9BD6_8084,
            0x26F7_4616_462E_78AB,
            0,
        );
        assert_eq!(
            generate_discovery_choices(
                &mut combat_state,
                false,
                Some(crate::content::cards::CardType::Attack)
            ),
            vec![
                CardId::SwordBoomerang,
                CardId::RecklessCharge,
                CardId::BloodForBlood
            ]
        );
        assert_eq!(combat_state.rng.card_random_rng.counter, 3);
    }

    #[test]
    fn combat_discard_potion_input_respects_java_can_discard_affordance() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
        combat_state.entities.potions = vec![Some(
            crate::content::potions::Potion::with_affordance_truth(
                crate::content::potions::PotionId::FirePotion,
                1,
                true,
                false,
                true,
            ),
        )];
        let mut engine_state = EngineState::CombatPlayerTurn;

        let alive = super::tick_until_stable_turn(
            &mut engine_state,
            &mut combat_state,
            ClientInput::DiscardPotion(0),
        );

        assert!(alive);
        assert!(combat_state.entities.potions[0].is_some());
        let diagnostics = combat_state.take_engine_diagnostics();
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("Potion cannot be discarded")),
            "combat discard input should be rejected instead of bypassing Java canDiscard"
        );
    }

    #[test]
    fn pending_hand_selection_potion_discard_preserves_the_selection_boundary() {
        let mut combat_state = blank_test_combat();
        let selected = crate::runtime::combat::CombatCard::for_scenario(CardId::Defend, 41);
        let selected_lineage = selected.lineage;
        combat_state.zones.hand = vec![selected];
        combat_state.entities.potions = vec![Some(
            crate::content::potions::Potion::with_affordance_truth(
                crate::content::potions::PotionId::EnergyPotion,
                71,
                true,
                true,
                false,
            ),
        )];
        let choice = crate::state::core::PendingChoice::HandSelect {
            candidate_lineages: vec![selected_lineage],
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: crate::state::core::HandSelectReason::Exhaust,
        };
        let mut engine_state = EngineState::PendingChoice(choice.clone());

        let alive = super::tick_until_stable_turn(
            &mut engine_state,
            &mut combat_state,
            ClientInput::DiscardPotion(0),
        );

        assert!(alive);
        assert_eq!(engine_state, EngineState::PendingChoice(choice));
        assert_eq!(combat_state.zones.hand.len(), 1);
        assert_eq!(combat_state.zones.hand[0].lineage, selected_lineage);
        assert!(combat_state.entities.potions[0].is_none());
        assert!(!combat_state.has_pending_actions());
    }

    #[test]
    fn nonpositive_cost_relic_play_blue_candle_spends_zero_energy_like_java() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat_state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::BlueCandle));
        combat_state.turn.energy = 4;
        combat_state.zones.hand = vec![CombatCard::for_scenario(CardId::Writhe, 90_001)];
        let mut engine_state = EngineState::CombatPlayerTurn;

        let alive = super::tick_until_stable_turn(
            &mut engine_state,
            &mut combat_state,
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
        );

        assert!(alive);
        assert_eq!(combat_state.turn.energy, 4);
        assert_eq!(combat_state.entities.player.current_hp, 79);
        assert!(combat_state
            .zones
            .exhaust_pile
            .iter()
            .any(|card| card.id == CardId::Writhe));
    }

    #[test]
    fn nonpositive_cost_relic_play_medical_kit_spends_zero_energy_like_java() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat_state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::MedicalKit));
        combat_state.turn.energy = 4;
        combat_state.zones.hand = vec![CombatCard::for_scenario(CardId::Burn, 90_002)];
        let mut engine_state = EngineState::CombatPlayerTurn;

        let alive = super::tick_until_stable_turn(
            &mut engine_state,
            &mut combat_state,
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
        );

        assert!(alive);
        assert_eq!(combat_state.turn.energy, 4);
        assert_eq!(combat_state.entities.player.current_hp, 80);
        assert!(combat_state
            .zones
            .exhaust_pile
            .iter()
            .any(|card| card.id == CardId::Burn));
    }

    #[test]
    fn time_warp_power_card_trigger_forces_stable_turn_transition() {
        let mut combat_state = blank_test_combat();
        let mut monster = planned_monster(EnemyId::JawWorm, 1);
        monster.id = 1;
        combat_state.entities.monsters = vec![monster];
        combat_state.entities.power_db.insert(
            1,
            vec![Power {
                power_type: PowerId::TimeWarp,
                instance_id: None,
                amount: 11,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        combat_state.zones.hand = vec![CombatCard::for_scenario(CardId::Evolve, 10001)];
        let mut engine_state = EngineState::CombatPlayerTurn;

        let alive = super::tick_until_stable_turn(
            &mut engine_state,
            &mut combat_state,
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
        );

        assert!(alive);
        assert_eq!(engine_state, EngineState::CombatPlayerTurn);
        assert!(
            !combat_state.turn.counters.early_end_turn_pending,
            "Time Warp triggered by a Power card must be consumed before exposing the next player boundary"
        );
        assert_eq!(
            crate::content::powers::store::power_amount(&combat_state, 1, PowerId::TimeWarp),
            0
        );
        assert_eq!(
            crate::content::powers::store::power_amount(&combat_state, 1, PowerId::Strength),
            2
        );
        assert_eq!(combat_state.turn.counters.cards_played_this_turn, 0);
    }

    #[test]
    fn foreign_influence_any_color_attack_pool_matches_java_shape() {
        let common = any_color_attack_pool_sorted(crate::content::cards::CardRarity::Common);
        assert!(common.contains(&CardId::PommelStrike));
        assert!(common.contains(&CardId::QuickSlash));
        assert!(common.contains(&CardId::BeamCell));
        assert!(common.contains(&CardId::BowlingBash));
        assert!(common
            .windows(2)
            .all(|pair| crate::content::cards::java_id(pair[0])
                <= crate::content::cards::java_id(pair[1])));

        let uncommon = any_color_attack_pool_sorted(crate::content::cards::CardRarity::Uncommon);
        assert!(uncommon.contains(&CardId::FlashOfSteel));
        assert!(uncommon.contains(&CardId::Tantrum));

        let rare = any_color_attack_pool_sorted(crate::content::cards::CardRarity::Rare);
        assert!(rare.contains(&CardId::HandOfGreed));
        assert!(!rare.contains(&CardId::Feed));
    }

    #[test]
    fn foreign_influence_generation_uses_java_rng_sources() {
        let mut combat_state = blank_test_combat();
        let card_random_before = combat_state.rng.card_random_rng.counter;
        let card_before = combat_state.rng.card_rng.counter;

        let choices = generate_foreign_influence_choices(&mut combat_state);

        assert_eq!(choices.len(), 3);
        assert!(choices
            .iter()
            .enumerate()
            .all(|(idx, id)| !choices[..idx].contains(id)));
        assert!(
            combat_state.rng.card_random_rng.counter >= card_random_before + 6,
            "each Java ForeignInfluence candidate consumes rarity roll + getAnyColorCard shuffle seed"
        );
        assert!(
            combat_state.rng.card_rng.counter >= card_before + 3,
            "CardGroup.getRandomCard(true, rarity) selects with AbstractDungeon.cardRng"
        );
        for id in choices {
            let def = crate::content::cards::get_card_definition(id);
            assert_eq!(def.card_type, crate::content::cards::CardType::Attack);
            assert!(!def.tags.contains(&crate::content::cards::CardTag::Healing));
        }
    }

    #[test]
    fn foreign_influence_selection_matches_java_hand_and_discard_effect_paths() {
        let mut hand_state = blank_test_combat();
        hand_state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::MasterRealityPower,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        let mut hand_engine = EngineState::PendingChoice(PendingChoice::ForeignInfluenceSelect {
            cards: vec![CardId::SearingBlow],
            upgraded: true,
        });

        resolve_pending_choice(
            &mut hand_engine,
            &mut hand_state,
            ClientInput::SubmitDiscoverChoice(0),
        )
        .expect("valid Foreign Influence choice should resolve");

        assert_eq!(hand_engine, EngineState::CombatProcessing);
        assert_eq!(hand_state.zones.hand.len(), 1);
        assert_eq!(hand_state.zones.hand[0].id, CardId::SearingBlow);
        assert_eq!(
            hand_state.zones.hand[0].upgrades, 1,
            "ShowCardAndAddToHandEffect upgrades the actual generated card under Master Reality"
        );
        assert_eq!(
            hand_state.zones.hand[0].get_cost(),
            0,
            "ForeignInfluence+ applies setCostForTurn(0) to the selected copy"
        );

        let mut discard_state = blank_test_combat();
        discard_state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::MasterRealityPower,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        discard_state.zones.hand = (0..10)
            .map(|uuid| CombatCard::for_scenario(CardId::Strike, 10_000 + uuid))
            .collect();
        let mut discard_engine =
            EngineState::PendingChoice(PendingChoice::ForeignInfluenceSelect {
                cards: vec![CardId::SearingBlow],
                upgraded: true,
            });

        resolve_pending_choice(
            &mut discard_engine,
            &mut discard_state,
            ClientInput::SubmitDiscoverChoice(0),
        )
        .expect("valid full-hand Foreign Influence choice should resolve");

        assert_eq!(discard_state.zones.discard_pile.len(), 1);
        assert_eq!(discard_state.zones.discard_pile[0].id, CardId::SearingBlow);
        assert_eq!(
            discard_state.zones.discard_pile[0].upgrades, 0,
            "ForeignInfluenceAction uses ShowCardAndAddToDiscardEffect(src, x, y), whose Java constructor upgrades only the visual copy"
        );
        assert_eq!(discard_state.zones.discard_pile[0].get_cost(), 0);
    }

    #[test]
    fn divinity_returns_to_neutral_at_start_of_next_player_turn() {
        let mut combat_state = blank_test_combat();
        let mut engine_state = EngineState::CombatProcessing;
        combat_state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 3)];
        combat_state.entities.player.stance = StanceId::Divinity;
        combat_state.turn.begin_turn_transition();

        for _ in 0..64 {
            if engine_state == EngineState::CombatPlayerTurn {
                break;
            }
            assert!(super::tick_engine(
                &mut engine_state,
                &mut combat_state,
                None
            ));
        }

        assert_eq!(
            combat_state.entities.player.stance,
            StanceId::Neutral,
            "Java DivinityStance.atStartOfTurn queues ChangeStanceAction(\"Neutral\") at the next player turn start"
        );
    }

    #[test]
    fn seek_grid_candidates_match_java_better_draw_pile_sort_order() {
        let mut combat_state = blank_test_combat();
        let mut engine_state = EngineState::CombatProcessing;
        combat_state.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::StrikeB, 10),
            CombatCard::for_scenario(CardId::DefendB, 20),
            CombatCard::for_scenario(CardId::Bash, 30),
        ])
        .into();
        combat_state.queue_action_back(Action::SuspendForGridSelect {
            source_pile: PileType::Draw,
            min: 1,
            max: 1,
            can_cancel: false,
            filter: GridSelectFilter::Any,
            reason: GridSelectReason::DrawPileToHand,
        });

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));

        match engine_state {
            EngineState::PendingChoice(PendingChoice::GridSelect {
                candidate_lineages, ..
            }) => assert_eq!(
                candidate_lineages,
                [30, 20, 10]
                    .map(crate::test_support::scenario_lineage)
                    .to_vec(),
                "Java BetterDrawPileToHandAction sorts the temporary draw-pile group before opening grid select"
            ),
            other => panic!("Seek-style grid select should remain pending, got {other:?}"),
        }
    }

    #[test]
    fn omniscience_single_candidate_still_opens_grid_select_like_java() {
        let mut combat_state = blank_test_combat();
        let mut engine_state = EngineState::CombatProcessing;
        combat_state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::StrikeP, 10)]).into();
        combat_state.queue_action_back(Action::SuspendForGridSelect {
            source_pile: PileType::Draw,
            min: 1,
            max: 1,
            can_cancel: false,
            filter: GridSelectFilter::Any,
            reason: GridSelectReason::Omniscience { play_amount: 2 },
        });

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));

        match engine_state {
            EngineState::PendingChoice(PendingChoice::GridSelect {
                candidate_lineages,
                reason,
                ..
            }) => {
                assert_eq!(candidate_lineages, vec![crate::test_support::scenario_lineage(10)]);
                assert_eq!(reason, GridSelectReason::Omniscience { play_amount: 2 });
            }
            other => panic!(
                "Java OmniscienceAction opens grid select even with one draw-pile card, got {other:?}"
            ),
        }
        assert!(
            combat_state.pop_next_action().is_none(),
            "Omniscience must wait for grid selection instead of auto-moving the only candidate"
        );
    }

    #[test]
    fn omniscience_selection_removes_draw_card_and_queues_autoplay_copies() {
        let mut combat_state = blank_test_combat();
        combat_state.turn.energy = 1;
        let mut selected = CombatCard::for_scenario(CardId::StrikeP, 10);
        selected.upgrades = 1;
        selected.misc_value = 4;
        selected.base_damage_override = Some(17);
        selected.base_damage_mut = 99;
        selected.free_to_play_once = true;
        combat_state.zones.draw_pile = (vec![selected]).into();
        let mut engine_state = EngineState::PendingChoice(PendingChoice::GridSelect {
            source_pile: PileType::Draw,
            candidate_lineages: vec![crate::runtime::combat::CardLineageId::constructed_scenario(
                10,
            )],
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: GridSelectReason::Omniscience { play_amount: 3 },
        });

        resolve_pending_choice(
            &mut engine_state,
            &mut combat_state,
            ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Grid,
                    vec![crate::test_support::scenario_lineage(10)],
                ),
            ),
        )
        .expect("valid Omniscience grid selection should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert!(combat_state.zones.draw_pile.is_empty());

        let first = combat_state
            .pop_next_action()
            .expect("Omniscience should queue the selected card first");
        match first {
            Action::EnqueueCardPlay { item, in_front } => {
                assert!(!in_front);
                assert_eq!(item.card.id, CardId::StrikeP);
                assert_eq!(
                    item.card.lineage,
                    crate::runtime::combat::CardLineageId::constructed_scenario(10)
                );
                assert_eq!(item.card.upgrades, 1);
                assert_eq!(item.card.misc_value, 4);
                assert_eq!(item.card.base_damage_override, Some(17));
                assert_eq!(
                    item.card.base_damage_mut, 99,
                    "Java Omniscience queues the selected original first, not a reset copy"
                );
                assert!(item.card.free_to_play_once);
                assert_eq!(item.card.exhaust_override, Some(true));
                assert_eq!(item.energy_on_use, 1);
                assert!(item.autoplay);
                assert!(item.random_target);
                assert!(!item.purge_on_use);
            }
            other => panic!("expected first Omniscience queued play, got {other:?}"),
        }

        let second = combat_state
            .pop_next_action()
            .expect("Omniscience should queue the first purge-on-use stat-equivalent copy");
        match second {
            Action::EnqueueCardPlay { item, in_front } => {
                assert!(!in_front);
                assert_eq!(item.card.id, CardId::StrikeP);
                assert_eq!(
                    item.card.lineage.origin(),
                    crate::runtime::combat::CardLineageOrigin::CombatGenerated
                );
                assert_eq!(item.card.upgrades, 1);
                assert_eq!(item.card.misc_value, 4);
                assert_eq!(item.card.base_damage_override, Some(17));
                assert_eq!(
                    item.card.base_damage_mut, 0,
                    "Java Omniscience extra plays use makeStatEquivalentCopy, not the rendered damage"
                );
                assert!(item.card.free_to_play_once);
                assert_eq!(item.card.exhaust_override, None);
                assert_eq!(item.energy_on_use, 1);
                assert!(!item.ignore_energy_total);
                assert!(item.autoplay);
                assert!(item.random_target);
                assert!(item.purge_on_use);
            }
            other => panic!("expected second Omniscience queued play, got {other:?}"),
        }

        let third = combat_state
            .pop_next_action()
            .expect("Omniscience should queue play_amount - 1 purge-on-use copies");
        match third {
            Action::EnqueueCardPlay { item, in_front } => {
                assert!(!in_front);
                assert_eq!(item.card.id, CardId::StrikeP);
                assert_eq!(
                    item.card.lineage.origin(),
                    crate::runtime::combat::CardLineageOrigin::CombatGenerated
                );
                assert_eq!(item.card.upgrades, 1);
                assert_eq!(item.card.misc_value, 4);
                assert_eq!(item.card.base_damage_override, Some(17));
                assert_eq!(item.card.base_damage_mut, 0);
                assert!(item.card.free_to_play_once);
                assert_eq!(item.card.exhaust_override, None);
                assert_eq!(item.energy_on_use, 1);
                assert!(!item.ignore_energy_total);
                assert!(item.autoplay);
                assert!(item.random_target);
                assert!(item.purge_on_use);
            }
            other => panic!("expected third Omniscience queued play, got {other:?}"),
        }
        assert_eq!(
            combat_state.pop_next_action(),
            None,
            "Java OmniscienceAction queues the selected original once plus playAmt - 1 copies"
        );
    }

    #[test]
    fn choose_one_selection_queues_selected_option_callback() {
        let mut combat_state = blank_test_combat();
        let mut engine_state = EngineState::PendingChoice(PendingChoice::ChooseOneSelect {
            choices: vec![
                crate::state::ChooseOneCardChoice {
                    card_id: CardId::BecomeAlmighty,
                    upgrades: 1,
                },
                crate::state::ChooseOneCardChoice {
                    card_id: CardId::FameAndFortune,
                    upgrades: 1,
                },
                crate::state::ChooseOneCardChoice {
                    card_id: CardId::LiveForever,
                    upgrades: 1,
                },
            ],
        });

        resolve_pending_choice(
            &mut engine_state,
            &mut combat_state,
            ClientInput::SubmitDiscoverChoice(2),
        )
        .expect("valid choose-one selection should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(
            combat_state.pop_next_action(),
            Some(Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::PlatedArmor,
                amount: 8,
            }),
            "Wish's LiveForever+ option should run the Java onChoseThisOption callback"
        );
    }

    #[test]
    fn empty_deck_to_hand_grid_with_wrong_source_or_filter_remains_broken() {
        for (source_pile, filter, reason) in [
            (
                PileType::Discard,
                GridSelectFilter::Any,
                GridSelectReason::DrawPileToHand,
            ),
            (
                PileType::Draw,
                GridSelectFilter::Skill,
                GridSelectReason::DrawPileToHand,
            ),
            (
                PileType::Discard,
                GridSelectFilter::Skill,
                GridSelectReason::SkillFromDeckToHand,
            ),
            (
                PileType::Draw,
                GridSelectFilter::Attack,
                GridSelectReason::SkillFromDeckToHand,
            ),
            (
                PileType::Exhaust,
                GridSelectFilter::Attack,
                GridSelectReason::AttackFromDeckToHand,
            ),
            (
                PileType::Draw,
                GridSelectFilter::Skill,
                GridSelectReason::AttackFromDeckToHand,
            ),
        ] {
            let mut combat_state = blank_test_combat();
            let mut engine_state = EngineState::CombatProcessing;
            combat_state.queue_action_back(Action::SuspendForGridSelect {
                source_pile,
                min: 1,
                max: 1,
                can_cancel: false,
                filter,
                reason,
            });
            assert!(super::tick_engine(
                &mut engine_state,
                &mut combat_state,
                None
            ));
            assert!(combat_state.take_engine_diagnostics().iter().any(|diagnostic| {
                diagnostic.severity == crate::state::selection::EngineDiagnosticSeverity::Error
                    && diagnostic.class == crate::state::selection::EngineDiagnosticClass::Broken
            }), "invalid {reason:?} on {source_pile:?} with {filter:?} must not become a legal empty effect");
        }
    }

    #[test]
    fn optional_deck_to_hand_keeps_its_grid_even_when_the_whole_pile_fits() {
        let mut combat_state = blank_test_combat();
        let mut engine_state = EngineState::CombatProcessing;
        combat_state.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 20),
        ]
        .into();
        combat_state.queue_action_back(Action::SuspendForGridSelect {
            source_pile: PileType::Draw,
            min: 0,
            max: 2,
            can_cancel: true,
            filter: GridSelectFilter::Any,
            reason: GridSelectReason::DrawPileToHand,
        });
        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));
        assert!(matches!(
            engine_state,
            EngineState::PendingChoice(PendingChoice::GridSelect {
                min_cards: 0,
                max_cards: 2,
                can_cancel: true,
                ..
            })
        ));
        assert_eq!(combat_state.zones.draw_pile.len(), 2);
        assert!(combat_state.zones.hand.is_empty());
    }

    #[test]
    fn secret_technique_grid_candidates_consume_java_add_to_random_spot_rng() {
        let mut combat_state = blank_test_combat();
        let mut engine_state = EngineState::CombatProcessing;
        combat_state.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::StrikeB, 10),
            CombatCard::for_scenario(CardId::DefendB, 20),
            CombatCard::for_scenario(CardId::Seek, 30),
            CombatCard::for_scenario(CardId::DefendG, 40),
        ])
        .into();

        let mut expected_rng = combat_state.rng.clone();
        let mut expected_candidates = Vec::new();
        for uuid in [40, 30, 20].map(crate::test_support::scenario_lineage) {
            if expected_candidates.is_empty() {
                expected_candidates.push(uuid);
            } else {
                let index = expected_rng
                    .card_random_rng
                    .random(expected_candidates.len() as i32 - 1)
                    as usize;
                expected_candidates.insert(index, uuid);
            }
        }

        combat_state.queue_action_back(Action::SuspendForGridSelect {
            source_pile: PileType::Draw,
            min: 1,
            max: 1,
            can_cancel: false,
            filter: GridSelectFilter::Skill,
            reason: GridSelectReason::SkillFromDeckToHand,
        });

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));

        match engine_state {
            EngineState::PendingChoice(PendingChoice::GridSelect {
                candidate_lineages, ..
            }) => assert_eq!(
                candidate_lineages, expected_candidates,
                "Java SkillFromDeckToHandAction builds its temporary group with addToRandomSpot"
            ),
            other => {
                panic!("Secret Technique-style grid select should remain pending, got {other:?}")
            }
        }
        assert_eq!(
            combat_state.rng.card_random_rng.counter, expected_rng.card_random_rng.counter,
            "opening Secret Technique's multi-candidate grid select consumes cardRandomRng"
        );
    }

    #[test]
    fn discovery_resume_burns_java_unused_choice_rng() {
        let mut combat_state = blank_test_combat();
        let mut engine_state = EngineState::CombatProcessing;
        combat_state.queue_action_back(crate::runtime::action::Action::SuspendForDiscovery {
            colorless: false,
            card_type: None,
            amount: 1,
            cost_for_turn: Some(0),
            can_skip: false,
        });

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));
        let counter_after_open = combat_state.rng.card_random_rng.counter;
        let selected_id = match &engine_state {
            EngineState::PendingChoice(PendingChoice::DiscoverySelect(choice)) => {
                assert!(!choice.can_skip);
                assert_eq!(choice.cards.len(), 3);
                choice.cards[0]
            }
            other => panic!("DiscoveryAction should open a discovery choice, got {other:?}"),
        };

        resolve_pending_choice(
            &mut engine_state,
            &mut combat_state,
            ClientInput::SubmitDiscoverChoice(0),
        )
        .expect("valid discovery choice should resolve");

        assert!(
            combat_state.rng.card_random_rng.counter >= counter_after_open + 3,
            "Java DiscoveryAction.update regenerates an unused choice set when the action resumes"
        );
        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(combat_state.zones.hand.len(), 1);
        assert_eq!(combat_state.zones.hand[0].id, selected_id);
    }

    #[test]
    fn discovery_choice_count_is_unaffected_by_card_reward_relics() {
        let mut combat = blank_test_combat();
        for relic in [
            RelicId::BustedCrown,
            RelicId::QuestionCard,
            RelicId::PrayerWheel,
        ] {
            combat.entities.player.add_relic(RelicState::new(relic));
        }
        let mut engine = EngineState::CombatProcessing;
        combat.queue_action_back(Action::SuspendForDiscovery {
            colorless: false,
            card_type: None,
            amount: 1,
            cost_for_turn: Some(0),
            can_skip: false,
        });
        assert!(super::tick_engine(&mut engine, &mut combat, None));
        let EngineState::PendingChoice(PendingChoice::DiscoverySelect(choice)) = engine else {
            panic!("expected Discovery choice");
        };
        assert_eq!(choice.cards.len(), 3);
    }

    #[test]
    fn typed_discovery_choice_can_skip_and_still_burns_resume_rng() {
        let mut combat_state = blank_test_combat();
        let mut engine_state = EngineState::CombatProcessing;
        combat_state.queue_action_back(crate::runtime::action::Action::SuspendForDiscovery {
            colorless: false,
            card_type: Some(crate::content::cards::CardType::Attack),
            amount: 1,
            cost_for_turn: Some(0),
            can_skip: true,
        });

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));
        let counter_after_open = combat_state.rng.card_random_rng.counter;
        match &engine_state {
            EngineState::PendingChoice(PendingChoice::DiscoverySelect(choice)) => {
                assert!(choice.can_skip);
                assert_eq!(
                    choice.card_type,
                    Some(crate::content::cards::CardType::Attack)
                );
                assert_eq!(choice.cards.len(), 3);
            }
            other => panic!("typed DiscoveryAction should open a skippable choice, got {other:?}"),
        }

        resolve_pending_choice(&mut engine_state, &mut combat_state, ClientInput::Cancel)
            .expect("skippable typed discovery choice should accept cancel");

        assert!(
            combat_state.rng.card_random_rng.counter >= counter_after_open + 3,
            "Java typed DiscoveryAction burns the resume-time generated choice set even when skipped"
        );
        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert!(combat_state.zones.hand.is_empty());
        assert_eq!(combat_state.turn.take_discovery_cost_for_turn(), None);
    }

    #[test]
    fn sacred_bark_discovery_potion_adds_two_selected_copies_with_java_hand_capacity_split() {
        let mut combat_state = blank_test_combat();
        combat_state.zones.hand = (0..9)
            .map(|idx| CombatCard::for_scenario(CardId::Defend, 10 + idx))
            .collect();
        let mut engine_state = EngineState::CombatProcessing;
        combat_state.queue_action_back(crate::runtime::action::Action::SuspendForDiscovery {
            colorless: false,
            card_type: Some(crate::content::cards::CardType::Attack),
            amount: 2,
            cost_for_turn: Some(0),
            can_skip: true,
        });

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));
        let selected_id = match &engine_state {
            EngineState::PendingChoice(PendingChoice::DiscoverySelect(choice)) => {
                assert_eq!(choice.amount, 2);
                choice.cards[0]
            }
            other => {
                panic!("Sacred Bark potion DiscoveryAction should open a choice, got {other:?}")
            }
        };

        resolve_pending_choice(
            &mut engine_state,
            &mut combat_state,
            ClientInput::SubmitDiscoverChoice(0),
        )
        .expect("valid discovery choice should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(combat_state.zones.hand.len(), 10);
        assert_eq!(combat_state.zones.discard_pile.len(), 1);
        assert_eq!(combat_state.zones.hand[9].id, selected_id);
        assert_eq!(combat_state.zones.discard_pile[0].id, selected_id);
        assert_eq!(combat_state.zones.hand[9].cost_for_turn_java(), 0);
        assert_eq!(combat_state.zones.discard_pile[0].cost_for_turn_java(), 0);
    }

    #[test]
    fn discovery_selection_uses_java_make_copy_and_master_reality_path() {
        let mut combat_state = blank_test_combat();
        combat_state.turn.counters.times_damaged_this_combat = 2;
        combat_state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::MasterRealityPower,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        let mut engine_state =
            EngineState::PendingChoice(PendingChoice::DiscoverySelect(DiscoveryChoiceState {
                cards: vec![CardId::BloodForBlood],
                colorless: false,
                card_type: None,
                amount: 1,
                can_skip: false,
            }));

        resolve_pending_choice(
            &mut engine_state,
            &mut combat_state,
            ClientInput::SubmitDiscoverChoice(0),
        )
        .expect("valid discovery choice should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(combat_state.zones.hand.len(), 1);
        let card = &combat_state.zones.hand[0];
        assert_eq!(card.id, CardId::BloodForBlood);
        assert_eq!(card.cost_modifier, -2);
        assert_eq!(
            card.get_cost(),
            1,
            "Java Discovery keeps Blood for Blood.makeCopy damagedThisCombat discount before Master Reality upgrades it"
        );
        assert_eq!(
            card.upgrades, 1,
            "Blood for Blood ignores the second Master Reality upgrade call because it is already upgraded"
        );
    }

    #[test]
    fn discovery_full_hand_discard_copy_gets_only_constructor_master_reality_upgrade() {
        let mut combat_state = blank_test_combat();
        combat_state.zones.hand = (0..9)
            .map(|idx| CombatCard::for_scenario(CardId::Defend, 10 + idx))
            .collect();
        combat_state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::MasterRealityPower,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        let mut engine_state =
            EngineState::PendingChoice(PendingChoice::DiscoverySelect(DiscoveryChoiceState {
                cards: vec![CardId::SearingBlow],
                colorless: false,
                card_type: None,
                amount: 2,
                can_skip: false,
            }));

        resolve_pending_choice(
            &mut engine_state,
            &mut combat_state,
            ClientInput::SubmitDiscoverChoice(0),
        )
        .expect("valid discovery choice should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(combat_state.zones.hand[9].id, CardId::SearingBlow);
        assert_eq!(
            combat_state.zones.hand[9].upgrades, 2,
            "Discovery hand copies get the explicit Master Reality upgrade and the ShowCardAndAddToHandEffect upgrade"
        );
        assert_eq!(combat_state.zones.discard_pile[0].id, CardId::SearingBlow);
        assert_eq!(
            combat_state.zones.discard_pile[0].upgrades, 1,
            "Discovery discard copies use ShowCardAndAddToDiscardEffect(src, x, y), whose second Master Reality upgrade applies only to a visual copy"
        );
    }

    #[test]
    fn pending_choice_generated_cards_use_independent_combat_lineages() {
        let mut combat_state = blank_test_combat();

        let mut first_choice =
            EngineState::PendingChoice(PendingChoice::DiscoverySelect(DiscoveryChoiceState {
                cards: vec![CardId::Strike],
                colorless: false,
                card_type: None,
                amount: 1,
                can_skip: false,
            }));
        resolve_pending_choice(
            &mut first_choice,
            &mut combat_state,
            ClientInput::SubmitDiscoverChoice(0),
        )
        .expect("first discovery choice should resolve");

        let mut second_choice =
            EngineState::PendingChoice(PendingChoice::DiscoverySelect(DiscoveryChoiceState {
                cards: vec![CardId::Defend],
                colorless: false,
                card_type: None,
                amount: 1,
                can_skip: false,
            }));
        resolve_pending_choice(
            &mut second_choice,
            &mut combat_state,
            ClientInput::SubmitDiscoverChoice(0),
        )
        .expect("second discovery choice should resolve");

        assert_eq!(
            combat_state.zones.hand[0].lineage,
            crate::runtime::combat::CardLineageId::combat_generated(1)
        );
        assert_eq!(
            combat_state.zones.hand[1].lineage,
            crate::runtime::combat::CardLineageId::combat_generated(2)
        );
        assert_eq!(
            combat_state
                .zones
                .combat_lineage_allocator
                .next_creation_ordinal(),
            3
        );
    }

    #[test]
    fn card_reward_selection_preserves_codex_master_reality_single_draw_path() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::MasterRealityPower,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        let mut engine_state = EngineState::PendingChoice(PendingChoice::CardRewardSelect {
            cards: vec![CardId::SearingBlow],
            destination: CardDestination::DrawPileRandom,
            can_skip: true,
        });

        resolve_pending_choice(
            &mut engine_state,
            &mut combat_state,
            ClientInput::SubmitDiscoverChoice(0),
        )
        .expect("valid Codex-style choice should resolve");

        assert_eq!(engine_state, EngineState::CombatProcessing);
        assert_eq!(combat_state.zones.draw_pile.len(), 1);
        assert_eq!(combat_state.zones.draw_pile[0].id, CardId::SearingBlow);
        assert_eq!(
            combat_state.zones.draw_pile[0].upgrades, 1,
            "Java CodexAction relies on ShowCardAndAddToDrawPileEffect for one Master Reality upgrade"
        );
    }

    #[test]
    fn card_reward_hand_destination_master_reality_branch_counts_match_java_effects() {
        let mut hand_state = blank_test_combat();
        hand_state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::MasterRealityPower,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        let mut hand_engine = EngineState::PendingChoice(PendingChoice::CardRewardSelect {
            cards: vec![CardId::SearingBlow],
            destination: CardDestination::Hand,
            can_skip: false,
        });

        resolve_pending_choice(
            &mut hand_engine,
            &mut hand_state,
            ClientInput::SubmitDiscoverChoice(0),
        )
        .expect("valid hand reward choice should resolve");

        assert_eq!(hand_state.zones.hand[0].id, CardId::SearingBlow);
        assert_eq!(
            hand_state.zones.hand[0].upgrades, 2,
            "ChooseOneColorless hand path gets the explicit Master Reality upgrade and the ShowCardAndAddToHandEffect upgrade"
        );

        let mut discard_state = blank_test_combat();
        discard_state.zones.hand = (0..10)
            .map(|idx| CombatCard::for_scenario(CardId::Defend, 1_000 + idx))
            .collect();
        discard_state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::MasterRealityPower,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        let mut discard_engine = EngineState::PendingChoice(PendingChoice::CardRewardSelect {
            cards: vec![CardId::SearingBlow],
            destination: CardDestination::Hand,
            can_skip: false,
        });

        resolve_pending_choice(
            &mut discard_engine,
            &mut discard_state,
            ClientInput::SubmitDiscoverChoice(0),
        )
        .expect("valid full-hand reward choice should resolve");

        assert_eq!(discard_state.zones.discard_pile[0].id, CardId::SearingBlow);
        assert_eq!(
            discard_state.zones.discard_pile[0].upgrades, 1,
            "ChooseOneColorless discard path keeps only the explicit Master Reality upgrade; the discard effect's extra upgrade is visual-only"
        );
    }

    #[test]
    fn colorless_card_reward_uses_java_random_colorless_combat_pool_order() {
        let mut combat_state = blank_test_combat();
        let pool = crate::content::cards::random_colorless_in_combat_pool();
        let mut expected_rng = combat_state.rng.card_random_rng.clone();
        let mut expected = Vec::new();
        while expected.len() < 3 {
            let idx = expected_rng.random(pool.len() as i32 - 1) as usize;
            let id = pool[idx];
            if !expected.contains(&id) {
                expected.push(id);
            }
        }

        combat_state.queue_action_back(Action::SuspendForCardReward {
            pool: CardRewardPool::Colorless,
            destination: CardDestination::Hand,
            can_skip: false,
            skip_if_monsters_basically_dead: false,
        });
        let mut engine_state = EngineState::CombatProcessing;
        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));

        let EngineState::PendingChoice(PendingChoice::CardRewardSelect {
            cards,
            destination,
            can_skip,
        }) = engine_state
        else {
            panic!("Toolbox-style colorless reward should open a card reward selection");
        };
        assert_eq!(cards, expected);
        assert_eq!(destination, CardDestination::Hand);
        assert!(!can_skip);
        assert_eq!(
            combat_state.rng.card_random_rng.counter, expected_rng.counter,
            "Java ChooseOneColorless consumes cardRandomRng against srcColorlessCardPool order, not rarity-grouped colorless pools"
        );
    }

    #[test]
    fn turn_transition_retains_selected_cards_once_and_clears_flag() {
        let mut combat_state = blank_test_combat();
        let mut retained = CombatCard::for_scenario(CardId::Defend, 20);
        retained.retain_override = Some(true);
        combat_state.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10), retained];

        discard_hand_for_turn_transition(&mut combat_state);

        assert_eq!(
            combat_state
                .zones
                .hand
                .iter()
                .map(|card| (card.id, card.lineage, card.retain_override))
                .collect::<Vec<_>>(),
            vec![(
                CardId::Defend,
                crate::test_support::scenario_lineage(20),
                None,
            )],
            "Java RestoreRetainedCardsAction clears AbstractCard.retain after preserving the card"
        );
        assert_eq!(
            combat_state
                .zones
                .discard_pile
                .iter()
                .map(|card| (card.id, card.lineage))
                .collect::<Vec<_>>(),
            vec![(CardId::Strike, crate::test_support::scenario_lineage(10))]
        );
    }

    #[test]
    fn end_turn_trigger_resets_temporary_costs_in_java_active_piles_before_discard() {
        let mut combat_state = blank_test_combat();
        let mut draw = CombatCard::for_scenario(CardId::Bash, 10);
        draw.set_cost_for_turn_java(0);
        let mut discard = CombatCard::for_scenario(CardId::Strike, 20);
        discard.set_cost_for_turn_java(0);
        let mut retained = CombatCard::for_scenario(CardId::Defend, 30);
        retained.set_cost_for_turn_java(0);
        retained.retain_override = Some(true);
        combat_state.zones.draw_pile = vec![draw].into();
        combat_state.zones.discard_pile = vec![discard].into();
        combat_state.zones.hand = vec![retained];

        crate::engine::action_handlers::cards::handle_end_turn_trigger(&mut combat_state);

        assert_eq!(combat_state.zones.hand[0].cost_for_turn, Some(0));
        crate::engine::action_handlers::cards::handle_end_turn_post_cards(&mut combat_state);

        assert_eq!(combat_state.zones.draw_pile[0].cost_for_turn, None);
        assert_eq!(combat_state.zones.discard_pile[0].cost_for_turn, None);
        assert_eq!(combat_state.zones.hand[0].cost_for_turn, None);
        assert_eq!(
            combat_state.zones.hand[0].retain_override,
            Some(true),
            "Java resetAttributes does not consume the one-turn Retain flag before DiscardAtEndOfTurnAction"
        );
    }

    #[test]
    fn corruption_end_turn_discard_refreshes_only_cards_still_in_hand() {
        use crate::content::powers::{store, PowerId};
        use crate::runtime::combat::{Power, PowerPayload};
        let mut state = blank_test_combat();
        store::set_powers_for(
            &mut state,
            0,
            vec![Power {
                power_type: PowerId::Corruption,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        let mut later = CombatCard::for_scenario(CardId::InfernalBlade, 10);
        let mut first = CombatCard::for_scenario(CardId::Shockwave, 20);
        let mut retained = CombatCard::for_scenario(CardId::Defend, 30);
        for card in [&mut later, &mut first, &mut retained] {
            card.set_cost_for_turn_java(0);
        }
        retained.retain_override = Some(true);
        state.zones.hand = vec![later, first, retained];
        crate::engine::action_handlers::cards::handle_end_turn_trigger(&mut state);

        crate::engine::action_handlers::cards::handle_end_turn_post_cards(&mut state);
        discard_hand_for_turn_transition(&mut state);

        let costs: Vec<_> = state
            .zones
            .discard_pile
            .iter()
            .map(|card| {
                (
                    card.id,
                    card.combat_cost_without_turn_override_java(),
                    card.cost_for_turn_java(),
                )
            })
            .collect();
        assert_eq!(
            costs,
            [(CardId::Shockwave, 2, 2), (CardId::InfernalBlade, 0, 0)]
        );
        assert_eq!(state.zones.hand[0].id, CardId::Defend);
        assert_eq!(
            state.zones.hand[0].combat_cost_without_turn_override_java(),
            1
        );
        assert_eq!(state.zones.hand[0].cost_for_turn_java(), 1);
    }

    #[test]
    fn turn_transition_preserves_intrinsic_self_retain_cards() {
        let mut combat_state = blank_test_combat();
        combat_state.zones.hand = vec![
            CombatCard::for_scenario(CardId::StrikeP, 30),
            CombatCard::for_scenario(CardId::Insight, 31),
            CombatCard::for_scenario(CardId::Miracle, 32),
        ];

        discard_hand_for_turn_transition(&mut combat_state);

        assert_eq!(
            combat_state
                .zones
                .hand
                .iter()
                .map(|card| (card.id, card.lineage))
                .collect::<Vec<_>>(),
            vec![
                (CardId::Insight, crate::test_support::scenario_lineage(31)),
                (CardId::Miracle, crate::test_support::scenario_lineage(32)),
            ],
            "Java selfRetain cards remain in hand during end-of-turn discard"
        );
        assert_eq!(
            combat_state
                .zones
                .discard_pile
                .iter()
                .map(|card| (card.id, card.lineage))
                .collect::<Vec<_>>(),
            vec![(CardId::StrikeP, crate::test_support::scenario_lineage(30))]
        );
    }

    #[test]
    fn end_turn_burn_autoplay_precedes_hand_discard_and_preserves_java_reshuffle_order() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        combat_state.turn.mark_skip_monster_turn_pending();
        combat_state.rng.shuffle_rng = crate::runtime::rng::StsRng::from_state(
            0x59DF_7613_AE3E_2DEF,
            0xC9D1_3AD6_1C42_81E1,
            3,
        );

        let mut battle_trance = CombatCard::for_scenario(CardId::BattleTrance, 10_001);
        battle_trance.upgrades = 1;
        combat_state.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Defend, 6),
            CombatCard::for_scenario(CardId::Bash, 9),
            battle_trance,
            CombatCard::for_scenario(CardId::Metallicize, 10_006),
        ]
        .into();

        let mut old_strike_plus = CombatCard::for_scenario(CardId::Strike, 4);
        old_strike_plus.upgrades = 1;
        let mut pommel = CombatCard::for_scenario(CardId::PommelStrike, 10_008);
        pommel.upgrades = 1;
        let mut armaments = CombatCard::for_scenario(CardId::Armaments, 10_005);
        armaments.upgrades = 1;
        let mut second_wind = CombatCard::for_scenario(CardId::SecondWind, 10_009);
        second_wind.upgrades = 1;
        let mut body_slam = CombatCard::for_scenario(CardId::BodySlam, 10_010);
        body_slam.upgrades = 1;
        combat_state.zones.discard_pile = vec![
            pommel,
            armaments,
            second_wind,
            body_slam,
            old_strike_plus,
            CombatCard::for_scenario(CardId::ShrugItOff, 10_004),
            CombatCard::for_scenario(CardId::Defend, 8),
            CombatCard::for_scenario(CardId::Strike, 0),
        ]
        .into();

        let mut hand_strike_plus = CombatCard::for_scenario(CardId::Strike, 2);
        hand_strike_plus.upgrades = 1;
        combat_state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Burn, 10_013),
            CombatCard::for_scenario(CardId::Strike, 1),
            hand_strike_plus,
        ];
        let hp_before = combat_state.entities.player.current_hp;
        let mut engine_state = EngineState::CombatPlayerTurn;

        assert!(super::tick_until_stable_turn(
            &mut engine_state,
            &mut combat_state,
            ClientInput::EndTurn,
        ));

        assert_eq!(engine_state, EngineState::CombatPlayerTurn);
        assert_eq!(combat_state.entities.player.current_hp, hp_before - 2);
        assert_eq!(
            combat_state
                .zones
                .hand
                .iter()
                .map(|card| card.lineage)
                .collect::<Vec<_>>(),
            [6, 9, 10_001, 10_006, 10_010]
                .map(crate::test_support::scenario_lineage)
                .to_vec(),
            "the four old draw cards are followed by the Java reshuffle top card"
        );
        assert_eq!(
            combat_state
                .zones
                .draw_pile
                .iter()
                .map(|card| card.lineage)
                .collect::<Vec<_>>(),
            [10_013, 8, 10_005, 10_009, 10_004, 10_008, 1, 2, 4, 0]
                .map(crate::test_support::scenario_lineage)
                .to_vec(),
            "Burn is queued from the bottom of the live hand before the remaining hand discards top-to-bottom"
        );
        assert_eq!(combat_state.rng.shuffle_rng.counter, 4);
    }

    #[test]
    fn end_turn_triggered_autoplay_resolves_before_single_ethereal_exhaust() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        combat_state.turn.mark_skip_monster_turn_pending();
        combat_state.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Defend, 40)].into();
        combat_state.zones.hand = vec![
            CombatCard::for_scenario(CardId::GhostlyArmor, 10),
            CombatCard::for_scenario(CardId::Burn, 20),
            CombatCard::for_scenario(CardId::Strike, 30),
        ];
        let mut engine_state = EngineState::CombatPlayerTurn;

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            Some(ClientInput::EndTurn),
        ));

        let mut observed_java_stage_boundary = false;
        for _ in 0..64 {
            if combat_state.zones.discard_pile.iter().any(|card| {
                card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(20)
            }) && combat_state.zones.hand.iter().any(|card| {
                card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(10)
            }) {
                observed_java_stage_boundary = true;
            }
            if engine_state == EngineState::CombatPlayerTurn {
                break;
            }
            assert!(super::tick_engine(
                &mut engine_state,
                &mut combat_state,
                None,
            ));
        }

        assert!(
            observed_java_stage_boundary,
            "Burn must finish its cardQueue use before DiscardAtEndOfTurnAction processes Ethereal"
        );
        assert!(combat_state
            .zones
            .exhaust_pile
            .iter()
            .any(|card| card.lineage
                == crate::runtime::combat::CardLineageId::constructed_scenario(10)));
        assert!(!combat_state
            .zones
            .exhaust_pile
            .iter()
            .any(|card| card.lineage
                == crate::runtime::combat::CardLineageId::constructed_scenario(20)));
    }

    #[test]
    fn dead_branch_card_from_end_turn_ethereal_exhaust_survives_the_discard_pass() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        combat_state.turn.mark_skip_monster_turn_pending();
        combat_state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::DeadBranch));
        combat_state.zones.draw_pile = (40..45)
            .map(|lineage| CombatCard::for_scenario(CardId::Defend, lineage))
            .collect::<Vec<_>>()
            .into();
        combat_state.zones.hand = vec![
            CombatCard::for_scenario(CardId::AscendersBane, 10),
            CombatCard::for_scenario(CardId::Strike, 20),
        ];
        let mut engine_state = EngineState::CombatPlayerTurn;

        assert!(super::tick_until_stable_turn(
            &mut engine_state,
            &mut combat_state,
            ClientInput::EndTurn,
        ));

        assert_eq!(engine_state, EngineState::CombatPlayerTurn);
        assert_eq!(combat_state.zones.hand.len(), 6);
        assert!(combat_state.zones.exhaust_pile.iter().any(|card| {
            card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(10)
        }));
        assert!(combat_state.zones.discard_pile.iter().any(|card| {
            card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(20)
        }));
    }

    #[test]
    fn end_turn_damage_places_combust_after_cards_and_metallicize_before_cards() {
        use crate::content::powers::{store, PowerId};
        use crate::runtime::combat::{Power, PowerPayload};
        for regret in [false, true] {
            let mut state = blank_test_combat();
            state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
            state.turn.mark_skip_monster_turn_pending();
            state.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Defend, 40)].into();
            let power = |power_type, amount, extra_data| Power {
                power_type,
                amount,
                extra_data,
                instance_id: None,
                payload: PowerPayload::None,
                just_applied: false,
            };
            store::set_powers_for(
                &mut state,
                0,
                vec![
                    power(PowerId::Rupture, 2, 0),
                    power(PowerId::Combust, 5, 1),
                    if regret {
                        power(PowerId::Buffer, 1, 0)
                    } else {
                        power(PowerId::Metallicize, 2, 0)
                    },
                ],
            );
            state.zones.hand = if regret {
                vec![
                    CombatCard::for_scenario(CardId::Regret, 0),
                    CombatCard::for_scenario(CardId::Wound, 1),
                    CombatCard::for_scenario(CardId::Wound, 2),
                ]
            } else {
                vec![CombatCard::for_scenario(CardId::Burn, 0)]
            };
            let mut engine = EngineState::CombatPlayerTurn;
            assert!(super::tick_until_stable_turn(
                &mut engine,
                &mut state,
                ClientInput::EndTurn
            ));
            assert_eq!(state.entities.player.current_hp, 79, "regret={regret}");
            assert_eq!(state.get_power(0, PowerId::Strength), 2);
            assert_eq!(state.get_power(0, PowerId::Buffer), 0);
        }
    }

    #[test]
    fn end_turn_power_selection_resumes_without_repeating_damage() {
        use crate::content::powers::{store, PowerId};
        use crate::runtime::combat::{Power, PowerPayload};
        use crate::state::selection::{SelectionResolution, SelectionScope};
        let mut state = blank_test_combat();
        state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        state.turn.mark_skip_monster_turn_pending();
        state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 0),
            CombatCard::for_scenario(CardId::Defend, 1),
        ];
        let keep = state.zones.hand[0].lineage;
        store::set_powers_for(
            &mut state,
            0,
            [(PowerId::Combust, 5, 1), (PowerId::RetainCards, 1, 0)]
                .into_iter()
                .map(|(power_type, amount, extra_data)| Power {
                    power_type,
                    amount,
                    extra_data,
                    instance_id: None,
                    payload: PowerPayload::None,
                    just_applied: false,
                })
                .collect(),
        );
        let mut engine = EngineState::CombatPlayerTurn;
        let initial_turn = state.turn.turn_count;
        assert!(super::tick_until_stable_turn(
            &mut engine,
            &mut state,
            ClientInput::EndTurn
        ));
        assert!(matches!(engine, EngineState::PendingChoice(_)));
        assert_eq!(state.entities.player.current_hp, 79);
        // A persisted turn at this choice retains which hooks already ran.
        state.turn = serde_json::from_slice(&serde_json::to_vec(&state.turn).unwrap()).unwrap();
        assert!(state.turn.counters.end_turn_powers_triggered);
        assert!(super::tick_until_stable_turn(
            &mut engine,
            &mut state,
            ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                SelectionScope::Hand,
                [keep]
            ))
        ));
        assert_eq!(engine, EngineState::CombatPlayerTurn);
        assert_eq!(state.entities.player.current_hp, 79);
        assert_eq!(state.turn.turn_count, initial_turn + 1);
        assert!(!state.turn.counters.end_turn_powers_triggered);
    }

    #[test]
    fn end_turn_regret_keeps_trigger_time_hand_size_after_earlier_burn_leaves_hand() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        combat_state.turn.mark_skip_monster_turn_pending();
        combat_state.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Defend, 40)].into();
        combat_state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Burn, 10),
            CombatCard::for_scenario(CardId::Regret, 20),
            CombatCard::for_scenario(CardId::Strike, 30),
        ];
        let hp_before = combat_state.entities.player.current_hp;
        let mut engine_state = EngineState::CombatPlayerTurn;

        assert!(super::tick_until_stable_turn(
            &mut engine_state,
            &mut combat_state,
            ClientInput::EndTurn,
        ));

        assert_eq!(
            combat_state.entities.player.current_hp,
            hp_before - 5,
            "Burn deals 2, while Regret retains the original three-card hand snapshot"
        );
    }

    #[test]
    fn runic_pyramid_keeps_hand_but_clears_one_turn_retain_flags() {
        let mut combat_state = blank_test_combat();
        combat_state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicPyramid));
        let mut explicitly_retained = CombatCard::for_scenario(CardId::Defend, 20);
        explicitly_retained.retain_override = Some(true);
        combat_state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            explicitly_retained,
            CombatCard::for_scenario(CardId::Bash, 30),
        ];

        discard_hand_for_turn_transition(&mut combat_state);

        assert_eq!(
            combat_state
                .zones
                .hand
                .iter()
                .map(|card| (card.id, card.lineage, card.retain_override))
                .collect::<Vec<_>>(),
            vec![
                (CardId::Strike, crate::test_support::scenario_lineage(10), None),
                (CardId::Bash, crate::test_support::scenario_lineage(30), None),
                (CardId::Defend, crate::test_support::scenario_lineage(20), None),
            ],
            "Java restores truly retained cards after the cards kept by Pyramid, then clears the one-turn retain flag"
        );
        assert!(combat_state.zones.discard_pile.is_empty());
    }

    #[test]
    fn equilibrium_keeps_late_unmarked_cards_and_restores_retained_cards_after_them() {
        let mut combat_state = blank_test_combat();
        crate::engine::action_handlers::powers::handle_apply_power(
            0,
            0,
            PowerId::Equilibrium,
            1,
            &mut combat_state,
        );
        let mut retained = CombatCard::for_scenario(CardId::Strike, 10);
        retained.retain_override = Some(true);
        combat_state.zones.hand = vec![retained, CombatCard::for_scenario(CardId::Defend, 20)];
        discard_hand_for_turn_transition(&mut combat_state);
        assert_eq!(
            combat_state
                .zones
                .hand
                .iter()
                .map(|card| card.id)
                .collect::<Vec<_>>(),
            [CardId::Defend, CardId::Strike]
        );
        assert!(combat_state.zones.discard_pile.is_empty());
        assert!(combat_state
            .zones
            .hand
            .iter()
            .all(|card| card.retain_override.is_none()));
    }

    #[test]
    fn equilibrium_marks_existing_non_ethereal_cards_during_the_power_callback() {
        let mut state = blank_test_combat();
        crate::engine::action_handlers::powers::handle_apply_power(
            0,
            0,
            PowerId::Equilibrium,
            1,
            &mut state,
        );
        state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Apparition, 20),
        ];
        crate::engine::action_handlers::execute_action(Action::EndTurnTrigger, &mut state);
        crate::engine::action_handlers::cards::handle_end_turn_post_cards(&mut state);
        assert_eq!(
            state.zones.hand[0].retain_override,
            Some(true),
            "Equilibrium marks the live hand during the post-card power callback, before its queued effects"
        );
        assert_eq!(state.zones.hand[1].retain_override, None);
    }

    #[test]
    fn equilibrium_marks_cards_before_establishment_queued_discount_resolves() {
        let mut state = blank_test_combat();
        state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        state.turn.mark_skip_monster_turn_pending();
        state.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
        state.zones.draw_pile = (20..25)
            .map(|lineage| CombatCard::for_scenario(CardId::Defend, lineage))
            .collect::<Vec<_>>()
            .into();
        for power in [PowerId::EstablishmentPower, PowerId::Equilibrium] {
            crate::engine::action_handlers::powers::handle_apply_power(0, 0, power, 1, &mut state);
        }
        let mut engine = EngineState::CombatPlayerTurn;
        assert!(super::tick_until_stable_turn(
            &mut engine,
            &mut state,
            ClientInput::EndTurn
        ));
        let strike = state
            .zones
            .hand
            .iter()
            .find(|card| card.lineage == crate::test_support::scenario_lineage(10))
            .unwrap();
        assert_eq!(strike.combat_cost_without_turn_override_java(), 0);
        assert_eq!(strike.retain_override, None);
    }

    #[test]
    fn turn_transition_runs_on_retained_hooks_even_under_runic_pyramid() {
        let mut combat_state = blank_test_combat();
        combat_state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicPyramid));
        combat_state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::SandsOfTime, 20),
        ];

        discard_hand_for_turn_transition(&mut combat_state);

        assert_eq!(combat_state.zones.hand[1].id, CardId::SandsOfTime,);
        assert_eq!(
            combat_state.zones.hand[1].get_cost(),
            3,
            "Java DiscardAtEndOfTurnAction still moves selfRetain cards through RestoreRetainedCardsAction under Runic Pyramid, so onRetained fires"
        );
        assert!(combat_state.zones.discard_pile.is_empty());
    }

    #[test]
    fn pyramid_skips_discard_without_granting_retain_or_preventing_ethereal() {
        let mut state = blank_test_combat();
        state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicPyramid));
        state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        state.turn.mark_skip_monster_turn_pending();
        crate::engine::action_handlers::powers::handle_apply_power(
            0,
            0,
            PowerId::EstablishmentPower,
            1,
            &mut state,
        );
        let mut retained = CombatCard::for_scenario(CardId::Defend, 12);
        retained.retain_override = Some(true);
        state.zones.hand = vec![
            CombatCard::for_scenario(CardId::SandsOfTime, 10),
            CombatCard::for_scenario(CardId::Strike, 11),
            retained,
            CombatCard::for_scenario(CardId::Apparition, 13),
        ];
        state.zones.draw_pile = (20..25)
            .map(|lineage| CombatCard::for_scenario(CardId::Defend, lineage))
            .collect::<Vec<_>>()
            .into();
        let mut engine = EngineState::CombatPlayerTurn;
        assert!(super::tick_until_stable_turn(
            &mut engine,
            &mut state,
            ClientInput::EndTurn
        ));
        let kept = state
            .zones
            .hand
            .iter()
            .take(3)
            .map(|card| {
                (
                    card.id,
                    card.combat_cost_without_turn_override_java(),
                    card.retain_override,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            kept,
            [
                (CardId::Strike, 1, None),
                (CardId::SandsOfTime, 2, None),
                (CardId::Defend, 0, None),
            ]
        );
        assert_eq!(state.zones.hand.len(), 8);
        assert_eq!(
            state
                .zones
                .exhaust_pile
                .iter()
                .map(|card| card.id)
                .collect::<Vec<_>>(),
            [CardId::Apparition]
        );
        assert!(state.zones.discard_pile.is_empty());
    }

    #[test]
    fn blur_retains_player_block_through_next_turn_while_power_ticks_down() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.player.block = 12;
        combat_state.entities.monsters = vec![crate::test_support::planned_monster(
            crate::content::monsters::EnemyId::Cultist,
            3,
        )];
        combat_state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::Blur,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        combat_state.turn.begin_turn_transition();
        let mut engine_state = EngineState::CombatProcessing;

        for _ in 0..64 {
            if engine_state == EngineState::CombatPlayerTurn {
                break;
            }
            assert!(super::tick_engine(
                &mut engine_state,
                &mut combat_state,
                None
            ));
        }

        assert_eq!(engine_state, EngineState::CombatPlayerTurn);
        assert_eq!(
            combat_state.entities.player.block, 12,
            "Java GameActionManager skips new-turn block loss while Blur exists"
        );
        assert!(
            !crate::content::powers::store::has_power(&combat_state, 0, PowerId::Blur),
            "Java BlurPower ticks down while still preserving that turn's block"
        );
    }

    #[test]
    fn draw_reduction_decay_is_queued_before_next_turn_draw_count_like_java_game_hand_size() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![crate::test_support::planned_monster(
            crate::content::monsters::EnemyId::Cultist,
            3,
        )];
        combat_state.zones.draw_pile = ((0..5)
            .map(|uuid| crate::runtime::combat::CombatCard::for_scenario(CardId::Strike, uuid))
            .collect::<Vec<_>>())
        .into();
        combat_state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::DrawReduction,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        combat_state.recompute_turn_start_draw_modifier();
        combat_state.turn.begin_turn_transition();
        let mut engine_state = EngineState::CombatProcessing;

        for _ in 0..96 {
            if engine_state == EngineState::CombatPlayerTurn {
                break;
            }
            assert!(super::tick_engine(
                &mut engine_state,
                &mut combat_state,
                None
            ));
        }

        assert_eq!(engine_state, EngineState::CombatPlayerTurn);
        assert_eq!(
            combat_state.zones.hand.len(),
            4,
            "Java queues ReducePowerAction, then constructs next-turn DrawCardAction from the still-reduced gameHandSize"
        );
        assert!(
            !crate::content::powers::store::has_power(&combat_state, 0, PowerId::DrawReduction),
            "The queued ReducePowerAction still removes DrawReduction before player control returns"
        );
    }

    #[test]
    fn turn_start_post_draw_hooks_queue_before_draw_generated_actions_like_java() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        combat_state.zones.draw_pile = (vec![
            crate::runtime::combat::CombatCard::for_scenario(CardId::Void, 71),
            crate::runtime::combat::CombatCard::for_scenario(CardId::Strike, 72),
            crate::runtime::combat::CombatCard::for_scenario(CardId::Strike, 73),
            crate::runtime::combat::CombatCard::for_scenario(CardId::Strike, 74),
            crate::runtime::combat::CombatCard::for_scenario(CardId::Strike, 75),
        ])
        .into();
        combat_state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::DrawCardNextTurn,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        combat_state.turn.mark_skip_monster_turn_pending();
        combat_state.turn.begin_turn_transition();
        // This test starts after ordinary end-turn power callbacks, at the
        // empty queue that prepares the next draw.
        crate::engine::action_handlers::cards::handle_end_turn_post_cards(&mut combat_state);
        let mut engine_state = EngineState::CombatProcessing;

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));
        assert_eq!(
            combat_state.engine.action_queue.front(),
            Some(&Action::PostDrawTrigger),
            "Rust synthetic hook action must run before DrawCards so hook actions append behind DrawCards like Java addToBot"
        );

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));
        assert_eq!(
            combat_state.engine.action_queue.iter().take(3).collect::<Vec<_>>(),
            vec![
                &Action::DrawCards(5),
                &Action::DrawCards(1),
                &Action::RemovePower {
                    target: 0,
                    power_id: PowerId::DrawCardNextTurn,
                },
            ],
            "Java runs atStartOfTurnPostDraw hook methods before DrawCardAction executes, so their addToBot actions are already behind the turn-start draw"
        );

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));
        assert_eq!(
            combat_state.engine.action_queue.iter().take(3).collect::<Vec<_>>(),
            vec![
                &Action::DrawCards(1),
                &Action::RemovePower {
                    target: 0,
                    power_id: PowerId::DrawCardNextTurn,
                },
                &Action::GainEnergy { amount: -1 },
            ],
            "Java VoidCard.triggerWhenDrawn uses addToBot, so it lands after already-queued post-draw hook actions"
        );
    }

    #[test]
    fn initial_battle_start_runs_turn_start_relics_before_opening_draw_like_java() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        combat_state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::Lantern));
        combat_state.queue_action_back(Action::PreBattleTrigger);
        let mut engine_state = EngineState::CombatProcessing;

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));
        assert_eq!(
            combat_state.engine.action_queue.front(),
            Some(&Action::BattleStartPreDrawTrigger)
        );

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));
        assert_eq!(
            combat_state.engine.action_queue.iter().take(2).collect::<Vec<_>>(),
            vec![&Action::GainEnergy { amount: 1 }, &Action::DrawCards(5)],
            "Java calls applyStartOfTurnRelics after queuing the initial DrawCardAction but before it executes; Lantern addToTop therefore runs before the opening draw"
        );
    }

    #[test]
    fn initial_battle_start_gambling_chip_suspends_after_opening_draw_like_java() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        combat_state.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::Strike, 81),
            CombatCard::for_scenario(CardId::Defend, 82),
            CombatCard::for_scenario(CardId::Strike, 83),
            CombatCard::for_scenario(CardId::Defend, 84),
            CombatCard::for_scenario(CardId::Strike, 85),
        ])
        .into();
        combat_state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::GamblingChip));
        combat_state.queue_action_back(Action::PreBattleTrigger);
        let mut engine_state = EngineState::CombatProcessing;

        for _ in 0..16 {
            if matches!(engine_state, EngineState::PendingChoice(_)) {
                break;
            }
            assert!(super::tick_engine(
                &mut engine_state,
                &mut combat_state,
                None
            ));
        }

        assert_eq!(
            combat_state.zones.hand.len(),
            5,
            "Gambling Chip atTurnStartPostDraw is called before the initial DrawCardAction executes, but its addToBot action must remain behind that draw"
        );
        assert!(matches!(
            engine_state,
            EngineState::PendingChoice(PendingChoice::HandSelect {
                reason: crate::state::HandSelectReason::GamblingChip,
                ..
            })
        ));
    }

    #[test]
    fn initial_battle_start_does_not_run_power_post_draw_hooks_like_java() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        combat_state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::DrawCardNextTurn,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        combat_state.queue_action_back(Action::PreBattleTrigger);
        let mut engine_state = EngineState::CombatProcessing;

        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));
        assert!(super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));

        assert_eq!(
            combat_state.engine.action_queue.iter().collect::<Vec<_>>(),
            vec![&Action::DrawCards(5)],
            "Java initial AbstractRoom.update calls applyStartOfTurnPostDrawRelics, but not applyStartOfTurnPostDrawPowers"
        );
    }

    #[test]
    fn vault_skip_monster_turn_bypasses_monster_actions_and_end_of_round_powers() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.player.current_hp = 50;
        combat_state.entities.player.block = 0;
        combat_state.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        combat_state.entities.monsters[0].id = 7;
        combat_state.entities.monsters[0].block = 5;
        combat_state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::Vulnerable,
                instance_id: None,
                amount: 2,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        combat_state.turn.mark_skip_monster_turn_pending();
        combat_state.turn.begin_turn_transition();
        let mut engine_state = EngineState::CombatProcessing;

        for _ in 0..64 {
            if engine_state == EngineState::CombatPlayerTurn {
                break;
            }
            assert!(super::tick_engine(
                &mut engine_state,
                &mut combat_state,
                None
            ));
        }

        assert_eq!(engine_state, EngineState::CombatPlayerTurn);
        assert_eq!(
            combat_state.entities.player.current_hp, 50,
            "Java Vault sets room.skipMonsterTurn, so queued monster attacks do not run"
        );
        assert_eq!(
            combat_state.entities.monsters[0].block, 5,
            "Java skips MonsterStartTurnAction as well, so monster pre-turn block loss does not run"
        );
        assert_eq!(
            crate::content::powers::store::power_amount(&combat_state, 0, PowerId::Vulnerable),
            2,
            "Java skips MonsterGroup.applyEndOfTurnPowers(), which also skips player atEndOfRound power ticking"
        );
        assert!(
            !combat_state.turn.counters.skip_monster_turn_pending,
            "Java clears room.skipMonsterTurn once the new player turn begins"
        );
        assert!(
            combat_state
                .monster_protocol_executed_move_history(7)
                .is_empty(),
            "a skipped monster turn must not become public executed history"
        );
    }

    #[test]
    fn monster_during_turn_powers_fire_before_next_monster_turn_like_java_apply_turn_powers() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.player.current_hp = 35;

        let mut exploding = planned_monster(EnemyId::Exploder, 1);
        exploding.id = 11;
        let mut next_monster = planned_monster(EnemyId::Cultist, 1);
        next_monster.id = 12;
        combat_state.entities.monsters = vec![exploding, next_monster];
        crate::content::powers::store::set_powers_for(
            &mut combat_state,
            11,
            vec![Power {
                power_type: PowerId::Explosive,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        combat_state.turn.begin_turn_transition();
        let mut engine_state = EngineState::CombatProcessing;

        for _ in 0..64 {
            if matches!(engine_state, EngineState::GameOver(_)) {
                break;
            }
            let keep_running = super::tick_engine(&mut engine_state, &mut combat_state, None);
            if !keep_running {
                break;
            }
        }

        assert_eq!(
            engine_state,
            EngineState::GameOver(crate::state::core::RunResult::Defeat)
        );
        let next_monster = combat_state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == 12)
            .expect("second monster should still exist");
        assert_eq!(
            combat_state.monster_protocol_executed_move_history(11),
            &[1]
        );
        assert!(
            combat_state
                .monster_protocol_executed_move_history(next_monster.id)
                .is_empty(),
            "Java GameActionManager calls m.applyTurnPowers() immediately after each monster takeTurn(); Explosive damage can kill the player before the next monster is dequeued"
        );
    }

    #[test]
    fn monster_phase_victory_does_not_start_an_extra_player_turn() {
        for (enemy, terminal_power, amount) in [
            (EnemyId::Transient, PowerId::Fading, 1),
            (EnemyId::Exploder, PowerId::Explosive, 1),
            (EnemyId::JawWorm, PowerId::Poison, 999),
        ] {
            let mut state = blank_test_combat();
            state.entities.player.current_hp = 50;
            state.entities.player.max_hp = 80;
            state.entities.player.block = 100;
            state
                .entities
                .player
                .add_relic(RelicState::new(RelicId::BurningBlood));
            let monster = planned_monster(enemy, 1);
            let monster_id = monster.id;
            state.entities.monsters = vec![monster];
            let power = |power_type, amount| Power {
                power_type,
                instance_id: None,
                amount,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            };
            crate::content::powers::store::set_powers_for(
                &mut state,
                monster_id,
                vec![power(terminal_power, amount)],
            );
            crate::content::powers::store::set_powers_for(
                &mut state,
                0,
                vec![power(PowerId::Brutality, 1)],
            );
            state.zones.draw_pile.clear();
            state.zones.hand.clear();
            state.zones.discard_pile.clear();
            for lineage in 100..103 {
                state.add_card_to_discard_pile_top(CombatCard::for_scenario(
                    CardId::Strike,
                    lineage,
                ));
            }
            state.turn.turn_count = 8;
            state.turn.energy = 0;
            state.turn.begin_turn_transition();
            let shuffle_before = state.rng.shuffle_rng.clone();
            let mut engine_state = EngineState::CombatProcessing;
            for _ in 0..64 {
                if !super::tick_engine(&mut engine_state, &mut state, None) {
                    break;
                }
            }
            assert!(
                matches!(engine_state, EngineState::RewardScreen(_)),
                "{enemy:?}"
            );
            assert_eq!(state.turn.turn_count, 8, "{enemy:?}: no extra round");
            assert_eq!(state.turn.energy, 0, "{enemy:?}: no energy recharge");
            assert_eq!(
                state.rng.shuffle_rng, shuffle_before,
                "{enemy:?}: no shuffle"
            );
            assert!(state.zones.hand.is_empty(), "{enemy:?}: no draw");
            assert_eq!(state.zones.discard_pile.len(), 3);
            assert_eq!(
                state.entities.player.current_hp, 56,
                "{enemy:?}: victory healing runs once without an extra Brutality trigger"
            );
        }
    }

    #[test]
    fn victory_settlement_uses_java_basically_dead_flags_not_zero_hp() {
        let mut combat_state = blank_test_combat();
        let mut zero_hp_not_dying = planned_monster(EnemyId::JawWorm, 1);
        zero_hp_not_dying.current_hp = 0;
        zero_hp_not_dying.is_dying = false;
        zero_hp_not_dying.is_escaped = false;
        zero_hp_not_dying.half_dead = false;
        combat_state.entities.monsters = vec![zero_hp_not_dying];
        let mut engine_state = EngineState::CombatProcessing;

        assert_eq!(
            settle_victory_if_ready(&mut engine_state, &mut combat_state),
            None,
            "Java MonsterGroup.areMonstersBasicallyDead ignores currentHealth; only isDying/isEscaping count"
        );
        assert_eq!(engine_state, EngineState::CombatProcessing);
    }

    #[test]
    fn lethal_queued_card_aftermath_resolves_before_victory_healing() {
        let mut combat_state = blank_test_combat();
        combat_state.entities.player.current_hp = 3;
        combat_state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::BurningBlood));

        let mut dead_monster = planned_monster(EnemyId::CorruptHeart, 1);
        dead_monster.current_hp = 0;
        dead_monster.is_dying = true;
        combat_state.entities.monsters = vec![dead_monster];
        combat_state.queue_action_back(Action::Damage(DamageInfo {
            source: 1,
            target: 0,
            base: 3,
            output: 3,
            damage_type: DamageType::Thorns,
            is_modified: false,
        }));
        let mut engine_state = EngineState::CombatProcessing;

        assert_eq!(
            settle_victory_if_ready(&mut engine_state, &mut combat_state),
            Some(true),
            "lethal use-card aftermath must stay ahead of victory hooks"
        );
        assert!(
            !combat_state.turn.counters.victory_triggered,
            "victory hooks must not be armed while committed card aftermath remains"
        );

        assert!(!super::tick_engine(
            &mut engine_state,
            &mut combat_state,
            None
        ));
        assert_eq!(
            engine_state,
            EngineState::GameOver(crate::state::core::RunResult::Defeat)
        );
        assert_eq!(combat_state.entities.player.current_hp, 0);
    }

    #[test]
    fn awakened_one_half_dead_rebirth_executes_monster_turn() {
        let mut awakened = crate::test_support::test_monster(EnemyId::AwakenedOne);
        awakened.id = 7;
        awakened.current_hp = 0;
        awakened.max_hp = 300;
        awakened.is_dying = false;
        awakened.half_dead = true;
        awakened.awakened_one.form1 = false;
        awakened.awakened_one.first_turn = true;
        awakened.awakened_one.protocol_seeded = true;
        awakened.set_planned_move_id(3);

        let mut combat_state = blank_test_combat();
        combat_state.entities.monsters = vec![awakened];
        crate::content::powers::store::set_powers_for(
            &mut combat_state,
            7,
            vec![Power {
                power_type: PowerId::Regen,
                instance_id: None,
                amount: 10,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        let mut engine_state = EngineState::CombatPlayerTurn;

        let alive = super::tick_until_stable_turn(
            &mut engine_state,
            &mut combat_state,
            ClientInput::EndTurn,
        );

        assert!(alive);
        let reborn = combat_state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == 7)
            .unwrap();
        assert_eq!(reborn.current_hp, 300);
        assert!(!reborn.half_dead);
        assert!(!reborn.is_dying);
        assert_eq!(
            combat_state.monster_protocol_executed_move_history(reborn.id),
            &[3]
        );
    }

    #[test]
    fn poison_death_spore_cloud_keeps_fresh_vulnerable_for_the_next_player_turn() {
        use crate::content::powers::store;
        use crate::runtime::combat::PowerPayload;

        let mut state = blank_test_combat();
        state.entities.player.current_hp = 100;
        state.entities.player.max_hp = 100;
        let mut fungi = planned_monster(EnemyId::FungiBeast, 1);
        fungi.id = 31;
        fungi.current_hp = 1;
        let mut survivor = planned_monster(EnemyId::Cultist, 1);
        survivor.id = 32;
        state.entities.monsters = vec![fungi, survivor];
        store::set_powers_for(
            &mut state,
            31,
            vec![
                Power {
                    power_type: PowerId::SporeCloud,
                    amount: 2,
                    extra_data: 0,
                    instance_id: None,
                    payload: PowerPayload::None,
                    just_applied: false,
                },
                Power {
                    power_type: PowerId::Poison,
                    amount: 1,
                    extra_data: 0,
                    instance_id: None,
                    payload: PowerPayload::SourceEntity(0),
                    just_applied: false,
                },
            ],
        );
        for expected_duration in [2, 1] {
            state.turn.begin_turn_transition();
            let mut engine = EngineState::CombatProcessing;
            for _ in 0..64 {
                if engine == EngineState::CombatPlayerTurn {
                    break;
                }
                assert!(super::tick_engine(&mut engine, &mut state, None));
            }
            assert_eq!(engine, EngineState::CombatPlayerTurn);
            assert!(state.entities.monsters[0].is_dying);
            let vulnerable = store::powers_for(&state, 0)
                .unwrap()
                .iter()
                .find(|power| power.power_type == PowerId::Vulnerable)
                .unwrap();
            assert_eq!(vulnerable.amount, expected_duration);
            assert!(
                !vulnerable.just_applied,
                "the round-end hook consumes the fresh flag exactly once"
            );
        }
    }

    #[test]
    fn loop_lightning_rng_precedes_turn_draw_reshuffle() {
        use crate::runtime::combat::{OrbEntity, OrbId};
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};

        let mut state = blank_test_combat();
        let mut monster = planned_monster(EnemyId::Cultist, 1);
        monster.current_hp = 200;
        monster.max_hp = 200;
        state.entities.monsters = vec![monster];
        state.entities.player.max_orbs = 1;
        state.entities.player.orbs = vec![OrbEntity::new(OrbId::Lightning)];
        crate::content::powers::store::set_powers_for(
            &mut state,
            0,
            vec![Power {
                power_type: PowerId::Loop,
                instance_id: None,
                amount: 5,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        for lineage in 100..106 {
            state.add_card_to_discard_pile_top(CombatCard::for_scenario(CardId::Defend, lineage));
        }
        state.turn.mark_skip_monster_turn_pending();
        let mut engine = EngineState::CombatPlayerTurn;
        let trace = begin_sts_rng_trace().unwrap();
        assert!(super::tick_until_stable_turn(
            &mut engine,
            &mut state,
            ClientInput::EndTurn,
        ));
        let calls = trace.finish().unwrap();
        // Native LoopPower calls the orb callbacks during atStartOfTurn.
        // Their five queued Lightning actions precede the turn's DrawCardAction,
        // even when that draw reshuffles. The first hit is the ordinary turn end.
        assert_eq!(calls.len(), 7);
        assert!(calls[..6].iter().all(|event| {
            event.owner == Some("floor.card_random")
                && matches!(
                    event.call,
                    StsRngCall::RandomIntRange {
                        start: 0,
                        end: 0,
                        ..
                    }
                )
        }));
        assert_eq!(calls[6].owner, Some("floor.shuffle"));
        assert!(matches!(calls[6].call, StsRngCall::RandomLongBits { .. }));
        assert_eq!(state.entities.monsters[0].current_hp, 182);
        assert_eq!(state.zones.hand.len(), 5);
    }

    #[test]
    fn monster_pre_turn_invincible_resets_before_poison_like_java_at_start_of_turn() {
        let mut combat_state = blank_test_combat();
        let mut monster = planned_monster(EnemyId::JawWorm, 1);
        monster.id = 31;
        monster.current_hp = 100;
        combat_state.entities.monsters = vec![monster];
        crate::content::powers::store::set_powers_for(
            &mut combat_state,
            31,
            vec![
                Power {
                    power_type: PowerId::Invincible,
                    instance_id: None,
                    amount: 0,
                    extra_data: 300,
                    payload: crate::runtime::combat::PowerPayload::None,
                    just_applied: false,
                },
                Power {
                    power_type: PowerId::Poison,
                    instance_id: None,
                    amount: 5,
                    extra_data: 0,
                    payload: crate::runtime::combat::PowerPayload::None,
                    just_applied: false,
                },
            ],
        );
        combat_state.turn.begin_turn_transition();
        let mut engine_state = EngineState::CombatProcessing;

        for _ in 0..64 {
            if engine_state == EngineState::CombatPlayerTurn {
                break;
            }
            assert!(super::tick_engine(
                &mut engine_state,
                &mut combat_state,
                None
            ));
        }

        assert_eq!(engine_state, EngineState::CombatPlayerTurn);
        assert_eq!(
            crate::content::powers::store::power_amount(
                &combat_state,
                31,
                PowerId::Invincible
            ),
            295,
            "Java InvinciblePower.atStartOfTurn resets maxAmt before PoisonPower queues start-of-turn HP loss; it is not reset again before the monster's takeTurn"
        );
    }
}
