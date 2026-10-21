mod coverage_rules;

use coverage_rules::{
    accursed_blacksmith_choice, addict_choice, colosseum_choice, duplicator_choice,
    fountain_choice, golden_shrine_choice, gremlin_wheel_choice, knowing_skull_choice, lab_choice,
    note_for_yourself_choice, secret_portal_choice, sensory_stone_choice, ssssserpent_choice,
    the_joust_choice, upgrade_shrine_choice,
};

use crate::ai::deck_mutation_compiler_v1::{
    compile_deck_mutation_decision_v1, DeckMutationCompilerRequestV1,
};
use crate::ai::event_resource_budget::{
    build_event_resource_budget, EventGainClass, EventResourceBudget, EventSpendClass,
    EventVarianceTolerance,
};
use crate::ai::route_window_facts::{build_route_window_facts, RouteWindowFactsConfig};
use crate::ai::strategic::has_curse_mitigation_or_synergy_v1;
use crate::ai::strategy::decision_pipeline::{
    evaluate_decision_candidate, DecisionCandidateKind, DecisionPipelineContext,
};
use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
use crate::ai::strategy::deck_strategic_deficit::{
    assess_deck_strategic_deficit_summary, StrategicBurdenLevel, StrategicDeficitLevel,
};
use crate::ai::strategy::reward_admission::{
    assess_reward_admission_from_master_deck, reward_admission_order_key_v1,
};
use crate::ai::strategy::run_strategic_facts::RunStrategicFacts;
use crate::content::cards::{
    get_card_definition, is_starter_basic, is_starter_defend, is_starter_strike, CardId, CardType,
};
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use crate::runtime::combat::CombatCard;
use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{
    EventActionKind, EventCardKind, EventEffect, EventId, EventOptionSemantics, EventRelicKind,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventOwnerPolicyGap {
    MissingEventState,
    UnownedEvent(EventId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventOwnerOptionSelector {
    Action(EventActionKind),
    Effect(EventEffect),
    OptionIndex(usize),
}

impl EventOwnerOptionSelector {
    pub fn matches(&self, option_index: usize, semantics: &EventOptionSemantics) -> bool {
        match self {
            EventOwnerOptionSelector::Action(action) => semantics.action == *action,
            EventOwnerOptionSelector::Effect(effect) => semantics.effects.contains(effect),
            EventOwnerOptionSelector::OptionIndex(index) => option_index == *index,
        }
    }
}

pub fn event_owner_policy_selector(
    run_state: &RunState,
) -> Result<EventOwnerOptionSelector, EventOwnerPolicyGap> {
    let event_id = run_state
        .event_state
        .as_ref()
        .map(|event| event.id)
        .ok_or(EventOwnerPolicyGap::MissingEventState)?;
    let selector = match event_id {
        EventId::BigFish => big_fish_choice(run_state),
        EventId::Cleric => cleric_choice(run_state),
        EventId::DeadAdventurer => dead_adventurer_choice(run_state),
        EventId::GoldenIdol => golden_idol_choice(run_state),
        EventId::LivingWall => living_wall_choice(run_state),
        EventId::Mushrooms => mushrooms_choice(run_state),
        EventId::ScrapOoze => scrap_ooze_choice(run_state),
        EventId::ShiningLight => shining_light_choice(run_state),
        EventId::Ssssserpent => ssssserpent_choice(run_state),
        EventId::WorldOfGoop => world_of_goop_choice(run_state),
        EventId::GoldenWing => golden_wing_choice(run_state),
        EventId::MatchAndKeep => match_and_keep_choice(run_state),
        EventId::GoldenShrine => golden_shrine_choice(run_state),
        EventId::Addict => addict_choice(run_state),
        EventId::BackTotheBasics => back_to_basics_choice(run_state),
        EventId::Beggar => beggar_choice(run_state),
        EventId::Colosseum => colosseum_choice(run_state),
        EventId::CursedTome => cursed_tome_choice(run_state),
        EventId::DrugDealer => drug_dealer_choice(run_state),
        EventId::ForgottenAltar => forgotten_altar_choice(run_state),
        EventId::Ghosts => ghosts_choice(run_state),
        EventId::KnowingSkull => knowing_skull_choice(run_state),
        EventId::MaskedBandits => masked_bandits_choice(run_state),
        EventId::Mausoleum => mausoleum_choice(run_state),
        EventId::Nest => nest_choice(run_state),
        EventId::Nloth => nloth_choice(run_state),
        EventId::TheJoust => the_joust_choice(run_state),
        EventId::TheLibrary => the_library_choice(run_state),
        EventId::Vampires => vampires_choice(run_state),
        EventId::Falling => super::falling_owner::falling_choice(run_state),
        EventId::MindBloom => mind_bloom_choice(run_state),
        EventId::MoaiHead => moai_head_choice(run_state),
        EventId::MysteriousSphere => mysterious_sphere_choice(run_state),
        EventId::SensoryStone => sensory_stone_choice(run_state),
        EventId::TombRedMask => tomb_red_mask_choice(run_state),
        EventId::WindingHalls => winding_halls_choice(run_state),
        EventId::AccursedBlacksmith => accursed_blacksmith_choice(run_state),
        EventId::BonfireElementals => bonfire_choice(run_state),
        EventId::BonfireSpirits => bonfire_choice(run_state),
        EventId::Designer => designer_choice(run_state),
        EventId::Duplicator => duplicator_choice(run_state),
        EventId::FaceTrader => face_trader_choice(run_state),
        EventId::FountainOfCurseCleansing => fountain_choice(run_state),
        EventId::GremlinWheelGame => gremlin_wheel_choice(run_state),
        EventId::Lab => lab_choice(run_state),
        EventId::NoteForYourself => note_for_yourself_choice(run_state),
        EventId::Purifier => purifier_choice(run_state),
        EventId::SecretPortal => secret_portal_choice(run_state),
        EventId::Transmorgrifier => transmorgrifier_choice(run_state),
        EventId::UpgradeShrine => upgrade_shrine_choice(run_state),
        EventId::WeMeetAgain => we_meet_again_choice(run_state),
        EventId::WomanInBlue => woman_in_blue_choice(run_state),
        EventId::Neow => return Err(EventOwnerPolicyGap::UnownedEvent(EventId::Neow)),
        EventId::SpireHeart => return Err(EventOwnerPolicyGap::UnownedEvent(EventId::SpireHeart)),
    };
    Ok(selector)
}

fn action(action: EventActionKind) -> EventOwnerOptionSelector {
    EventOwnerOptionSelector::Action(action)
}

fn effect(effect: EventEffect) -> EventOwnerOptionSelector {
    EventOwnerOptionSelector::Effect(effect)
}

fn option_index(index: usize) -> EventOwnerOptionSelector {
    EventOwnerOptionSelector::OptionIndex(index)
}

fn big_fish_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    if run_state.current_hp * 100 <= run_state.max_hp * 35 {
        return effect(EventEffect::Heal(run_state.max_hp / 3));
    }
    if run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::Omamori && relic.counter > 0 && !relic.used_up)
    {
        return effect(EventEffect::ObtainCurse {
            count: 1,
            kind: EventCardKind::Specific(CardId::Regret),
        });
    }
    effect(EventEffect::GainMaxHp(5))
}

fn beggar_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if run_state.gold >= 75 && has_safe_purge_target(run_state) => {
            action(EventActionKind::Trade)
        }
        0 => action(EventActionKind::Leave),
        1 => action(EventActionKind::DeckOperation),
        _ => action(EventActionKind::Leave),
    }
}

