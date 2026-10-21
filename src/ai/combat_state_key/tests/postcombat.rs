use super::*;

#[test]
fn stable_astrolabe_selection_preserves_click_order() {
    let run = crate::state::run::RunState::new(1, 20, true, "Ironclad");
    let mut a = crate::state::core::RunPendingChoiceState {
        selected: vec![run.master_deck[1].lineage, run.master_deck[2].lineage],
        min_choices: 3,
        max_choices: 3,
        reason: crate::state::core::RunPendingChoiceReason::TransformUpgraded,
        source: crate::state::selection::DomainEventSource::Relic(
            crate::content::relics::RelicId::Astrolabe,
        ),
        return_state: Box::new(EngineState::MapNavigation),
    };
    let before = stable_outcome_key(
        &EngineState::RunPendingChoice(a.clone()),
        &blank_test_combat(),
    );
    a.selected.reverse();
    let after = stable_outcome_key(&EngineState::RunPendingChoice(a), &blank_test_combat());
    assert_ne!(before, after);
}

#[test]
fn stable_run_pending_choice_keeps_return_state_payloads_distinct() {
    let baseline = blank_test_combat();
    let reward_a = crate::state::rewards::RewardState {
        screen_context: crate::state::rewards::RewardScreenContext::Standard,
        items: vec![crate::state::rewards::RewardItem::Gold { amount: 10 }],
        pending_card_choice: None,
        pending_card_reward_index: None,
        skippable: true,
    };
    let reward_b = crate::state::rewards::RewardState {
        screen_context: crate::state::rewards::RewardScreenContext::Standard,
        items: vec![crate::state::rewards::RewardItem::Gold { amount: 20 }],
        pending_card_choice: None,
        pending_card_reward_index: None,
        skippable: true,
    };

    let a = stable_outcome_key(
        &EngineState::RunPendingChoice(crate::state::core::RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 1,
            max_choices: 1,
            reason: crate::state::core::RunPendingChoiceReason::Purge,
            source: crate::state::selection::DomainEventSource::Selection(
                crate::state::core::RunPendingChoiceReason::Purge.into(),
            ),
            return_state: Box::new(EngineState::RewardScreen(reward_a)),
        }),
        &baseline,
    );
    let b = stable_outcome_key(
        &EngineState::RunPendingChoice(crate::state::core::RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 1,
            max_choices: 1,
            reason: crate::state::core::RunPendingChoiceReason::Purge,
            source: crate::state::selection::DomainEventSource::Selection(
                crate::state::core::RunPendingChoiceReason::Purge.into(),
            ),
            return_state: Box::new(EngineState::RewardScreen(reward_b)),
        }),
        &baseline,
    );

    assert_ne!(a, b);
}

