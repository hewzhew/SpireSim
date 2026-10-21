use crate::content::relics::RelicTier;
use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
/// Neow Event — the starting blessing event.
///
/// Java: NeowEvent.java + NeowReward.java
///
/// Two modes:
/// - **miniBlessing** (bossCount == 0): 2 fixed choices — NeowsLament or +10% MaxHP
/// - **blessing** (bossCount > 0): 4 categories randomly selected:
///   Cat 0: 3 cards / rare card / remove card / upgrade card / transform card / random colorless
///   Cat 1: 3 potions / common relic / +10% HP / NeowsLament / +100 gold
///   Cat 2: drawback + rare relic / remove 2 / 3 rare cards / +250 gold / transform 2 / +20% HP / random colorless 2
///   Cat 3: swap starter relic for boss relic
///
/// screens:
///   0 = initial dialog (click to advance to choices)
///   1 = choices displayed
///   2 = completed
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventOption,
    EventOptionSemantics, EventOptionTransition, EventRelicKind, EventSelectionKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;
use serde::{Deserialize, Serialize};

/// Source-audited run-start input corresponding to Java's captured
/// `<CLASS>_SPIRITS` eligibility. Profile persistence and the special replay
/// edge are deliberately owned outside the run state.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NeowEligibilityV1 {
    Mini,
    #[default]
    Rich,
}