fn back_to_basics_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    if has_bad_purge_target(run_state) {
        return option_index(0);
    }
    if unupgraded_starter_basic_count(run_state) >= 4 {
        return option_index(1);
    }
    if has_safe_purge_target(run_state) {
        option_index(0)
    } else {
        option_index(1)
    }
}

fn scrap_ooze_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if scrap_ooze_should_reach(run_state) => action(EventActionKind::Special),
        0 => action(EventActionKind::Leave),
        _ => action(EventActionKind::Leave),
    }
}

fn scrap_ooze_should_reach(run_state: &RunState) -> bool {
    let Some(event_state) = run_state.event_state.as_ref() else {
        return false;
    };
    let next_loss = scrap_ooze_next_hp_loss(run_state).unwrap_or({
        if run_state.ascension_level >= 15 {
            5
        } else {
            3
        }
    });
    let hp_after = run_state.current_hp.saturating_sub(next_loss);
    let budget = event_resource_budget_for(run_state);
    if hp_after < budget.hp.route_break_floor || hp_after < 8 {
        return false;
    }
    if spend_breaks_route(hp_loss_class(&budget, next_loss)) {
        return false;
    }

    let already_digging = event_state.internal_state != 0;
    if !already_digging
        && (spend_reserved_or_worse(budget.hp.session_loss)
            || budget.variance.tolerance == EventVarianceTolerance::Low)
    {
        return false;
    }

    if already_digging {
        hp_after >= budget.hp.reserve_floor
    } else {
        hp_after >= budget.hp.free_floor
            || matches!(
                budget.hp.session_loss,
                EventSpendClass::FreeToSpend | EventSpendClass::BudgetedForHighReturn
            )
    }
}

fn scrap_ooze_next_hp_loss(run_state: &RunState) -> Option<i32> {
    crate::engine::event_handler::get_event_options(run_state)
        .first()
        .and_then(|option| {
            option
                .semantics
                .effects
                .iter()
                .find_map(|effect| match effect {
                    EventEffect::LoseHp(loss) => Some(*loss),
                    _ => None,
                })
        })
}

fn drug_dealer_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if drug_dealer_should_take_jax(run_state) => option_index(0),
        0 if drug_dealer_should_take_mutagenic(run_state) => option_index(2),
        0 if drug_dealer_has_safe_transform_two_plan(run_state) => option_index(1),
        0 => option_index(2),
        _ => action(EventActionKind::Leave),
    }
}

fn drug_dealer_has_safe_transform_two_plan(run_state: &RunState) -> bool {
    matches!(
        event_resource_budget_for(run_state).deck.random_transform,
        EventSpendClass::FreeToSpend | EventSpendClass::BudgetedForHighReturn
    ) && drug_dealer_safe_transform_target_count(run_state) >= 2
}

fn drug_dealer_safe_transform_target_count(run_state: &RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| {
            let def = get_card_definition(card.id);
            def.card_type != CardType::Curse
                && card.id != CardId::Parasite
                && (is_starter_strike(card.id) || is_starter_defend(card.id))
        })
        .count()
}

fn drug_dealer_should_take_mutagenic(run_state: &RunState) -> bool {
    has_relic(run_state, RelicId::ClockworkSouvenir)
        || has_relic(run_state, RelicId::OrangePellets)
        || has_relic(run_state, RelicId::BagOfMarbles)
        || has_card(run_state, CardId::Panacea)
        || drug_dealer_turn_one_conversion_score(run_state) >= 3
}

fn drug_dealer_turn_one_conversion_score(run_state: &RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .map(|card| {
            let def = get_card_definition(card.id);
            if def.card_type != CardType::Attack || is_starter_strike(card.id) {
                0
            } else if def.is_multi_damage
                || matches!(
                    card.id,
                    CardId::Pummel
                        | CardId::TwinStrike
                        | CardId::SwordBoomerang
                        | CardId::Whirlwind
                )
            {
                2
            } else if def.cost <= 1 {
                1
            } else {
                0
            }
        })
        .sum()
}

fn drug_dealer_should_take_jax(run_state: &RunState) -> bool {
    has_card(run_state, CardId::Rupture)
        && (has_card(run_state, CardId::Reaper) || has_relic(run_state, RelicId::RunicCube))
        && (has_strength_payoff_card(run_state)
            || drug_dealer_turn_one_conversion_score(run_state) >= 3)
        && !spend_reserved_or_worse(event_resource_budget_for(run_state).hp.medium_loss)
}

fn has_strength_payoff_card(run_state: &RunState) -> bool {
    run_state.master_deck.iter().any(|card| {
        matches!(
            card.id,
            CardId::HeavyBlade
                | CardId::LimitBreak
                | CardId::Pummel
                | CardId::Reaper
                | CardId::SwordBoomerang
                | CardId::TwinStrike
        )
    })
}

fn bonfire_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 => action(EventActionKind::Continue),
        1 => action(EventActionKind::DeckOperation),
        2 => action(EventActionKind::Continue),
        _ => action(EventActionKind::Leave),
    }
}

fn face_trader_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 => action(EventActionKind::Continue),
        1 if face_trader_should_touch(run_state) => option_index(0),
        1 if face_trader_should_trade(run_state) => option_index(1),
        1 => option_index(2),
        _ => action(EventActionKind::Leave),
    }
}

fn face_trader_should_touch(run_state: &RunState) -> bool {
    let budget = event_resource_budget_for(run_state);
    let loss = (run_state.max_hp / 10).max(1);
    !has_relic(run_state, RelicId::Ectoplasm)
        && !spend_reserved_or_worse(hp_loss_class(&budget, loss))
        && matches!(
            budget.gold.gold_gain,
            EventGainClass::UsefulSoon | EventGainClass::UnknownOpportunity
        )
}

fn face_trader_should_trade(run_state: &RunState) -> bool {
    if run_state.ascension_level >= 15 {
        return false;
    }
    let very_early = run_state.act_num == 1 && run_state.floor_num <= 6;
    let cannot_touch = !face_trader_should_touch(run_state);
    very_early
        && cannot_touch
        && run_state.current_hp * 100 >= run_state.max_hp * 60
        && !deck_hates_turn_one_weak(run_state)
}

fn deck_hates_turn_one_weak(run_state: &RunState) -> bool {
    let deficit = assess_deck_strategic_deficit_summary(
        &run_state.master_deck,
        RunStrategicFacts::from_run_state(run_state),
    );
    deficit.frontload_damage == StrategicDeficitLevel::Missing
        || drug_dealer_turn_one_conversion_score(run_state) >= 4
}

fn shining_light_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    if !run_state
        .master_deck
        .iter()
        .any(crate::state::core::master_deck_card_can_upgrade)
    {
        return action(EventActionKind::Leave);
    }
    let after = run_state.current_hp - shining_light_damage(run_state);
    if after >= 18 && after * 100 >= run_state.max_hp * 35 {
        effect(EventEffect::UpgradeCard { count: 2 })
    } else {
        action(EventActionKind::Leave)
    }
}

fn living_wall_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    if legal_purge_targets(run_state)
        .iter()
        .any(|card| is_non_parasite_curse(card))
    {
        return effect(living_wall_remove_effect());
    }
    if legal_purge_targets(run_state)
        .iter()
        .any(|card| card.id == crate::content::cards::CardId::Parasite)
    {
        return effect(EventEffect::TransformCard { count: 1 });
    }
    let best_upgrade = crate::ai::strategy::campfire_upgrade_quality::rank_campfire_upgrades(
        &run_state.master_deck,
        has_relic(run_state, RelicId::SneckoEye),
    )
    .into_iter()
    .find(|target| {
        legal_purge_targets(run_state)
            .iter()
            .any(|card| card.lineage == run_state.master_deck[target.deck_index].lineage)
    });
    if best_upgrade.as_ref().is_some_and(|target| {
        target.tier >= crate::ai::strategy::campfire_upgrade_quality::CampfireUpgradeTier::Useful
    }) {
        return effect(EventEffect::UpgradeCard { count: 1 });
    }
    if legal_purge_targets(run_state)
        .iter()
        .any(|card| is_starter_strike(card.id))
    {
        return effect(living_wall_remove_effect());
    }
    if best_upgrade.is_some() {
        return effect(EventEffect::UpgradeCard { count: 1 });
    }
    effect(EventEffect::TransformCard { count: 1 })
}