#[test]
fn stable_postcombat_keys_normalize_display_only_order() {
    let baseline = blank_test_combat();

    let reward_a = crate::state::rewards::RewardState {
        screen_context: crate::state::rewards::RewardScreenContext::Standard,
        items: vec![
            crate::state::rewards::RewardItem::Gold { amount: 10 },
            crate::state::rewards::RewardItem::EmeraldKey,
        ],
        pending_card_choice: Some(vec![
            crate::state::rewards::RewardCard::new(CardId::Strike, 0),
            crate::state::rewards::RewardCard::new(CardId::Defend, 0),
        ]),
        pending_card_reward_index: Some(0),
        skippable: true,
    };
    let reward_b = crate::state::rewards::RewardState {
        screen_context: crate::state::rewards::RewardScreenContext::Standard,
        items: vec![
            crate::state::rewards::RewardItem::EmeraldKey,
            crate::state::rewards::RewardItem::Gold { amount: 10 },
        ],
        pending_card_choice: Some(vec![
            crate::state::rewards::RewardCard::new(CardId::Defend, 0),
            crate::state::rewards::RewardCard::new(CardId::Strike, 0),
        ]),
        pending_card_reward_index: Some(1),
        skippable: true,
    };
    assert_eq!(
        stable_outcome_key(&EngineState::RewardScreen(reward_a), &baseline),
        stable_outcome_key(&EngineState::RewardScreen(reward_b), &baseline),
    );

    let shop_a = crate::state::shop::ShopState {
        cards: vec![
            crate::state::shop::ShopCard {
                card_id: CardId::Strike,
                upgrades: 0,
                price: 50,
                can_buy: true,
                blocked_reason: None,
            },
            crate::state::shop::ShopCard {
                card_id: CardId::Defend,
                upgrades: 0,
                price: 60,
                can_buy: true,
                blocked_reason: None,
            },
        ],
        relics: vec![crate::state::shop::ShopRelic {
            relic_id: crate::content::relics::RelicId::BurningBlood,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        }],
        potions: vec![crate::state::shop::ShopPotion {
            potion_id: PotionId::SteroidPotion,
            price: 55,
            can_buy: true,
            blocked_reason: None,
        }],
        purge_cost: 75,
        purge_available: true,
        pending_reward_overlay: None,
    };
    let shop_b = crate::state::shop::ShopState {
        cards: vec![
            crate::state::shop::ShopCard {
                card_id: CardId::Defend,
                upgrades: 0,
                price: 60,
                can_buy: true,
                blocked_reason: None,
            },
            crate::state::shop::ShopCard {
                card_id: CardId::Strike,
                upgrades: 0,
                price: 50,
                can_buy: true,
                blocked_reason: None,
            },
        ],
        relics: vec![crate::state::shop::ShopRelic {
            relic_id: crate::content::relics::RelicId::BurningBlood,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        }],
        potions: vec![crate::state::shop::ShopPotion {
            potion_id: PotionId::SteroidPotion,
            price: 55,
            can_buy: true,
            blocked_reason: None,
        }],
        purge_cost: 75,
        purge_available: true,
        pending_reward_overlay: None,
    };
    assert_eq!(
        stable_outcome_key(&EngineState::Shop(shop_a), &baseline),
        stable_outcome_key(&EngineState::Shop(shop_b), &baseline),
    );

    let boss_a = crate::state::rewards::BossRelicChoiceState::new(vec![
        crate::content::relics::RelicId::BlackBlood,
        crate::content::relics::RelicId::RunicDome,
    ]);
    let boss_b = crate::state::rewards::BossRelicChoiceState::new(vec![
        crate::content::relics::RelicId::RunicDome,
        crate::content::relics::RelicId::BlackBlood,
    ]);
    assert_eq!(
        stable_outcome_key(&EngineState::BossRelicSelect(boss_a), &baseline),
        stable_outcome_key(&EngineState::BossRelicSelect(boss_b), &baseline),
    );

    let run_a = crate::state::core::RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: 1,
        max_choices: 1,
        reason: crate::state::core::RunPendingChoiceReason::Purge,
        source: crate::state::selection::DomainEventSource::Selection(
            crate::state::core::RunPendingChoiceReason::Purge.into(),
        ),
        return_state: Box::new(EngineState::RewardScreen(
            crate::state::rewards::RewardState {
                screen_context: crate::state::rewards::RewardScreenContext::Standard,
                items: vec![
                    crate::state::rewards::RewardItem::Gold { amount: 10 },
                    crate::state::rewards::RewardItem::EmeraldKey,
                ],
                pending_card_choice: Some(vec![
                    crate::state::rewards::RewardCard::new(CardId::Strike, 0),
                    crate::state::rewards::RewardCard::new(CardId::Defend, 0),
                ]),
                pending_card_reward_index: Some(0),
                skippable: true,
            },
        )),
    };
    let run_b = crate::state::core::RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: 1,
        max_choices: 1,
        reason: crate::state::core::RunPendingChoiceReason::Purge,
        source: crate::state::selection::DomainEventSource::Selection(
            crate::state::core::RunPendingChoiceReason::Purge.into(),
        ),
        return_state: Box::new(EngineState::RewardScreen(
            crate::state::rewards::RewardState {
                screen_context: crate::state::rewards::RewardScreenContext::Standard,
                items: vec![
                    crate::state::rewards::RewardItem::EmeraldKey,
                    crate::state::rewards::RewardItem::Gold { amount: 10 },
                ],
                pending_card_choice: Some(vec![
                    crate::state::rewards::RewardCard::new(CardId::Defend, 0),
                    crate::state::rewards::RewardCard::new(CardId::Strike, 0),
                ]),
                pending_card_reward_index: Some(1),
                skippable: true,
            },
        )),
    };
    assert_eq!(
        stable_outcome_key(&EngineState::RunPendingChoice(run_a), &baseline),
        stable_outcome_key(&EngineState::RunPendingChoice(run_b), &baseline),
    );
}