/// Neow reward types
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NeowRewardType {
    ThreeEnemyKill,       // NeowsLament relic
    TenPercentHpBonus,    // +10% maxHP
    ThreeCards,           // Choose from 3 cards
    OneRandomRareCard,    // Obtain 1 random rare
    RemoveCard,           // Remove 1 card
    UpgradeCard,          // Upgrade 1 card
    TransformCard,        // Transform 1 card
    RandomColorless,      // Choose from 3 colorless
    ThreeSmallPotions,    // Obtain 3 random potions
    RandomCommonRelic,    // Obtain random common relic
    HundredGold,          // +100 gold
    RandomColorless2,     // Choose from 3 rare colorless (with drawback)
    RemoveTwo,            // Remove 2 cards (with drawback)
    OneRareRelic,         // Obtain random rare relic (with drawback)
    ThreeRareCards,       // Choose from 3 rare cards (with drawback)
    TwoFiftyGold,         // +250 gold (with drawback)
    TransformTwoCards,    // Transform 2 cards (with drawback)
    TwentyPercentHpBonus, // +20% maxHP (with drawback)
    BossRelic,            // Swap starter relic for boss relic
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NeowDrawback {
    None,
    TenPercentHpLoss,
    NoGold,
    Curse,
    PercentDamage,
}

/// Stored in event_state.extra_data as encoded i32s:
/// [0] = number of choices
/// [1..N] = reward type for each choice (as i32)
/// [N+1..2N] = drawback type for each choice (as i32)
fn encode_reward(r: NeowRewardType) -> i32 {
    match r {
        NeowRewardType::ThreeEnemyKill => 0,
        NeowRewardType::TenPercentHpBonus => 1,
        NeowRewardType::ThreeCards => 2,
        NeowRewardType::OneRandomRareCard => 3,
        NeowRewardType::RemoveCard => 4,
        NeowRewardType::UpgradeCard => 5,
        NeowRewardType::TransformCard => 6,
        NeowRewardType::RandomColorless => 7,
        NeowRewardType::ThreeSmallPotions => 8,
        NeowRewardType::RandomCommonRelic => 9,
        NeowRewardType::HundredGold => 10,
        NeowRewardType::RandomColorless2 => 11,
        NeowRewardType::RemoveTwo => 12,
        NeowRewardType::OneRareRelic => 13,
        NeowRewardType::ThreeRareCards => 14,
        NeowRewardType::TwoFiftyGold => 15,
        NeowRewardType::TransformTwoCards => 16,
        NeowRewardType::TwentyPercentHpBonus => 17,
        NeowRewardType::BossRelic => 18,
    }
}

fn decode_reward(v: i32) -> NeowRewardType {
    match v {
        0 => NeowRewardType::ThreeEnemyKill,
        1 => NeowRewardType::TenPercentHpBonus,
        2 => NeowRewardType::ThreeCards,
        3 => NeowRewardType::OneRandomRareCard,
        4 => NeowRewardType::RemoveCard,
        5 => NeowRewardType::UpgradeCard,
        6 => NeowRewardType::TransformCard,
        7 => NeowRewardType::RandomColorless,
        8 => NeowRewardType::ThreeSmallPotions,
        9 => NeowRewardType::RandomCommonRelic,
        10 => NeowRewardType::HundredGold,
        11 => NeowRewardType::RandomColorless2,
        12 => NeowRewardType::RemoveTwo,
        13 => NeowRewardType::OneRareRelic,
        14 => NeowRewardType::ThreeRareCards,
        15 => NeowRewardType::TwoFiftyGold,
        16 => NeowRewardType::TransformTwoCards,
        17 => NeowRewardType::TwentyPercentHpBonus,
        18 => NeowRewardType::BossRelic,
        _ => NeowRewardType::TenPercentHpBonus,
    }
}

fn decode_drawback(v: i32) -> NeowDrawback {
    match v {
        0 => NeowDrawback::None,
        1 => NeowDrawback::TenPercentHpLoss,
        2 => NeowDrawback::NoGold,
        3 => NeowDrawback::Curse,
        4 => NeowDrawback::PercentDamage,
        _ => NeowDrawback::None,
    }
}

/// Stable simulator-owned identity for one visible Neow reward button.
/// Display text and UI position are deliberately not part of this identity.
pub fn choice_identity(
    event_state: &EventState,
    choice_idx: usize,
) -> Option<(NeowRewardType, NeowDrawback)> {
    if event_state.id != crate::state::events::EventId::Neow || event_state.current_screen != 1 {
        return None;
    }
    let count = event_state.extra_data.first().copied()?.max(0) as usize;
    if choice_idx >= count || event_state.extra_data.len() < 1 + count * 2 {
        return None;
    }
    Some((
        decode_reward(event_state.extra_data[1 + choice_idx]),
        decode_drawback(event_state.extra_data[1 + count + choice_idx]),
    ))
}

fn encode_drawback(d: NeowDrawback) -> i32 {
    match d {
        NeowDrawback::None => 0,
        NeowDrawback::TenPercentHpLoss => 1,
        NeowDrawback::NoGold => 2,
        NeowDrawback::Curse => 3,
        NeowDrawback::PercentDamage => 4,
    }
}

fn reward_label(r: NeowRewardType, hp_bonus: i32) -> String {
    match r {
        NeowRewardType::ThreeEnemyKill => {
            "Enemies in your next three combats have 1 HP.".to_string()
        }
        NeowRewardType::TenPercentHpBonus => format!("Max HP +{}", hp_bonus),
        NeowRewardType::ThreeCards => "Choose a card to obtain.".to_string(),
        NeowRewardType::OneRandomRareCard => "Obtain a random rare card.".to_string(),
        NeowRewardType::RemoveCard => "Remove a card.".to_string(),
        NeowRewardType::UpgradeCard => "Upgrade a card.".to_string(),
        NeowRewardType::TransformCard => "Transform a card.".to_string(),
        NeowRewardType::RandomColorless => "Choose a colorless card to obtain.".to_string(),
        NeowRewardType::ThreeSmallPotions => "Obtain 3 random potions.".to_string(),
        NeowRewardType::RandomCommonRelic => "Obtain a random common relic.".to_string(),
        NeowRewardType::HundredGold => "Obtain 100 Gold.".to_string(),
        NeowRewardType::RandomColorless2 => "Choose a rare colorless card to obtain.".to_string(),
        NeowRewardType::RemoveTwo => "Remove 2 cards.".to_string(),
        NeowRewardType::OneRareRelic => "Obtain a random rare relic.".to_string(),
        NeowRewardType::ThreeRareCards => "Choose a rare card to obtain.".to_string(),
        NeowRewardType::TwoFiftyGold => "Obtain 250 Gold.".to_string(),
        NeowRewardType::TransformTwoCards => "Transform 2 cards.".to_string(),
        NeowRewardType::TwentyPercentHpBonus => format!("Max HP +{}", hp_bonus * 2),
        NeowRewardType::BossRelic => {
            "Obtain a random Boss Relic. Lose your starter Relic.".to_string()
        }
    }
}

fn drawback_label(d: NeowDrawback, hp_bonus: i32, dmg: i32) -> String {
    match d {
        NeowDrawback::None => String::new(),
        NeowDrawback::TenPercentHpLoss => format!("Lose {} Max HP. ", hp_bonus),
        NeowDrawback::NoGold => "Lose all Gold. ".to_string(),
        NeowDrawback::Curse => "Obtain a Curse. ".to_string(),
        NeowDrawback::PercentDamage => format!("Take {} damage. ", dmg),
    }
}

const MINI_ELIGIBILITY_STATE: i32 = 0;
const RICH_ELIGIBILITY_STATE: i32 = 1;

/// Set up the Neow introduction without manufacturing future rewards.
///
/// Java creates `NeowEvent.rng` and the four Rich rewards only when the
/// introduction's button transition calls `blessing()`. Mini Neow never
/// creates that RNG at all.
pub fn setup_neow_event(run_state: &mut RunState, eligibility: NeowEligibilityV1) {
    run_state.neow_rng = None;
    run_state.event_state = Some(EventState {
        id: crate::state::events::EventId::Neow,
        current_screen: 0,
        internal_state: encode_eligibility(eligibility),
        completed: false,
        combat_pending: false,
        prepared_encounter: None,
        card_payload: Vec::new(),
        extra_data: Vec::new(),
    });
}

fn encode_eligibility(eligibility: NeowEligibilityV1) -> i32 {
    match eligibility {
        NeowEligibilityV1::Mini => MINI_ELIGIBILITY_STATE,
        NeowEligibilityV1::Rich => RICH_ELIGIBILITY_STATE,
    }
}

/// Reads the Mini/Rich branch captured when this Neow event was initialized.
/// The active event, rather than a projection or display label, owns it.
pub fn event_eligibility(event_state: &EventState) -> Result<NeowEligibilityV1, String> {
    if event_state.id != crate::state::events::EventId::Neow {
        return Err("event state is not Neow".to_owned());
    }
    match event_state.internal_state {
        MINI_ELIGIBILITY_STATE => Ok(NeowEligibilityV1::Mini),
        RICH_ELIGIBILITY_STATE => Ok(NeowEligibilityV1::Rich),
        other => Err(format!("invalid Neow eligibility state {other}")),
    }
}

fn generate_neow_choices(run_state: &mut RunState, event_state: &mut EventState) {
    let mut extra = Vec::new();

    if event_eligibility(event_state)
        .expect("Neow event must retain its initialization eligibility")
        == NeowEligibilityV1::Mini
    {
        // miniBlessing: 2 fixed choices
        run_state.neow_rng = None;
        extra.push(2); // count
        extra.push(encode_reward(NeowRewardType::ThreeEnemyKill));
        extra.push(encode_reward(NeowRewardType::TenPercentHpBonus));
        extra.push(encode_drawback(NeowDrawback::None));
        extra.push(encode_drawback(NeowDrawback::None));
    } else {
        // blessing: 4 categories, each pick one reward
        let neow_rng = run_state
            .neow_rng
            .insert(crate::runtime::rng::StsRng::new_named(
                run_state.seed,
                "run.neow",
            ));

        // Category 0: small bonuses
        let cat0_options = [
            NeowRewardType::ThreeCards,
            NeowRewardType::OneRandomRareCard,
            NeowRewardType::RemoveCard,
            NeowRewardType::UpgradeCard,
            NeowRewardType::TransformCard,
            NeowRewardType::RandomColorless,
        ];
        let r0 = cat0_options[neow_rng.random_range(0, cat0_options.len() as i32 - 1) as usize];

        // Category 1: medium bonuses
        let cat1_options = [
            NeowRewardType::ThreeSmallPotions,
            NeowRewardType::RandomCommonRelic,
            NeowRewardType::TenPercentHpBonus,
            NeowRewardType::ThreeEnemyKill,
            NeowRewardType::HundredGold,
        ];
        let r1 = cat1_options[neow_rng.random_range(0, cat1_options.len() as i32 - 1) as usize];

        // Category 2: big bonuses with drawback
        // Roll drawback first (Java: getRewardDrawbackOptions)
        let drawback_options = [
            NeowDrawback::TenPercentHpLoss,
            NeowDrawback::NoGold,
            NeowDrawback::Curse,
            NeowDrawback::PercentDamage,
        ];
        let drawback =
            drawback_options[neow_rng.random_range(0, drawback_options.len() as i32 - 1) as usize];

        // Build cat2 options with conditional filtering based on drawback
        let mut cat2_options = vec![NeowRewardType::RandomColorless2];
        if drawback != NeowDrawback::Curse {
            cat2_options.push(NeowRewardType::RemoveTwo);
        }
        cat2_options.push(NeowRewardType::OneRareRelic);
        cat2_options.push(NeowRewardType::ThreeRareCards);
        if drawback != NeowDrawback::NoGold {
            cat2_options.push(NeowRewardType::TwoFiftyGold);
        }
        cat2_options.push(NeowRewardType::TransformTwoCards);
        if drawback != NeowDrawback::TenPercentHpLoss {
            cat2_options.push(NeowRewardType::TwentyPercentHpBonus);
        }
        let r2 = cat2_options[neow_rng.random_range(0, cat2_options.len() as i32 - 1) as usize];

        // Category 3 has only Boss Relic, but Java still constructs a
        // NeowReward and calls rng.random(0, 0) for that singleton list.
        // The result is fixed while the RNG consumption is not optional.
        let _singleton_roll = neow_rng.random_range(0, 0);
        let r3 = NeowRewardType::BossRelic;

        extra.push(4); // count
        extra.push(encode_reward(r0));
        extra.push(encode_reward(r1));
        extra.push(encode_reward(r2));
        extra.push(encode_reward(r3));
        extra.push(encode_drawback(NeowDrawback::None));
        extra.push(encode_drawback(NeowDrawback::None));
        extra.push(encode_drawback(drawback));
        extra.push(encode_drawback(NeowDrawback::None)); // Boss relic has no drawback enum (it IS the drawback)
    }

    event_state.extra_data = extra;
}

pub fn get_choices(run_state: &RunState, event_state: &EventState) -> Vec<EventChoiceMeta> {
    let hp_bonus = (run_state.max_hp as f32 * 0.1) as i32;
    let percent_dmg = run_state.current_hp / 10 * 3;

    match event_state.current_screen {
        0 => {
            // Initial dialog — single "proceed" button
            vec![EventChoiceMeta::new("[Proceed]")]
        }
        1 => {
            // Display reward choices
            let count = event_state.extra_data[0] as usize;
            let mut choices = Vec::new();
            for i in 0..count {
                let reward = decode_reward(event_state.extra_data[1 + i]);
                let drawback = decode_drawback(event_state.extra_data[1 + count + i]);
                let db_label = drawback_label(drawback, hp_bonus, percent_dmg);
                let rw_label = reward_label(reward, hp_bonus);
                choices.push(EventChoiceMeta::new(format!("{}{}", db_label, rw_label)));
            }
            choices
        }
        _ => {
            vec![EventChoiceMeta::new("[Leave]")]
        }
    }
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    get_choices(run_state, event_state)
        .into_iter()
        .enumerate()
        .map(|(index, ui)| {
            let semantics = neow_option_semantics(run_state, event_state, index);
            EventOption::new(ui, semantics)
        })
        .collect()
}

fn neow_option_semantics(
    run_state: &RunState,
    event_state: &EventState,
    choice_idx: usize,
) -> EventOptionSemantics {
    match event_state.current_screen {
        0 => EventOptionSemantics {
            action: EventActionKind::Continue,
            transition: EventOptionTransition::AdvanceScreen,
            ..EventOptionSemantics::default()
        },
        1 => {
            let count = event_state.extra_data.first().copied().unwrap_or(0).max(0) as usize;
            if choice_idx >= count || event_state.extra_data.len() < 1 + count * 2 {
                return EventOptionSemantics::default();
            }
            let reward = decode_reward(event_state.extra_data[1 + choice_idx]);
            let drawback = decode_drawback(event_state.extra_data[1 + count + choice_idx]);
            let mut effects = neow_drawback_effects(run_state, drawback);
            effects.extend(neow_reward_effects(run_state, reward));
            EventOptionSemantics {
                action: neow_action_kind(reward),
                effects,
                transition: neow_reward_transition(reward),
                terminal: true,
                ..EventOptionSemantics::default()
            }
        }
        _ => EventOptionSemantics {
            action: EventActionKind::Leave,
            transition: EventOptionTransition::Complete,
            terminal: true,
            ..EventOptionSemantics::default()
        },
    }
}

fn neow_action_kind(reward: NeowRewardType) -> EventActionKind {
    match reward {
        NeowRewardType::RemoveCard
        | NeowRewardType::RemoveTwo
        | NeowRewardType::UpgradeCard
        | NeowRewardType::TransformCard
        | NeowRewardType::TransformTwoCards => EventActionKind::DeckOperation,
        _ => EventActionKind::Gain,
    }
}

fn neow_reward_transition(reward: NeowRewardType) -> EventOptionTransition {
    match reward {
        NeowRewardType::RemoveCard | NeowRewardType::RemoveTwo => {
            EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard)
        }
        NeowRewardType::UpgradeCard => {
            EventOptionTransition::OpenSelection(EventSelectionKind::UpgradeCard)
        }
        NeowRewardType::TransformCard | NeowRewardType::TransformTwoCards => {
            EventOptionTransition::OpenSelection(EventSelectionKind::TransformCard)
        }
        NeowRewardType::ThreeSmallPotions
        | NeowRewardType::ThreeCards
        | NeowRewardType::ThreeRareCards
        | NeowRewardType::RandomColorless
        | NeowRewardType::RandomColorless2 => EventOptionTransition::OpenReward,
        _ => EventOptionTransition::Complete,
    }
}