fn living_wall_remove_effect() -> EventEffect {
    EventEffect::RemoveCard {
        count: 1,
        target_lineage: None,
        kind: EventCardKind::Unknown,
    }
}

fn world_of_goop_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if has_relic(run_state, RelicId::Ectoplasm) => action(EventActionKind::Leave),
        0 if hp_after_loss_is_safe(run_state, event_hp_loss_estimate(run_state, 11)) => {
            effect(EventEffect::GainGold(75))
        }
        0 => action(EventActionKind::Leave),
        _ => action(EventActionKind::Leave),
    }
}

fn golden_wing_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if hp_after_loss_is_safe(run_state, event_hp_loss_estimate(run_state, 7))
            && has_safe_purge_target(run_state) =>
        {
            action(EventActionKind::DeckOperation)
        }
        0 if golden_wing_destroy_available(run_state)
            && !has_relic(run_state, RelicId::Ectoplasm) =>
        {
            effect(EventEffect::GainGoldRange { min: 50, max: 80 })
        }
        0 => action(EventActionKind::Leave),
        1 => action(EventActionKind::DeckOperation),
        _ => action(EventActionKind::Leave),
    }
}

fn golden_wing_destroy_available(run_state: &RunState) -> bool {
    crate::engine::event_handler::get_event_options(run_state)
        .into_iter()
        .any(|option| {
            !option.ui.disabled
                && option
                    .semantics
                    .effects
                    .contains(&EventEffect::GainGoldRange { min: 50, max: 80 })
        })
}

fn cleric_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if cleric_should_heal_before_purify(run_state) => action(EventActionKind::Trade),
        0 if cleric_can_purify(run_state) && has_safe_purge_target(run_state) => {
            action(EventActionKind::DeckOperation)
        }
        0 if cleric_heal_has_real_value(run_state) => action(EventActionKind::Trade),
        0 => action(EventActionKind::Leave),
        _ => action(EventActionKind::Leave),
    }
}

fn cleric_should_heal_before_purify(run_state: &RunState) -> bool {
    run_state.gold >= 35
        && cleric_heal_effective_amount(run_state) > 0
        && !has_relic(run_state, RelicId::MarkOfTheBloom)
        && run_state.current_hp * 100 <= run_state.max_hp * 35
}

fn cleric_heal_has_real_value(run_state: &RunState) -> bool {
    run_state.gold >= 35
        && cleric_heal_effective_amount(run_state) >= 8
        && !has_relic(run_state, RelicId::MarkOfTheBloom)
        && run_state.current_hp * 100 <= run_state.max_hp * 55
}

fn cleric_heal_effective_amount(run_state: &RunState) -> i32 {
    let heal = (run_state.max_hp as f32 * 0.25) as i32;
    (run_state.max_hp - run_state.current_hp).max(0).min(heal)
}

fn cleric_can_purify(run_state: &RunState) -> bool {
    run_state.gold >= cleric_purify_cost(run_state)
        && crate::state::core::has_non_bottled_purgeable_master_deck_card(run_state)
}

fn cleric_purify_cost(run_state: &RunState) -> i32 {
    if run_state.ascension_level >= 15 {
        75
    } else {
        50
    }
}

fn golden_idol_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if golden_idol_has_acceptable_payment(run_state) => effect(EventEffect::ObtainRelic {
            count: 1,
            kind: EventRelicKind::Specific(RelicId::GoldenIdol),
        }),
        0 => action(EventActionKind::Leave),
        1 if has_omamori_charge(run_state) => effect(EventEffect::ObtainCurse {
            count: 1,
            kind: EventCardKind::Specific(CardId::Injury),
        }),
        1 if golden_idol_curse_payment_is_digested(run_state) => effect(EventEffect::ObtainCurse {
            count: 1,
            kind: EventCardKind::Specific(CardId::Injury),
        }),
        1 if golden_idol_hide_is_acceptable(run_state) => {
            effect(EventEffect::LoseMaxHp(golden_idol_max_hp_loss(run_state)))
        }
        1 if hp_after_loss_is_safe(
            run_state,
            event_hp_loss_estimate(run_state, golden_idol_damage(run_state)),
        ) =>
        {
            effect(EventEffect::LoseHp(golden_idol_damage(run_state)))
        }
        1 => effect(EventEffect::LoseMaxHp(golden_idol_max_hp_loss(run_state))),
        _ => action(EventActionKind::Leave),
    }
}

fn golden_idol_has_acceptable_payment(run_state: &RunState) -> bool {
    has_omamori_charge(run_state)
        || golden_idol_curse_payment_is_digested(run_state)
        || golden_idol_hide_is_acceptable(run_state)
        || hp_after_loss_is_safe(
            run_state,
            event_hp_loss_estimate(run_state, golden_idol_damage(run_state)),
        )
}

fn golden_idol_curse_payment_is_digested(run_state: &RunState) -> bool {
    curse_count(run_state) < 2 && has_curse_mitigation_or_synergy_v1(run_state)
}

fn golden_idol_hide_is_acceptable(run_state: &RunState) -> bool {
    run_state
        .max_hp
        .saturating_sub(golden_idol_max_hp_loss(run_state))
        >= 50
}

fn golden_idol_damage(run_state: &RunState) -> i32 {
    if run_state.ascension_level >= 15 {
        (run_state.max_hp as f32 * 0.35) as i32
    } else {
        (run_state.max_hp as f32 * 0.25) as i32
    }
}

fn golden_idol_max_hp_loss(run_state: &RunState) -> i32 {
    let loss = if run_state.ascension_level >= 15 {
        (run_state.max_hp as f32 * 0.10) as i32
    } else {
        (run_state.max_hp as f32 * 0.08) as i32
    };
    loss.max(1)
}

fn transmorgrifier_choice(_run_state: &RunState) -> EventOwnerOptionSelector {
    action(EventActionKind::Leave)
}

fn purifier_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if has_safe_purge_target(run_state) => action(EventActionKind::DeckOperation),
        0 => action(EventActionKind::Leave),
        _ => action(EventActionKind::Leave),
    }
}

fn the_library_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if the_library_should_sleep(run_state) => option_index(1),
        0 => option_index(0),
        1 => the_library_best_card_index(run_state)
            .map(option_index)
            .unwrap_or_else(|| action(EventActionKind::Leave)),
        _ => action(EventActionKind::Leave),
    }
}

fn the_library_should_sleep(run_state: &RunState) -> bool {
    !has_relic(run_state, RelicId::MarkOfTheBloom)
        && run_state.current_hp * 100 <= run_state.max_hp * 35
}

fn the_library_best_card_index(run_state: &RunState) -> Option<usize> {
    let event_state = run_state.event_state.as_ref()?;
    let context = DecisionPipelineContext::reward(DeckPlanSnapshot::from_run_state(run_state));
    (0..event_state.card_payload.len())
        .filter_map(|idx| {
            let (card, upgrades) = crate::content::events::the_library::library_card_entry_at(
                &event_state.card_payload,
                idx,
            )?;
            let admission =
                assess_reward_admission_from_master_deck(&run_state.master_deck, card, upgrades);
            let evaluation = evaluate_decision_candidate(
                context,
                DecisionCandidateKind::CardRewardPick { card, upgrades },
                Some(&admission),
            );
            Some((
                (
                    evaluation.order_key(false),
                    reward_admission_order_key_v1(&admission),
                    idx,
                ),
                idx,
            ))
        })
        .min_by_key(|(key, _)| *key)
        .map(|(_, idx)| idx)
}