fn neow_drawback_effects(run_state: &RunState, drawback: NeowDrawback) -> Vec<EventEffect> {
    let hp_bonus = (run_state.max_hp as f32 * 0.1) as i32;
    let percent_dmg = run_state.current_hp / 10 * 3;
    match drawback {
        NeowDrawback::None => Vec::new(),
        NeowDrawback::TenPercentHpLoss => vec![EventEffect::LoseMaxHp(hp_bonus)],
        NeowDrawback::NoGold => vec![EventEffect::LoseGold(run_state.gold)],
        NeowDrawback::Curse => vec![EventEffect::ObtainCurse {
            count: 1,
            kind: EventCardKind::RandomCurse,
        }],
        NeowDrawback::PercentDamage => vec![EventEffect::LoseHp(percent_dmg)],
    }
}

fn neow_reward_effects(run_state: &RunState, reward: NeowRewardType) -> Vec<EventEffect> {
    let hp_bonus = (run_state.max_hp as f32 * 0.1) as i32;
    match reward {
        NeowRewardType::ThreeEnemyKill => vec![EventEffect::ObtainRelic {
            count: 1,
            kind: EventRelicKind::Specific(crate::content::relics::RelicId::NeowsLament),
        }],
        NeowRewardType::TenPercentHpBonus => vec![EventEffect::GainMaxHp(hp_bonus)],
        NeowRewardType::TwentyPercentHpBonus => vec![EventEffect::GainMaxHp(hp_bonus * 2)],
        NeowRewardType::HundredGold => vec![EventEffect::GainGold(100)],
        NeowRewardType::TwoFiftyGold => vec![EventEffect::GainGold(250)],
        NeowRewardType::RandomCommonRelic => vec![EventEffect::ObtainRelic {
            count: 1,
            kind: EventRelicKind::RandomCommonRelic,
        }],
        NeowRewardType::OneRareRelic => vec![EventEffect::ObtainRelic {
            count: 1,
            kind: EventRelicKind::RandomRareRelic,
        }],
        NeowRewardType::BossRelic => vec![
            EventEffect::LoseStarterRelic {
                specific: run_state.relics.first().map(|relic| relic.id),
            },
            EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::RandomBossRelic,
            },
        ],
        NeowRewardType::OneRandomRareCard => vec![EventEffect::ObtainCard {
            count: 1,
            kind: EventCardKind::RandomClassRare,
        }],
        NeowRewardType::RemoveCard => vec![EventEffect::RemoveCard {
            count: 1,
            target_lineage: None,
            kind: EventCardKind::Unknown,
        }],
        NeowRewardType::RemoveTwo => vec![EventEffect::RemoveCard {
            count: 2,
            target_lineage: None,
            kind: EventCardKind::Unknown,
        }],
        NeowRewardType::UpgradeCard => vec![EventEffect::UpgradeCard { count: 1 }],
        NeowRewardType::TransformCard => vec![EventEffect::TransformCard { count: 1 }],
        NeowRewardType::TransformTwoCards => vec![EventEffect::TransformCard { count: 2 }],
        NeowRewardType::ThreeSmallPotions => vec![EventEffect::ObtainPotion { count: 3 }],
        NeowRewardType::ThreeCards => vec![EventEffect::OfferCards {
            count: 3,
            kind: EventCardKind::RandomClassCommonOrUncommon,
        }],
        NeowRewardType::ThreeRareCards => vec![EventEffect::OfferCards {
            count: 3,
            kind: EventCardKind::RandomClassRare,
        }],
        NeowRewardType::RandomColorless => vec![EventEffect::OfferCards {
            count: 3,
            kind: EventCardKind::RandomColorlessUncommon,
        }],
        NeowRewardType::RandomColorless2 => vec![EventEffect::OfferCards {
            count: 3,
            kind: EventCardKind::RandomColorlessRare,
        }],
    }
}

pub fn handle_choice(engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            // Java generates Mini/Rich rewards as part of this committed
            // dialog transition, not while constructing the run.
            generate_neow_choices(run_state, &mut event_state);
            event_state.current_screen = 1;
        }
        1 => {
            let count = event_state.extra_data[0] as usize;
            if choice_idx < count {
                let reward = decode_reward(event_state.extra_data[1 + choice_idx]);
                let drawback = decode_drawback(event_state.extra_data[1 + count + choice_idx]);

                // Apply drawback first (Java: NeowReward.activate())
                apply_drawback(run_state, drawback);

                // Apply reward
                apply_reward(engine_state, run_state, reward, &mut event_state);
            }
            event_state.current_screen = 2;
            event_state.completed = true;
        }
        _ => {
            // Leave — already completed
        }
    }

    run_state.event_state = Some(event_state);
}

fn apply_drawback(run_state: &mut RunState, drawback: NeowDrawback) {
    let hp_bonus = (run_state.max_hp as f32 * 0.1) as i32;
    match drawback {
        NeowDrawback::None => {}
        NeowDrawback::TenPercentHpLoss => {
            run_state.lose_max_hp_with_source(
                hp_bonus,
                DomainEventSource::Event(crate::state::events::EventId::Neow),
            );
        }
        NeowDrawback::NoGold => {
            run_state.set_gold_with_source(
                0,
                DomainEventSource::Event(crate::state::events::EventId::Neow),
            );
        }
        NeowDrawback::Curse => {
            // Java: getCardWithoutRng(CURSE) -> returnRandomCurse() ->
            // CardLibrary.getCurse(), which consumes one cardRng index.
            let curse = run_state.random_obtainable_curse();
            super::obtain_event_card(run_state, crate::state::events::EventId::Neow, curse);
        }
        NeowDrawback::PercentDamage => {
            let dmg = run_state.current_hp / 10 * 3;
            run_state.set_current_hp_with_source(
                (run_state.current_hp - dmg).max(1),
                DomainEventSource::Event(crate::state::events::EventId::Neow),
            );
        }
    }
}

fn apply_reward(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    reward: NeowRewardType,
    _event_state: &mut EventState,
) {
    let hp_bonus = (run_state.max_hp as f32 * 0.1) as i32;

    match reward {
        NeowRewardType::ThreeEnemyKill => {
            // Obtain NeowsLament relic
            let relic_id = crate::content::relics::RelicId::NeowsLament;
            if let Some(next_state) = run_state.obtain_relic_with_source(
                relic_id,
                crate::state::core::EngineState::EventRoom,
                DomainEventSource::Event(crate::state::events::EventId::Neow),
            ) {
                *engine_state = next_state;
            }
        }
        NeowRewardType::TenPercentHpBonus => {
            run_state.gain_max_hp_with_source(
                hp_bonus,
                hp_bonus,
                DomainEventSource::Event(crate::state::events::EventId::Neow),
            );
        }
        NeowRewardType::TwentyPercentHpBonus => {
            run_state.gain_max_hp_with_source(
                hp_bonus * 2,
                hp_bonus * 2,
                DomainEventSource::Event(crate::state::events::EventId::Neow),
            );
        }
        NeowRewardType::HundredGold => {
            run_state.change_gold_with_source(
                100,
                DomainEventSource::Event(crate::state::events::EventId::Neow),
            );
        }
        NeowRewardType::TwoFiftyGold => {
            run_state.change_gold_with_source(
                250,
                DomainEventSource::Event(crate::state::events::EventId::Neow),
            );
        }
        NeowRewardType::RandomCommonRelic => {
            let relic_id = run_state.random_relic_by_tier(RelicTier::Common);
            if let Some(next_state) = run_state.obtain_relic_with_source(
                relic_id,
                crate::state::core::EngineState::EventRoom,
                DomainEventSource::Event(crate::state::events::EventId::Neow),
            ) {
                *engine_state = next_state;
            }
        }
        NeowRewardType::OneRareRelic => {
            let relic_id = run_state.random_relic_by_tier(RelicTier::Rare);
            if let Some(next_state) = run_state.obtain_relic_with_source(
                relic_id,
                crate::state::core::EngineState::EventRoom,
                DomainEventSource::Event(crate::state::events::EventId::Neow),
            ) {
                *engine_state = next_state;
            }
        }
        NeowRewardType::BossRelic => {
            // Remove starter relic (first relic)
            let _ = run_state.remove_relic_at_with_source(
                0,
                DomainEventSource::Event(crate::state::events::EventId::Neow),
            );
            // Obtain random boss relic
            let relic_id = run_state.random_relic_by_tier(RelicTier::Boss);
            // Trigger effects like Pandora's Box transforming the deck.
            if let Some(next_state) = run_state.obtain_relic_with_source(
                relic_id,
                crate::state::core::EngineState::EventRoom,
                DomainEventSource::Event(crate::state::events::EventId::Neow),
            ) {
                *engine_state = next_state;
            }
        }
        NeowRewardType::OneRandomRareCard => {
            // Get a random rare card from the player's class pool and add to deck
            let pool = crate::engine::campfire_handler::nonempty_card_pool_for_class(
                run_state.player_class,
                crate::content::cards::CardRarity::Rare,
            );
            if !pool.is_empty() {
                let idx = neow_random_index(run_state, pool.len());
                super::obtain_event_card(run_state, crate::state::events::EventId::Neow, pool[idx]);
            }
        }
        NeowRewardType::RemoveCard => {
            // Trigger RunPendingChoice for card removal
            *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                selected: Vec::new(),
                min_choices: 1,
                max_choices: 1,
                reason: RunPendingChoiceReason::Purge,
                source: DomainEventSource::Event(crate::state::events::EventId::Neow),
                return_state: Box::new(EngineState::EventRoom),
            });
        }
        NeowRewardType::RemoveTwo => {
            *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                selected: Vec::new(),
                min_choices: 2,
                max_choices: 2,
                reason: RunPendingChoiceReason::Purge,
                source: DomainEventSource::Event(crate::state::events::EventId::Neow),
                return_state: Box::new(EngineState::EventRoom),
            });
        }
        NeowRewardType::UpgradeCard => {
            *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                selected: Vec::new(),
                min_choices: 1,
                max_choices: 1,
                reason: RunPendingChoiceReason::Upgrade,
                source: DomainEventSource::Event(crate::state::events::EventId::Neow),
                return_state: Box::new(EngineState::EventRoom),
            });
        }
        NeowRewardType::TransformCard | NeowRewardType::TransformTwoCards => {
            let count = if reward == NeowRewardType::TransformTwoCards {
                2
            } else {
                1
            };
            *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                selected: Vec::new(),
                min_choices: count,
                max_choices: count,
                reason: RunPendingChoiceReason::Transform,
                source: DomainEventSource::Event(crate::state::events::EventId::Neow),
                return_state: Box::new(EngineState::EventRoom),
            });
        }
        NeowRewardType::ThreeSmallPotions => {
            // Java Neow adds potion rewards through PotionHelper.getRandomPotion
            // and opens the reward screen; it does not directly fill slots.
            let mut reward_state = crate::rewards::state::RewardState::new();
            for _ in 0..3 {
                let potion_id = run_state.random_potion_flat();
                reward_state
                    .items
                    .push(crate::rewards::state::RewardItem::Potion { potion_id });
            }
            // CombatRewardScreen.open() generates an ordinary card row before
            // NeowReward removes that row. Preserve its rarity/upgrade RNG and
            // cardBlizzRandomizer effects even though no cards are offered.
            let _ = crate::rewards::generator::generate_card_reward_items(
                run_state, false, false, false,
            );
            *engine_state = EngineState::RewardScreen(reward_state);
        }
        NeowRewardType::ThreeCards => {
            // Generate 3 card choices from player's class pool (mixed rarity)
            let cards = generate_neow_class_cards(run_state, false);
            *engine_state = EngineState::RewardScreen(
                crate::rewards::state::RewardState::neow_card_choice(cards),
            );
        }
        NeowRewardType::ThreeRareCards => {
            // Generate 3 rare card choices from player's class pool
            let cards = generate_neow_class_cards(run_state, true);
            *engine_state = EngineState::RewardScreen(
                crate::rewards::state::RewardState::neow_card_choice(cards),
            );
        }
        NeowRewardType::RandomColorless => {
            // Java rollRarity only returns common/uncommon, then maps common
            // colorless rewards to uncommon. This reward is therefore three
            // uncommon colorless choices.
            let cards = generate_neow_colorless_cards(run_state, false);
            *engine_state = EngineState::RewardScreen(
                crate::rewards::state::RewardState::neow_card_choice(cards),
            );
        }
        NeowRewardType::RandomColorless2 => {
            // 3 rare colorless cards
            let cards = generate_neow_colorless_cards(run_state, true);
            *engine_state = EngineState::RewardScreen(
                crate::rewards::state::RewardState::neow_card_choice(cards),
            );
        }
    }
}