fn tomb_red_mask_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if has_relic(run_state, RelicId::RedMask) => effect(EventEffect::GainGold(222)),
        0 if run_state.gold <= 80 => action(EventActionKind::Trade),
        0 => action(EventActionKind::Leave),
        _ => action(EventActionKind::Leave),
    }
}

fn we_meet_again_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    action(if event_screen(run_state) == 0 {
        EventActionKind::Decline
    } else {
        EventActionKind::Leave
    })
}

fn mysterious_sphere_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    action(match event_screen(run_state) {
        1 => EventActionKind::Fight,
        _ => EventActionKind::Leave,
    })
}

fn nest_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 => action(EventActionKind::Continue),
        1 if !hp_after_loss_is_safe(run_state, 6) => {
            effect(EventEffect::GainGold(if run_state.ascension_level >= 15 {
                50
            } else {
                99
            }))
        }
        // Taking Ritual Dagger opens a run-level growth commitment.  The
        // current production actor can preserve exact growth/HP frontier
        // evidence, but it cannot yet justify spending fungible gold on that
        // commitment.  A small deck is not evidence that the commitment will
        // be upgraded, drawn at a safe lethal, or deliberately selected by a
        // continuation owner.  Until that owner exists, take the dagger only
        // when Ectoplasm makes the displayed gold alternative unavailable.
        1 if has_relic(run_state, RelicId::Ectoplasm) => effect(EventEffect::ObtainColorlessCard {
            count: 1,
            kind: EventCardKind::Specific(CardId::RitualDagger),
        }),
        1 => effect(EventEffect::GainGold(if run_state.ascension_level >= 15 {
            50
        } else {
            99
        })),
        _ => action(EventActionKind::Leave),
    }
}

fn nloth_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    nloth_feed_option(run_state).unwrap_or_else(|| action(EventActionKind::Leave))
}

fn nloth_feed_option(run_state: &RunState) -> Option<EventOwnerOptionSelector> {
    crate::engine::event_handler::get_event_options(run_state)
        .into_iter()
        .enumerate()
        .filter_map(|(option_index, option)| {
            let relic_id = option.semantics.effects.iter().find_map(|effect| {
                if let EventEffect::LoseRelic {
                    specific: Some(id), ..
                } = effect
                {
                    Some(*id)
                } else {
                    None
                }
            })?;
            let relic = run_state.relics.iter().find(|relic| relic.id == relic_id)?;
            let priority =
                crate::ai::strategy::relic_expendability::nloth_free_feed_priority(relic)?;
            Some((priority, option_index))
        })
        .max_by_key(|(priority, _)| *priority)
        .map(|(_, idx)| option_index(idx))
}

fn match_and_keep_choice(_run_state: &RunState) -> EventOwnerOptionSelector {
    option_index(0)
}

fn mind_bloom_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    if run_state.floor_num % 50 > 40 && run_state.current_hp * 100 <= run_state.max_hp * 35 {
        return effect(EventEffect::Heal(
            (run_state.max_hp - run_state.current_hp).max(0),
        ));
    }
    if run_state.floor_num % 50 <= 40 && omamori_charges(run_state) >= 2 {
        return effect(EventEffect::GainGold(999));
    }
    action(EventActionKind::Fight)
}

fn moai_head_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if run_state.current_hp * 100 <= run_state.max_hp * 45 => option_index(0),
        0 if has_relic(run_state, RelicId::GoldenIdol) && run_state.floor_num <= 43 => {
            option_index(1)
        }
        0 => action(EventActionKind::Leave),
        _ => action(EventActionKind::Leave),
    }
}

fn mushrooms_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    action(match event_screen(run_state) {
        0 if mushrooms_eat_is_emergency(run_state) => EventActionKind::Trade,
        0 | 2 => EventActionKind::Fight,
        _ => EventActionKind::Leave,
    })
}

fn mushrooms_eat_is_emergency(run_state: &RunState) -> bool {
    run_state.current_hp * 100 < run_state.max_hp * 30
}

fn forgotten_altar_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    if has_relic(run_state, RelicId::GoldenIdol) {
        return action(EventActionKind::Trade);
    }
    if has_omamori_charge(run_state) {
        return effect(EventEffect::ObtainCurse {
            count: 1,
            kind: EventCardKind::Specific(CardId::Decay),
        });
    }
    if forgotten_altar_pray_projected_hp(run_state) >= forgotten_altar_min_hp_after_pray(run_state)
    {
        effect(EventEffect::GainMaxHp(5))
    } else {
        effect(EventEffect::ObtainCurse {
            count: 1,
            kind: EventCardKind::Specific(CardId::Decay),
        })
    }
}

fn forgotten_altar_pray_projected_hp(run_state: &RunState) -> i32 {
    let (max_hp_gain, raw_loss) = forgotten_altar_pray_effects(run_state)
        .unwrap_or((5, forgotten_altar_fallback_pray_loss(run_state)));
    let healed_hp = (run_state.current_hp + max_hp_gain).min(run_state.max_hp + max_hp_gain);
    healed_hp.saturating_sub(event_hp_loss_estimate(run_state, raw_loss))
}

fn forgotten_altar_pray_effects(run_state: &RunState) -> Option<(i32, i32)> {
    crate::engine::event_handler::get_event_options(run_state)
        .into_iter()
        .filter(|option| !option.ui.disabled)
        .find_map(|option| {
            let max_hp_gain = option
                .semantics
                .effects
                .iter()
                .find_map(|effect| match effect {
                    EventEffect::GainMaxHp(amount) => Some(*amount),
                    _ => None,
                })?;
            let hp_loss = option
                .semantics
                .effects
                .iter()
                .find_map(|effect| match effect {
                    EventEffect::LoseHp(amount) => Some(*amount),
                    _ => None,
                })?;
            Some((max_hp_gain, hp_loss))
        })
}

fn forgotten_altar_fallback_pray_loss(run_state: &RunState) -> i32 {
    let pct = if run_state.ascension_level >= 15 {
        0.35
    } else {
        0.25
    };
    (run_state.max_hp as f32 * pct).round() as i32
}

fn forgotten_altar_min_hp_after_pray(run_state: &RunState) -> i32 {
    let deficit = assess_deck_strategic_deficit_summary(
        &run_state.master_deck,
        RunStrategicFacts::from_run_state(run_state),
    );
    let mut min_hp = if run_state.ascension_level >= 15 {
        32
    } else {
        26
    };
    if has_relic(run_state, RelicId::RunicDome) {
        min_hp += 4;
    }
    if matches!(
        deficit.block_or_mitigation,
        StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
    ) {
        min_hp += 4;
    }
    if deficit.deck_burden == StrategicBurdenLevel::Heavy {
        min_hp -= 2;
    }
    min_hp
}

fn masked_bandits_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    action(match event_screen(run_state) {
        0 if masked_bandits_should_pay(run_state) => EventActionKind::Trade,
        0 => EventActionKind::Fight,
        1..=3 => EventActionKind::Continue,
        _ => EventActionKind::Leave,
    })
}

fn masked_bandits_should_pay(run_state: &RunState) -> bool {
    let danger = run_state.current_hp <= 20 || run_state.current_hp * 100 <= run_state.max_hp * 25;
    danger || !masked_bandits_ready(run_state)
}