fn neow_random_index(run_state: &mut RunState, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    run_state
        .neow_rng
        .as_mut()
        .expect("Neow random reward requires an initialized Rich Neow RNG")
        .random(len as i32 - 1) as usize
}

fn neow_roll_rarity(run_state: &mut RunState) -> crate::content::cards::CardRarity {
    use crate::content::cards::CardRarity;
    if run_state
        .neow_rng
        .as_mut()
        .expect("Neow card rarity requires an initialized Rich Neow RNG")
        .random_boolean_chance(0.33)
    {
        CardRarity::Uncommon
    } else {
        CardRarity::Common
    }
}

fn neow_pick_unique_card(
    run_state: &mut RunState,
    pool: &[crate::content::cards::CardId],
    selected: &[crate::content::cards::CardId],
) -> Option<crate::content::cards::CardId> {
    if pool.is_empty() {
        return None;
    }

    loop {
        let card_id = pool[neow_random_index(run_state, pool.len())];
        if !selected.contains(&card_id) {
            return Some(card_id);
        }
    }
}

/// Generate 3 cards from the player's class card pool.
/// If `rare_only` is true, all 3 cards come from the Rare pool.
/// Otherwise, Java Neow rolls only Common/Uncommon: 33% Uncommon, else Common.
fn generate_neow_class_cards(
    run_state: &mut RunState,
    rare_only: bool,
) -> Vec<crate::rewards::state::RewardCard> {
    use crate::content::cards::CardRarity;
    let mut cards = Vec::new();
    let mut selected_ids = Vec::new();
    for _ in 0..3 {
        let mut rarity = neow_roll_rarity(run_state);
        if rare_only {
            rarity = CardRarity::Rare;
        }
        let pool = crate::engine::campfire_handler::nonempty_card_pool_for_class(
            run_state.player_class,
            rarity,
        );
        if let Some(card_id) = neow_pick_unique_card(run_state, pool, &selected_ids) {
            selected_ids.push(card_id);
            cards.push(crate::rewards::state::RewardCard::new(
                card_id,
                run_state.preview_obtain_card_upgrades(card_id, 0),
            ));
        }
    }
    cards
}

/// Generate 3 colorless cards for Neow rewards.
/// If `rare_only` is true, picks from rare colorless pool only.
/// Otherwise picks from uncommon only: Java rollRarity returns common/uncommon,
/// and getColorlessRewardCards maps common to uncommon.
fn generate_neow_colorless_cards(
    run_state: &mut RunState,
    rare_only: bool,
) -> Vec<crate::rewards::state::RewardCard> {
    let mut cards = Vec::new();
    let mut selected_ids = Vec::new();
    for _ in 0..3 {
        let mut rarity = neow_roll_rarity(run_state);
        if rare_only {
            rarity = crate::content::cards::CardRarity::Rare;
        } else if rarity == crate::content::cards::CardRarity::Common {
            rarity = crate::content::cards::CardRarity::Uncommon;
        }
        let mut card_id = run_state.random_colorless_card_from_pool(rarity);
        while selected_ids.contains(&card_id) {
            card_id = run_state.random_colorless_card_from_pool(rarity);
        }
        selected_ids.push(card_id);
        cards.push(crate::rewards::state::RewardCard::new(
            card_id,
            run_state.preview_obtain_card_upgrades(card_id, 0),
        ));
    }
    cards
}

#[cfg(test)]
mod tests {
    use super::{
        encode_drawback, encode_reward, generate_neow_class_cards, generate_neow_colorless_cards,
        handle_choice, neow_drawback_effects, neow_reward_effects, NeowDrawback, NeowEligibilityV1,
        NeowRewardType, RICH_ELIGIBILITY_STATE,
    };
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::engine::run_loop::tick_run;
    use crate::runtime::combat::CombatCard;
    use crate::runtime::rng::StsRng;
    use crate::state::core::{ClientInput, EngineState, RunPendingChoiceReason};
    use crate::state::events::{EventCardKind, EventEffect, EventId, EventRelicKind, EventState};
    use crate::state::run::{RunState, RunStateCheckpointV1};
    use crate::state::selection::{
        DomainEvent, DomainEventSource, SelectionReason, SelectionResolution, SelectionScope,
        SelectionTargetRef,
    };

    fn deck_card(id: CardId, uuid: u32, upgrades: u8) -> CombatCard {
        let mut card = CombatCard::for_scenario(id, uuid);
        card.upgrades = upgrades;
        card
    }

    fn neow_run_with_reward(reward: NeowRewardType, deck: Vec<CombatCard>) -> RunState {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.master_deck = deck;
        run_state.neow_rng = Some(StsRng::new(run_state.seed));
        run_state.event_state = Some(EventState {
            id: EventId::Neow,
            current_screen: 1,
            internal_state: RICH_ELIGIBILITY_STATE,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![
                1,
                encode_reward(reward),
                encode_drawback(NeowDrawback::None),
            ],
        });
        run_state
    }

    fn choose_neow_reward(run_state: &mut RunState) -> EngineState {
        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, run_state, 0);
        engine_state
    }

    #[test]
    fn explicit_run_start_eligibility_owns_mini_or_rich_surface() {
        let mut mini =
            RunState::new_with_neow_eligibility(3, 20, false, "Ironclad", NeowEligibilityV1::Mini);
        let mut rich =
            RunState::new_with_neow_eligibility(3, 20, false, "Ironclad", NeowEligibilityV1::Rich);

        assert!(mini.neow_rng.is_none());
        assert!(rich.neow_rng.is_none());
        assert!(mini.event_state.as_ref().unwrap().extra_data.is_empty());
        assert!(rich.event_state.as_ref().unwrap().extra_data.is_empty());

        let mut mini_engine = EngineState::EventRoom;
        let mut rich_engine = EngineState::EventRoom;
        handle_choice(&mut mini_engine, &mut mini, 0);
        handle_choice(&mut rich_engine, &mut rich, 0);

        assert_eq!(mini.event_state.as_ref().unwrap().extra_data[0], 2);
        assert!(mini.neow_rng.is_none());
        assert_eq!(rich.event_state.as_ref().unwrap().extra_data[0], 4);
        assert_eq!(rich.neow_rng.as_ref().unwrap().counter, 5);
    }

    #[test]
    fn neow_rng_absence_and_activation_survive_checkpoint_round_trip() {
        let mut run_state =
            RunState::new_with_neow_eligibility(9, 20, false, "Ironclad", NeowEligibilityV1::Rich);

        let intro = RunStateCheckpointV1::from_run_state(&run_state);
        let intro_bytes = serde_json::to_vec(&intro).unwrap();
        let intro: RunStateCheckpointV1 = serde_json::from_slice(&intro_bytes).unwrap();
        assert!(intro.into_run_state().unwrap().neow_rng.is_none());

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut run_state, 0);
        let reward = RunStateCheckpointV1::from_run_state(&run_state);
        let reward_bytes = serde_json::to_vec(&reward).unwrap();
        let reward: RunStateCheckpointV1 = serde_json::from_slice(&reward_bytes).unwrap();
        assert_eq!(
            reward.into_run_state().unwrap().neow_rng.unwrap().counter,
            5
        );
    }

    #[test]
    fn reward_semantics_preserve_relic_pool_boundaries() {
        let run_state = RunState::new(1, 0, true, "Ironclad");

        assert!(
            neow_reward_effects(&run_state, NeowRewardType::RandomCommonRelic).contains(
                &EventEffect::ObtainRelic {
                    count: 1,
                    kind: EventRelicKind::RandomCommonRelic,
                }
            )
        );
        assert!(
            neow_reward_effects(&run_state, NeowRewardType::OneRareRelic).contains(
                &EventEffect::ObtainRelic {
                    count: 1,
                    kind: EventRelicKind::RandomRareRelic,
                }
            )
        );

        let boss_effects = neow_reward_effects(&run_state, NeowRewardType::BossRelic);
        assert!(boss_effects.contains(&EventEffect::LoseStarterRelic {
            specific: Some(RelicId::BurningBlood),
        }));
        assert!(boss_effects.contains(&EventEffect::ObtainRelic {
            count: 1,
            kind: EventRelicKind::RandomBossRelic,
        }));
    }

    #[test]
    fn reward_semantics_preserve_card_pool_boundaries() {
        let run_state = RunState::new(1, 0, true, "Ironclad");

        assert!(
            neow_reward_effects(&run_state, NeowRewardType::OneRandomRareCard).contains(
                &EventEffect::ObtainCard {
                    count: 1,
                    kind: EventCardKind::RandomClassRare,
                }
            )
        );
        assert!(
            neow_reward_effects(&run_state, NeowRewardType::ThreeCards).contains(
                &EventEffect::OfferCards {
                    count: 3,
                    kind: EventCardKind::RandomClassCommonOrUncommon,
                }
            )
        );
        assert!(
            neow_reward_effects(&run_state, NeowRewardType::ThreeRareCards).contains(
                &EventEffect::OfferCards {
                    count: 3,
                    kind: EventCardKind::RandomClassRare,
                }
            )
        );
        assert!(
            neow_reward_effects(&run_state, NeowRewardType::RandomColorless).contains(
                &EventEffect::OfferCards {
                    count: 3,
                    kind: EventCardKind::RandomColorlessUncommon,
                }
            )
        );
        assert!(
            neow_reward_effects(&run_state, NeowRewardType::RandomColorless2).contains(
                &EventEffect::OfferCards {
                    count: 3,
                    kind: EventCardKind::RandomColorlessRare,
                }
            )
        );
    }

    #[test]
    fn curse_drawback_semantics_name_the_random_ordinary_curse_pool() {
        let run_state = RunState::new(1, 20, true, "Ironclad");

        assert_eq!(
            neow_drawback_effects(&run_state, NeowDrawback::Curse),
            vec![EventEffect::ObtainCurse {
                count: 1,
                kind: EventCardKind::RandomCurse,
            }]
        );
    }

    #[test]
    fn curse_drawback_consumes_card_rng_and_obtains_that_pool_result() {
        let mut run_state = neow_run_with_reward(NeowRewardType::HundredGold, Vec::new());
        run_state
            .event_state
            .as_mut()
            .expect("Neow event")
            .extra_data[2] = encode_drawback(NeowDrawback::Curse);
        let mut expected_rng = run_state.rng_pool.card_rng.clone();
        let pool = crate::content::cards::get_curse_pool();
        let expected = pool[expected_rng.random_range(0, pool.len() as i32 - 1) as usize];

        choose_neow_reward(&mut run_state);

        assert_eq!(run_state.rng_pool.card_rng, expected_rng);
        assert_eq!(run_state.master_deck.len(), 1);
        assert_eq!(run_state.master_deck[0].id, expected);
        assert!(pool.contains(&run_state.master_deck[0].id));
    }

    #[test]
    fn relic_rewards_use_java_front_pool_path() {
        let mut common = neow_run_with_reward(NeowRewardType::RandomCommonRelic, Vec::new());
        common.common_relic_pool = vec![RelicId::BloodVial, RelicId::Anchor];
        choose_neow_reward(&mut common);
        assert!(
            common
                .relics
                .iter()
                .any(|relic| relic.id == RelicId::BloodVial),
            "Java Neow RANDOM_COMMON_RELIC calls returnRandomRelic(COMMON), which removes index 0"
        );
        assert!(!common
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::Anchor));

        let mut rare = neow_run_with_reward(NeowRewardType::OneRareRelic, Vec::new());
        rare.rare_relic_pool = vec![RelicId::Mango, RelicId::OldCoin];
        choose_neow_reward(&mut rare);
        assert!(
            rare.relics.iter().any(|relic| relic.id == RelicId::Mango),
            "Java Neow ONE_RARE_RELIC calls returnRandomRelic(RARE), which removes index 0"
        );
        assert!(!rare.relics.iter().any(|relic| relic.id == RelicId::OldCoin));

        let mut boss = neow_run_with_reward(NeowRewardType::BossRelic, Vec::new());
        boss.boss_relic_pool = vec![RelicId::CoffeeDripper, RelicId::SneckoEye];
        choose_neow_reward(&mut boss);
        assert!(
            boss.relics
                .iter()
                .any(|relic| relic.id == RelicId::CoffeeDripper),
            "Java Neow BOSS_RELIC calls returnRandomRelic(BOSS), which removes index 0"
        );
        assert!(!boss
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::SneckoEye));
    }

    #[test]
    fn remove_selection_uses_java_purgeable_cards_including_bottled() {
        let mut run_state = neow_run_with_reward(
            NeowRewardType::RemoveCard,
            vec![
                deck_card(CardId::Strike, 11, 0),
                deck_card(CardId::Defend, 12, 0),
                deck_card(CardId::AscendersBane, 13, 0),
            ],
        );
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(11).to_runtime_i32_payload();
        run_state.relics.push(bottle);

        let engine_state = choose_neow_reward(&mut run_state);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Neow remove reward should open deck purge selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::Purge);
        let request = choice.selection_request(&run_state);
        assert_eq!(request.reason, SelectionReason::Purge);
        assert_eq!(
            request.targets,
            vec![
                SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(11)
                ),
                SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(12)
                )
            ],
            "Java Neow opens masterDeck.getPurgeableCards(), so bottled cards remain eligible"
        );
    }

    #[test]
    fn transform_selection_uses_java_purgeable_cards_including_bottled() {
        let mut run_state = neow_run_with_reward(
            NeowRewardType::TransformCard,
            vec![
                deck_card(CardId::Strike, 11, 0),
                deck_card(CardId::Defend, 12, 0),
                deck_card(CardId::Necronomicurse, 13, 0),
            ],
        );
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(11).to_runtime_i32_payload();
        run_state.relics.push(bottle);

        let engine_state = choose_neow_reward(&mut run_state);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Neow transform reward should open deck transform selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::Transform);
        let request = choice.selection_request(&run_state);
        assert_eq!(request.reason, SelectionReason::Transform);
        assert_eq!(
            request.targets,
            vec![
                SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(11)
                ),
                SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(12)
                )
            ],
            "Java Neow transform also uses masterDeck.getPurgeableCards()"
        );
    }

    #[test]
    fn upgrade_selection_uses_java_upgradable_cards() {
        let mut run_state = neow_run_with_reward(
            NeowRewardType::UpgradeCard,
            vec![
                deck_card(CardId::Strike, 11, 1),
                deck_card(CardId::Defend, 12, 0),
                deck_card(CardId::Injury, 13, 0),
            ],
        );

        let engine_state = choose_neow_reward(&mut run_state);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Neow upgrade reward should open deck upgrade selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::Upgrade);
        let request = choice.selection_request(&run_state);
        assert_eq!(request.reason, SelectionReason::Upgrade);
        assert_eq!(
            request.targets,
            vec![SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(12)
            )],
            "Java Neow upgrade opens masterDeck.getUpgradableCards()"
        );
    }

    #[test]
    fn remove_two_selection_removes_selected_cards_with_event_source() {
        let mut run_state = neow_run_with_reward(
            NeowRewardType::RemoveTwo,
            vec![
                deck_card(CardId::Strike, 11, 0),
                deck_card(CardId::Defend, 12, 0),
                deck_card(CardId::Bash, 13, 0),
            ],
        );
        let mut engine_state = choose_neow_reward(&mut run_state);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![
                    SelectionTargetRef::CardLineage(
                        crate::runtime::combat::CardLineageId::constructed_scenario(11)
                    ),
                    SelectionTargetRef::CardLineage(
                        crate::runtime::combat::CardLineageId::constructed_scenario(12)
                    ),
                ],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(EventId::Neow),
            } if card.id == CardId::Strike && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(11)
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(EventId::Neow),
            } if card.id == CardId::Defend && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(12)
        )));
    }

    #[test]
    fn selected_upgrade_uses_event_source() {
        let mut run_state = neow_run_with_reward(
            NeowRewardType::UpgradeCard,
            vec![deck_card(CardId::Defend, 12, 0)],
        );
        let mut engine_state = choose_neow_reward(&mut run_state);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(12)
                )],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(run_state.master_deck[0].upgrades, 1);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardUpgraded {
                after,
                source: DomainEventSource::Event(EventId::Neow),
                ..
            } if after.id == CardId::Defend && after.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(12) && after.upgrades == 1
        )));
    }

    #[test]
    fn transform_two_selection_transforms_selected_cards_with_event_source() {
        let mut run_state = neow_run_with_reward(
            NeowRewardType::TransformTwoCards,
            vec![
                deck_card(CardId::Strike, 11, 0),
                deck_card(CardId::Defend, 12, 0),
                deck_card(CardId::Bash, 13, 0),
            ],
        );
        let mut engine_state = choose_neow_reward(&mut run_state);
        let misc_before = run_state.rng_pool.misc_rng.counter;
        let neow_before = run_state.neow_rng.as_ref().unwrap().counter;

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![
                    SelectionTargetRef::CardLineage(
                        crate::runtime::combat::CardLineageId::constructed_scenario(11)
                    ),
                    SelectionTargetRef::CardLineage(
                        crate::runtime::combat::CardLineageId::constructed_scenario(12)
                    ),
                ],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(
            run_state.rng_pool.misc_rng.counter, misc_before,
            "Java Neow transform uses NeowEvent.rng, not miscRng"
        );
        assert_eq!(
            run_state.neow_rng.as_ref().unwrap().counter,
            neow_before + 2,
            "two selected Neow transforms consume two NeowEvent.rng card rolls"
        );
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardTransformed {
                before,
                source: DomainEventSource::Event(EventId::Neow),
                ..
            } if before.id == CardId::Strike && before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(11)
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::CardTransformed {
                before,
                source: DomainEventSource::Event(EventId::Neow),
                ..
            } if before.id == CardId::Defend && before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(12)
        )));
    }

    #[test]
    fn transform_two_removes_both_selected_cards_before_obtaining_replacements() {
        let mut run_state = neow_run_with_reward(
            NeowRewardType::TransformTwoCards,
            vec![
                deck_card(CardId::Parasite, 11, 0),
                deck_card(CardId::Parasite, 12, 0),
                deck_card(CardId::Strike, 13, 0),
            ],
        );
        let mut engine_state = choose_neow_reward(&mut run_state);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![
                    SelectionTargetRef::CardLineage(
                        crate::runtime::combat::CardLineageId::constructed_scenario(11)
                    ),
                    SelectionTargetRef::CardLineage(
                        crate::runtime::combat::CardLineageId::constructed_scenario(12)
                    ),
                ],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(run_state.max_hp, 74);
        assert_eq!(run_state.current_hp, 74);

        let events = run_state.take_emitted_events();
        let first_transform_pos = events
            .iter()
            .position(|event| matches!(event, DomainEvent::CardTransformed { .. }))
            .expect("Neow transform two should obtain transformed replacements");
        let parasite_loss_positions = events
            .iter()
            .enumerate()
            .filter_map(|(idx, event)| match event {
                DomainEvent::MaxHpChanged {
                    delta: -3,
                    source: DomainEventSource::Event(EventId::Neow),
                    ..
                } => Some(idx),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            parasite_loss_positions.len(),
            2,
            "both selected Parasites should run their Java onRemoveFromMasterDeck hooks"
        );
        assert!(
            parasite_loss_positions
                .iter()
                .all(|idx| *idx < first_transform_pos),
            "Java Neow TRANSFORM_TWO_CARDS removes both selected cards before creating ShowCardAndObtainEffect replacements"
        );
    }

    #[test]
    fn proceed_creates_java_neow_rng_and_consumes_choice_generation_calls() {
        let mut run_state = RunState::new(7, 0, true, "Ironclad");
        assert!(run_state.neow_rng.is_none());

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(
            run_state.neow_rng.as_ref().unwrap().counter,
            5,
            "standard Neow blessing also consumes rng.random(0, 0) while constructing the singleton Boss Relic category"
        );
    }

    #[test]
    fn one_random_rare_card_uses_neow_rng_not_card_rng() {
        let mut run_state = neow_run_with_reward(NeowRewardType::OneRandomRareCard, Vec::new());
        let card_before = run_state.rng_pool.card_rng.counter;
        let neow_before = run_state.neow_rng.as_ref().unwrap().counter;
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.rng_pool.card_rng.counter, card_before);
        assert_eq!(
            run_state.neow_rng.as_ref().unwrap().counter,
            neow_before + 1
        );
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                source: DomainEventSource::Event(EventId::Neow),
                ..
            }
        )));
    }

    #[test]
    fn three_small_potions_open_reward_screen_with_flat_potion_helper_rng() {
        let mut run_state = neow_run_with_reward(NeowRewardType::ThreeSmallPotions, Vec::new());
        let starting_potions = run_state.potions.clone();
        let potion_rng_before = run_state.rng_pool.potion_rng.counter;

        let engine_state = choose_neow_reward(&mut run_state);

        assert_eq!(
            run_state.rng_pool.potion_rng.counter,
            potion_rng_before + 3,
            "Java Neow uses PotionHelper.getRandomPotion(), one flat potionRng index per potion reward"
        );
        assert_eq!(
            run_state.potions, starting_potions,
            "Java Neow opens potion rewards instead of directly filling potion slots"
        );
        let EngineState::RewardScreen(rewards) = engine_state else {
            panic!("Neow three potion reward should open reward screen");
        };
        assert_eq!(rewards.items.len(), 3);
        assert!(rewards
            .items
            .iter()
            .all(|item| matches!(item, crate::rewards::state::RewardItem::Potion { .. })));
    }

    #[test]
    fn normal_class_card_reward_uses_neow_rng_and_never_rolls_rare() {
        let mut run_state = neow_run_with_reward(NeowRewardType::ThreeCards, Vec::new());
        let card_before = run_state.rng_pool.card_rng.counter;
        let neow_before = run_state.neow_rng.as_ref().unwrap().counter;

        let cards = generate_neow_class_cards(&mut run_state, false);

        assert_eq!(run_state.rng_pool.card_rng.counter, card_before);
        assert!(run_state.neow_rng.as_ref().unwrap().counter >= neow_before + 6);
        assert_eq!(cards.len(), 3);
        assert!(cards.iter().all(|card| {
            let rarity = crate::content::cards::get_card_definition(card.id).rarity;
            rarity == crate::content::cards::CardRarity::Common
                || rarity == crate::content::cards::CardRarity::Uncommon
        }));
    }

    #[test]
    fn normal_colorless_reward_is_uncommon_only_like_java() {
        let mut run_state = neow_run_with_reward(NeowRewardType::RandomColorless, Vec::new());
        let card_before = run_state.rng_pool.card_rng.counter;
        let neow_before = run_state.neow_rng.as_ref().unwrap().counter;

        let cards = generate_neow_colorless_cards(&mut run_state, false);

        assert!(run_state.rng_pool.card_rng.counter >= card_before + 3);
        assert_eq!(
            run_state.neow_rng.as_ref().unwrap().counter,
            neow_before + 3
        );
        assert_eq!(cards.len(), 3);
        assert!(cards.iter().all(|card| {
            crate::content::cards::get_card_definition(card.id).rarity
                == crate::content::cards::CardRarity::Uncommon
        }));
    }

    #[test]
    fn card_rewards_open_direct_choices_and_close_to_neow() {
        for reward in [
            NeowRewardType::ThreeCards,
            NeowRewardType::ThreeRareCards,
            NeowRewardType::RandomColorless,
            NeowRewardType::RandomColorless2,
        ] {
            for skip in [false, true] {
                let mut run_state = neow_run_with_reward(reward, Vec::new());
                let mut state = choose_neow_reward(&mut run_state);
                let EngineState::RewardScreen(rewards) = &state else {
                    panic!("expected direct choice")
                };
                assert_eq!(
                    rewards.screen_context,
                    crate::state::rewards::RewardScreenContext::NeowCardChoice
                );
                let offers = rewards.pending_card_choice.as_ref().unwrap();
                assert_eq!(offers.len(), 3);
                let picked = offers[0].clone();
                let rng_after_offers = run_state.neow_rng.clone();
                tick_run(
                    &mut state,
                    &mut run_state,
                    &mut None,
                    Some(if skip {
                        ClientInput::Cancel
                    } else {
                        ClientInput::SelectCard(0)
                    }),
                );
                assert_eq!(state, EngineState::EventRoom);
                assert_eq!(run_state.master_deck.len(), usize::from(!skip));
                if !skip {
                    assert_eq!(run_state.master_deck[0].id, picked.id);
                }
                assert_eq!(run_state.neow_rng, rng_after_offers);
            }
        }
    }
}