fn masked_bandits_ready(run_state: &RunState) -> bool {
    let deficit = assess_deck_strategic_deficit_summary(
        &run_state.master_deck,
        RunStrategicFacts::from_run_state(run_state),
    );
    let multi_enemy_offense = deficit.aoe_or_minion_control != StrategicDeficitLevel::Missing;
    let clean_focus_fire = matches!(
        deficit.frontload_damage,
        StrategicDeficitLevel::Adequate | StrategicDeficitLevel::Surplus
    ) && !deficit.too_many_low_impact_attacks;
    let sustained_mitigation = matches!(
        deficit.block_or_mitigation,
        StrategicDeficitLevel::Adequate | StrategicDeficitLevel::Surplus
    );

    (multi_enemy_offense || clean_focus_fire)
        && (sustained_mitigation || has_bandit_swing_potion(run_state))
}

fn has_bandit_swing_potion(run_state: &RunState) -> bool {
    run_state.potions.iter().any(|slot| {
        slot.as_ref().is_some_and(|potion| {
            matches!(
                potion.id,
                PotionId::FirePotion
                    | PotionId::ExplosivePotion
                    | PotionId::FearPotion
                    | PotionId::BlockPotion
            )
        })
    })
}

fn vampires_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if vampires_should_give_vial(run_state) => action(EventActionKind::Trade),
        0 => action(EventActionKind::Decline),
        _ => action(EventActionKind::Leave),
    }
}

fn vampires_should_give_vial(run_state: &RunState) -> bool {
    has_relic(run_state, RelicId::BloodVial)
        && starter_strike_count(run_state) >= 3
        && run_state.master_deck.len() <= 32
}

fn starter_strike_count(run_state: &RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| is_starter_strike(card.id))
        .count()
}

fn woman_in_blue_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    // Woman in Blue still charges gold when Sozu prevents the potion reward.
    // This is therefore not merely a full-belt capacity question: every paid
    // option is an exact resource loss with no obtainable benefit.
    if has_relic(run_state, RelicId::Sozu) {
        return action(EventActionKind::Leave);
    }
    let empty_slots = run_state
        .potions
        .iter()
        .filter(|slot| slot.is_none())
        .count();
    match empty_slots {
        0 if run_state.ascension_level >= 15 => effect(EventEffect::ObtainPotion { count: 1 }),
        0 => action(EventActionKind::Leave),
        1 => effect(EventEffect::ObtainPotion { count: 1 }),
        2 => effect(EventEffect::ObtainPotion { count: 2 }),
        _ => effect(EventEffect::ObtainPotion { count: 3 }),
    }
}

fn winding_halls_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 => action(EventActionKind::Continue),
        1 if winding_halls_should_press_on(run_state) => effect(EventEffect::Heal(
            (run_state.max_hp as f32
                * if run_state.ascension_level >= 15 {
                    0.20
                } else {
                    0.25
                })
            .round() as i32,
        )),
        1 => effect(EventEffect::LoseMaxHp(
            (run_state.max_hp as f32 * 0.05).round() as i32,
        )),
        _ => action(EventActionKind::Leave),
    }
}

fn winding_halls_should_press_on(run_state: &RunState) -> bool {
    run_state.current_hp * 100 <= run_state.max_hp * 30
        && !has_card(run_state, CardId::Writhe)
        && curse_count(run_state) < 2
}

fn cursed_tome_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    let screen = event_screen(run_state);
    action(match screen {
        0 if cursed_tome_take_is_safe(run_state, screen) => EventActionKind::Continue,
        0 => EventActionKind::Leave,
        1..=3 => EventActionKind::Continue,
        4 if cursed_tome_take_is_safe(run_state, screen) => EventActionKind::Accept,
        4 => EventActionKind::Decline,
        _ => EventActionKind::Leave,
    })
}

fn cursed_tome_take_is_safe(run_state: &RunState, screen: usize) -> bool {
    hp_after_loss_is_safe(run_state, cursed_tome_take_loss_from(run_state, screen))
}

fn designer_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 => action(EventActionKind::Continue),
        1 => designer_service_choice(run_state),
        _ => action(EventActionKind::Leave),
    }
}

fn designer_service_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    let asc = run_state.ascension_level;
    let has_remove = designer_has_clear_remove_target(run_state);
    let has_upgrade = run_state
        .master_deck
        .iter()
        .any(crate::state::core::master_deck_card_can_upgrade);
    if run_state.gold >= designer_full_service_cost(asc) && has_remove {
        return option_index(2);
    }
    if designer_cleanup_removes_cards(run_state)
        && run_state.gold >= designer_cleanup_cost(asc)
        && has_remove
    {
        return option_index(1);
    }
    if run_state.gold >= designer_adjust_cost(asc) && has_upgrade {
        return option_index(0);
    }
    option_index(3)
}

fn designer_has_clear_remove_target(run_state: &RunState) -> bool {
    has_safe_purge_target(run_state)
}

fn designer_adjust_cost(asc: u8) -> i32 {
    if asc >= 15 {
        50
    } else {
        40
    }
}

fn designer_cleanup_cost(asc: u8) -> i32 {
    if asc >= 15 {
        75
    } else {
        60
    }
}

fn designer_full_service_cost(asc: u8) -> i32 {
    if asc >= 15 {
        110
    } else {
        90
    }
}

fn dead_adventurer_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if dead_adventurer_should_search_once(run_state) => option_index(0),
        0 => action(EventActionKind::Leave),
        1 => action(EventActionKind::Fight),
        _ => action(EventActionKind::Leave),
    }
}

fn dead_adventurer_should_search_once(run_state: &RunState) -> bool {
    let Some(event) = run_state.event_state.as_ref() else {
        return false;
    };
    crate::content::events::dead_adventurer::num_rewards(event.internal_state) == 0
        && run_state.current_hp * 100 >= run_state.max_hp * 70
}

fn designer_cleanup_removes_cards(run_state: &RunState) -> bool {
    run_state
        .event_state
        .as_ref()
        .is_some_and(|event| event.internal_state & 2 != 0)
}

fn mausoleum_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    if has_omamori_charge(run_state) {
        return option_index(0);
    }
    if run_state.ascension_level < 15 {
        if curse_count(run_state) >= 2
            || has_card(run_state, CardId::Writhe)
            || has_card(run_state, CardId::Clash)
        {
            option_index(1)
        } else {
            option_index(0)
        }
    } else if has_curse_mitigation_or_synergy_v1(run_state) {
        option_index(0)
    } else {
        option_index(1)
    }
}

fn ghosts_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    let max_hp_after = run_state
        .max_hp
        .saturating_sub(((run_state.max_hp as f32) * 0.5).ceil() as i32);
    let projected_current_hp = run_state.current_hp.min(max_hp_after);
    let budget = event_resource_budget_for(run_state);
    let avoids_immediate_hp_clamp = projected_current_hp == run_state.current_hp;
    let preserves_route_floor = max_hp_after >= budget.hp.route_break_floor
        && projected_current_hp >= budget.hp.route_break_floor;
    let preserves_route_reserve =
        max_hp_after >= budget.hp.reserve_floor && projected_current_hp >= budget.hp.reserve_floor;
    if !preserves_route_floor {
        return option_index(1);
    }

    let deficit = assess_deck_strategic_deficit_summary(
        &run_state.master_deck,
        RunStrategicFacts::from_run_state(run_state),
    );
    let repairs_mitigation_gap = matches!(
        deficit.block_or_mitigation,
        StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
    );
    let supported = has_ghosts_conversion_support(run_state);
    let cost_is_supported = preserves_route_reserve
        || (avoids_immediate_hp_clamp && (repairs_mitigation_gap || supported));
    if !cost_is_supported {
        return option_index(1);
    }

    if run_state.ascension_level >= 15 {
        if repairs_mitigation_gap && supported {
            option_index(0)
        } else {
            option_index(1)
        }
    } else if repairs_mitigation_gap || supported {
        option_index(0)
    } else {
        option_index(1)
    }
}

fn cursed_tome_take_loss_from(run_state: &RunState, screen: usize) -> i32 {
    let final_damage = if run_state.ascension_level >= 15 {
        15
    } else {
        10
    };
    [(1, 1), (2, 2), (3, 3), (4, final_damage)]
        .into_iter()
        .filter(|(loss_screen, _)| screen <= *loss_screen)
        .map(|(_, amount)| event_hp_loss_estimate(run_state, amount))
        .sum()
}

fn event_resource_budget_for(run_state: &RunState) -> EventResourceBudget {
    let route_facts = build_route_window_facts(run_state, RouteWindowFactsConfig::default());
    build_event_resource_budget(run_state, &route_facts)
}

fn hp_loss_class(budget: &EventResourceBudget, loss: i32) -> EventSpendClass {
    if loss <= 5 {
        budget.hp.small_loss
    } else if loss <= 12 {
        budget.hp.medium_loss
    } else {
        budget.hp.large_loss
    }
}

fn spend_reserved_or_worse(class: EventSpendClass) -> bool {
    matches!(
        class,
        EventSpendClass::Reserved | EventSpendClass::RouteBreaking | EventSpendClass::Unavailable
    )
}

fn spend_breaks_route(class: EventSpendClass) -> bool {
    matches!(
        class,
        EventSpendClass::RouteBreaking | EventSpendClass::Unavailable
    )
}

fn event_hp_loss_estimate(run_state: &RunState, amount: i32) -> i32 {
    let has_tungsten_rod = run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::TungstenRod);
    if has_tungsten_rod && amount > 0 {
        amount - 1
    } else {
        amount
    }
}

fn hp_after_loss_is_safe(run_state: &RunState, loss: i32) -> bool {
    let after = run_state.current_hp.saturating_sub(loss);
    after >= 18 && after * 100 >= run_state.max_hp * 35
}

fn has_omamori_charge(run_state: &RunState) -> bool {
    omamori_charges(run_state) > 0
}

fn omamori_charges(run_state: &RunState) -> i32 {
    run_state
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::Omamori && !relic.used_up)
        .map(|relic| relic.counter.max(0))
        .unwrap_or_default()
}

fn has_relic(run_state: &RunState, relic_id: RelicId) -> bool {
    run_state
        .relics
        .iter()
        .any(|relic| relic.id == relic_id && !relic.used_up)
}

fn has_ghosts_conversion_support(run_state: &RunState) -> bool {
    has_relic(run_state, RelicId::ToxicEgg)
        || has_relic(run_state, RelicId::TungstenRod)
        || has_relic(run_state, RelicId::StrangeSpoon)
        || has_card(run_state, CardId::Feed)
        || has_card(run_state, CardId::FeelNoPain)
        || (has_card(run_state, CardId::Corruption) && has_card(run_state, CardId::DarkEmbrace))
}

fn has_card(run_state: &RunState, card_id: CardId) -> bool {
    run_state.master_deck.iter().any(|card| card.id == card_id)
}

fn curse_count(run_state: &RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| {
            get_card_definition(card.id).card_type == crate::content::cards::CardType::Curse
        })
        .count()
}

fn has_bad_purge_target(run_state: &RunState) -> bool {
    legal_purge_targets(run_state)
        .into_iter()
        .any(|card| is_non_parasite_curse(card) || card.id == CardId::Parasite)
}

fn has_safe_purge_target(run_state: &RunState) -> bool {
    let choice = single_deck_mutation_choice(RunPendingChoiceReason::PurgeNonBottled);
    compile_deck_mutation_decision_v1(
        run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_execute_one(),
    )
    .selected_plan
    .is_some()
}

fn single_deck_mutation_choice(reason: RunPendingChoiceReason) -> RunPendingChoiceState {
    RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: 1,
        max_choices: 1,
        reason,
        source: DomainEventSource::Selection(reason.into()),
        return_state: Box::new(EngineState::EventRoom),
    }
}

fn legal_purge_targets(run_state: &RunState) -> Vec<&CombatCard> {
    run_state
        .master_deck
        .iter()
        .filter(|card| {
            crate::state::core::run_pending_choice_allows_card_for_run(
                &RunPendingChoiceReason::PurgeNonBottled,
                card,
                run_state,
            )
        })
        .collect()
}

fn unupgraded_starter_basic_count(run_state: &RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| {
            is_starter_basic(card.id) && crate::state::core::master_deck_card_can_upgrade(card)
        })
        .count()
}

fn is_non_parasite_curse(card: &CombatCard) -> bool {
    get_card_definition(card.id).card_type == crate::content::cards::CardType::Curse
        && card.id != crate::content::cards::CardId::Parasite
}

fn shining_light_damage(run_state: &RunState) -> i32 {
    let fraction = if run_state.ascension_level >= 15 {
        0.3
    } else {
        0.2
    };
    (run_state.max_hp as f32 * fraction).round() as i32
}

fn event_screen(run_state: &RunState) -> usize {
    run_state
        .event_state
        .as_ref()
        .map(|event| event.current_screen)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::RelicState;
    use crate::state::events::{EventId, EventState};

    fn forgotten_altar_run(current_hp: i32, max_hp: i32) -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.act_num = 2;
        run_state.floor_num = 29;
        run_state.current_hp = current_hp;
        run_state.max_hp = max_hp;
        run_state.event_state = Some(EventState::new(EventId::ForgottenAltar));
        run_state.relics = vec![RelicState::new(RelicId::RunicDome)];
        run_state.master_deck = forgotten_altar_heavy_burden_deck();
        run_state
    }

    fn forgotten_altar_heavy_burden_deck() -> Vec<CombatCard> {
        [
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Headbutt,
            CardId::Armaments,
            CardId::BurningPact,
            CardId::Whirlwind,
            CardId::Feed,
            CardId::ShrugItOff,
            CardId::Cleave,
            CardId::DemonForm,
            CardId::Rupture,
        ]
        .into_iter()
        .enumerate()
        .map(|(idx, id)| CombatCard::for_scenario(id, idx as u32))
        .collect()
    }

    #[test]
    fn forgotten_altar_prays_when_projected_hp_clears_v0_safety_floor() {
        let run_state = forgotten_altar_run(52, 99);

        assert_eq!(
            forgotten_altar_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::GainMaxHp(5))
        );
    }

    #[test]
    fn forgotten_altar_takes_decay_when_projected_hp_is_below_v0_safety_floor() {
        let run_state = forgotten_altar_run(40, 99);

        assert_eq!(
            forgotten_altar_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::ObtainCurse {
                count: 1,
                kind: EventCardKind::Specific(CardId::Decay),
            })
        );
    }

    #[test]
    fn forgotten_altar_offers_golden_idol_before_other_choices() {
        let mut run_state = forgotten_altar_run(20, 99);
        run_state.relics.push(RelicState::new(RelicId::GoldenIdol));

        assert_eq!(
            forgotten_altar_choice(&run_state),
            EventOwnerOptionSelector::Action(EventActionKind::Trade)
        );
    }

    #[test]
    fn forgotten_altar_uses_omamori_to_block_decay_before_losing_hp() {
        let mut run_state = forgotten_altar_run(52, 99);
        run_state.relics.push(RelicState::new(RelicId::Omamori));

        assert_eq!(
            forgotten_altar_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::ObtainCurse {
                count: 1,
                kind: EventCardKind::Specific(CardId::Decay),
            })
        );
    }

    fn event_run(event_id: EventId, current_hp: i32, max_hp: i32, gold: i32) -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = current_hp;
        run_state.max_hp = max_hp;
        run_state.gold = gold;
        run_state.event_state = Some(EventState::new(event_id));
        run_state
    }

    #[test]
    fn world_of_goop_gathers_gold_when_hp_loss_is_safe() {
        let run_state = event_run(EventId::WorldOfGoop, 80, 80, 0);

        assert_eq!(
            world_of_goop_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::GainGold(75))
        );
    }

    fn nest_run(current_hp: i32, ascension_level: u8) -> RunState {
        let mut run_state = event_run(EventId::Nest, current_hp, 80, 0);
        run_state.act_num = 2;
        run_state.floor_num = 24;
        run_state.ascension_level = ascension_level;
        run_state
            .event_state
            .as_mut()
            .expect("Nest event state")
            .current_screen = 1;
        run_state
    }

    #[test]
    fn nest_small_deck_does_not_claim_an_unfunded_growth_commitment() {
        let mut run_state = nest_run(80, 0);
        run_state
            .relics
            .push(RelicState::new(RelicId::FusionHammer));

        assert!(run_state.master_deck.len() < 28);
        assert_eq!(
            nest_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::GainGold(99))
        );
    }

    #[test]
    fn nest_takes_dagger_when_ectoplasm_nullifies_the_gold_alternative() {
        let mut run_state = nest_run(80, 0);
        run_state.relics.push(RelicState::new(RelicId::Ectoplasm));

        assert_eq!(
            nest_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::ObtainColorlessCard {
                count: 1,
                kind: EventCardKind::Specific(CardId::RitualDagger),
            })
        );
    }

    #[test]
    fn nest_does_not_spend_unsafe_hp_even_when_gold_is_unavailable() {
        let mut run_state = nest_run(1, 0);
        run_state.relics.push(RelicState::new(RelicId::Ectoplasm));

        assert_eq!(
            nest_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::GainGold(99))
        );
    }

    #[test]
    fn world_of_goop_leaves_when_hp_loss_is_unsafe() {
        let run_state = event_run(EventId::WorldOfGoop, 20, 80, 100);

        assert_eq!(
            world_of_goop_choice(&run_state),
            EventOwnerOptionSelector::Action(EventActionKind::Leave)
        );
    }

    #[test]
    fn golden_wing_prays_when_hp_and_remove_target_are_safe() {
        let run_state = event_run(EventId::GoldenWing, 80, 80, 0);

        assert_eq!(
            golden_wing_choice(&run_state),
            EventOwnerOptionSelector::Action(EventActionKind::DeckOperation)
        );
    }

    #[test]
    fn golden_wing_destroys_when_remove_hp_is_unsafe_but_attack_is_available() {
        let mut run_state = event_run(EventId::GoldenWing, 20, 80, 0);
        let mut pommel = CombatCard::for_scenario(CardId::PommelStrike, 99);
        pommel.upgrades = 1;
        run_state.master_deck.push(pommel);

        assert_eq!(
            golden_wing_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::GainGoldRange { min: 50, max: 80 })
        );
    }

    #[test]
    fn cleric_purifies_when_hp_is_safe_and_remove_target_exists() {
        let run_state = event_run(EventId::Cleric, 70, 80, 50);

        assert_eq!(
            cleric_choice(&run_state),
            EventOwnerOptionSelector::Action(EventActionKind::DeckOperation)
        );
    }

    #[test]
    fn cleric_heals_before_purify_when_hp_is_dangerous() {
        let run_state = event_run(EventId::Cleric, 20, 80, 100);

        assert_eq!(
            cleric_choice(&run_state),
            EventOwnerOptionSelector::Action(EventActionKind::Trade)
        );
    }

    #[test]
    fn golden_idol_takes_idol_and_defaults_to_hide_payment() {
        let mut run_state = event_run(EventId::GoldenIdol, 80, 80, 0);

        assert_eq!(
            golden_idol_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::Specific(RelicId::GoldenIdol),
            })
        );

        run_state.event_state.as_mut().unwrap().current_screen = 1;
        assert_eq!(
            golden_idol_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::LoseMaxHp(6))
        );
    }

    #[test]
    fn golden_idol_uses_omamori_to_block_injury_payment() {
        let mut run_state = event_run(EventId::GoldenIdol, 80, 80, 0);
        run_state.event_state.as_mut().unwrap().current_screen = 1;
        run_state.relics.push(RelicState::new(RelicId::Omamori));

        assert_eq!(
            golden_idol_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::ObtainCurse {
                count: 1,
                kind: EventCardKind::Specific(CardId::Injury),
            })
        );
    }

    #[test]
    fn golden_idol_uses_blue_candle_to_digest_injury_before_losing_max_hp() {
        let mut run_state = event_run(EventId::GoldenIdol, 72, 85, 143);
        run_state.event_state.as_mut().unwrap().current_screen = 1;
        run_state.relics.push(RelicState::new(RelicId::BlueCandle));

        assert_eq!(
            golden_idol_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::ObtainCurse {
                count: 1,
                kind: EventCardKind::Specific(CardId::Injury),
            })
        );
    }

    #[test]
    fn scrap_ooze_reaches_when_hp_budget_is_safe_and_leaves_when_low() {
        let run_state = event_run(EventId::ScrapOoze, 80, 80, 0);
        assert_eq!(
            scrap_ooze_choice(&run_state),
            EventOwnerOptionSelector::Action(EventActionKind::Special)
        );

        let low_hp = event_run(EventId::ScrapOoze, 14, 80, 0);
        assert_eq!(
            scrap_ooze_choice(&low_hp),
            EventOwnerOptionSelector::Action(EventActionKind::Leave)
        );
    }

    fn masked_bandits_run(deck: &[CardId], current_hp: i32, gold: i32) -> RunState {
        let mut run_state = event_run(EventId::MaskedBandits, current_hp, 80, gold);
        run_state.act_num = 2;
        run_state.floor_num = 19;
        run_state.master_deck = deck
            .iter()
            .copied()
            .enumerate()
            .map(|(index, id)| CombatCard::for_scenario(id, index as u32))
            .collect();
        run_state
    }

    #[test]
    fn masked_bandits_pays_when_high_hp_hides_an_unprepared_multi_enemy_deck() {
        let run_state = masked_bandits_run(
            &[
                CardId::Strike,
                CardId::Strike,
                CardId::Strike,
                CardId::Defend,
                CardId::Defend,
                CardId::Defend,
                CardId::Defend,
                CardId::Bash,
                CardId::Berserk,
                CardId::WildStrike,
                CardId::ShrugItOff,
                CardId::HeavyBlade,
                CardId::Clothesline,
                CardId::TwinStrike,
                CardId::Clash,
                CardId::Intimidate,
                CardId::Evolve,
                CardId::BattleTrance,
                CardId::Shockwave,
                CardId::TrueGrit,
            ],
            72,
            292,
        );

        assert!(!masked_bandits_ready(&run_state));
        assert_eq!(
            masked_bandits_choice(&run_state),
            EventOwnerOptionSelector::Action(EventActionKind::Trade)
        );
    }

    #[test]
    fn masked_bandits_still_fights_with_real_multi_enemy_offense_and_mitigation() {
        let run_state = masked_bandits_run(
            &[
                CardId::Strike,
                CardId::Strike,
                CardId::Strike,
                CardId::Defend,
                CardId::Defend,
                CardId::Defend,
                CardId::Defend,
                CardId::Bash,
                CardId::Immolate,
                CardId::Cleave,
                CardId::FlameBarrier,
                CardId::ShrugItOff,
                CardId::Disarm,
                CardId::Shockwave,
                CardId::BattleTrance,
            ],
            72,
            292,
        );

        assert!(masked_bandits_ready(&run_state));
        assert_eq!(
            masked_bandits_choice(&run_state),
            EventOwnerOptionSelector::Action(EventActionKind::Fight)
        );
    }

    #[test]
    fn masked_bandits_does_not_spend_a_dangerously_low_hp_buffer_even_when_ready() {
        let run_state = masked_bandits_run(
            &[
                CardId::Strike,
                CardId::Strike,
                CardId::Defend,
                CardId::Defend,
                CardId::Bash,
                CardId::Immolate,
                CardId::Cleave,
                CardId::FlameBarrier,
                CardId::ShrugItOff,
                CardId::Disarm,
                CardId::Shockwave,
            ],
            20,
            292,
        );

        assert_eq!(
            masked_bandits_choice(&run_state),
            EventOwnerOptionSelector::Action(EventActionKind::Trade)
        );
    }

    #[test]
    fn drug_dealer_defaults_to_transform_two_when_safe_targets_exist() {
        let run_state = event_run(EventId::DrugDealer, 70, 80, 0);

        assert_eq!(
            drug_dealer_choice(&run_state),
            EventOwnerOptionSelector::OptionIndex(1)
        );
    }

    #[test]
    fn drug_dealer_takes_mutagenic_when_artifact_preserves_strength() {
        let mut run_state = event_run(EventId::DrugDealer, 70, 80, 0);
        run_state
            .relics
            .push(RelicState::new(RelicId::ClockworkSouvenir));

        assert_eq!(
            drug_dealer_choice(&run_state),
            EventOwnerOptionSelector::OptionIndex(2)
        );
    }

    #[test]
    fn bonfire_approaches_and_offers_when_card_can_be_removed() {
        let mut run_state = event_run(EventId::BonfireSpirits, 70, 80, 0);

        assert_eq!(
            bonfire_choice(&run_state),
            EventOwnerOptionSelector::Action(EventActionKind::Continue)
        );

        run_state.event_state.as_mut().unwrap().current_screen = 1;
        assert_eq!(
            bonfire_choice(&run_state),
            EventOwnerOptionSelector::Action(EventActionKind::DeckOperation)
        );
    }

    #[test]
    fn face_trader_touches_when_hp_can_buy_near_term_gold_and_leaves_when_low() {
        let mut run_state = event_run(EventId::FaceTrader, 70, 80, 0);
        run_state.event_state.as_mut().unwrap().current_screen = 1;
        assert_eq!(
            face_trader_choice(&run_state),
            EventOwnerOptionSelector::OptionIndex(0)
        );

        let mut low_hp = event_run(EventId::FaceTrader, 12, 80, 0);
        low_hp.event_state.as_mut().unwrap().current_screen = 1;
        assert_eq!(
            face_trader_choice(&low_hp),
            EventOwnerOptionSelector::OptionIndex(2)
        );
    }

    #[test]
    fn woman_in_blue_never_buys_potions_blocked_by_sozu() {
        let mut run_state = event_run(EventId::WomanInBlue, 70, 80, 300);
        run_state.relics.push(RelicState::new(RelicId::Sozu));

        assert_eq!(
            woman_in_blue_choice(&run_state),
            EventOwnerOptionSelector::Action(EventActionKind::Leave)
        );
    }

    #[test]
    fn woman_in_blue_still_buys_for_real_empty_slots_without_sozu() {
        let run_state = event_run(EventId::WomanInBlue, 70, 80, 300);

        assert_eq!(
            woman_in_blue_choice(&run_state),
            EventOwnerOptionSelector::Effect(EventEffect::ObtainPotion { count: 3 })
        );
    }

    #[test]
    fn ghosts_declines_when_existing_mitigation_makes_half_max_hp_an_unneeded_trade() {
        let mut run_state = event_run(EventId::Ghosts, 62, 85, 82);
        run_state.act_num = 2;
        run_state.floor_num = 30;
        run_state.relics = [
            RelicId::BurningBlood,
            RelicId::CeramicFish,
            RelicId::MealTicket,
            RelicId::Ectoplasm,
            RelicId::ArtOfWar,
        ]
        .into_iter()
        .map(RelicState::new)
        .collect();
        run_state.master_deck = [
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Blind,
            CardId::ThunderClap,
            CardId::TwinStrike,
            CardId::BurningPact,
            CardId::BurningPact,
            CardId::Immolate,
            CardId::DarkEmbrace,
            CardId::ShrugItOff,
            CardId::ShrugItOff,
            CardId::Uppercut,
            CardId::Inflame,
            CardId::Entrench,
            CardId::Impervious,
            CardId::Whirlwind,
            CardId::TrueGrit,
            CardId::PommelStrike,
            CardId::Disarm,
            CardId::Intimidate,
            CardId::Reaper,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, id)| CombatCard::for_scenario(id, index as u32))
        .collect();

        let deficit = assess_deck_strategic_deficit_summary(
            &run_state.master_deck,
            RunStrategicFacts::from_run_state(&run_state),
        );
        assert!(
            matches!(
                deficit.block_or_mitigation,
                StrategicDeficitLevel::Adequate | StrategicDeficitLevel::Surplus
            ),
            "the regression deck must already have a real mitigation package: {deficit:?}"
        );
        assert_eq!(
            ghosts_choice(&run_state),
            EventOwnerOptionSelector::OptionIndex(1)
        );
    }

    #[test]
    fn ghosts_still_accepts_as_a_real_mitigation_repair() {
        let mut run_state = event_run(EventId::Ghosts, 72, 80, 0);
        run_state.act_num = 2;
        run_state.floor_num = 24;
        run_state.master_deck = [
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Bash,
            CardId::Clash,
            CardId::WildStrike,
            CardId::TwinStrike,
            CardId::HeavyBlade,
            CardId::SwordBoomerang,
            CardId::Flex,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, id)| CombatCard::for_scenario(id, index as u32))
        .collect();

        let deficit = assess_deck_strategic_deficit_summary(
            &run_state.master_deck,
            RunStrategicFacts::from_run_state(&run_state),
        );
        assert!(
            matches!(
                deficit.block_or_mitigation,
                StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
            ),
            "the negative control must actually lack mitigation: {deficit:?}"
        );
        assert_eq!(
            ghosts_choice(&run_state),
            EventOwnerOptionSelector::OptionIndex(0)
        );
    }

    #[test]
    fn ghosts_keeps_a_low_hp_exhaust_supported_line_when_max_hp_loss_does_not_clamp_hp() {
        let mut run_state = event_run(EventId::Ghosts, 34, 80, 224);
        run_state.act_num = 2;
        run_state.floor_num = 19;
        run_state.relics = [
            RelicId::BurningBlood,
            RelicId::PaperFrog,
            RelicId::IncenseBurner,
            RelicId::Ectoplasm,
        ]
        .into_iter()
        .map(RelicState::new)
        .collect();
        run_state.master_deck = [
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Brutality,
            CardId::FeelNoPain,
            CardId::WildStrike,
            CardId::FlameBarrier,
            CardId::ShrugItOff,
            CardId::Blind,
            CardId::Cleave,
            CardId::Armaments,
            CardId::Disarm,
            CardId::Cleave,
            CardId::DemonForm,
            CardId::FlameBarrier,
            CardId::BattleTrance,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, id)| CombatCard::for_scenario(id, index as u32))
        .collect();

        assert_eq!(run_state.current_hp.min(run_state.max_hp / 2), 34);
        assert_eq!(
            ghosts_choice(&run_state),
            EventOwnerOptionSelector::OptionIndex(0)
        );
    }
}

#[cfg(test)]
mod coverage_tests;
