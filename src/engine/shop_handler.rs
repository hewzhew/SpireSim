use crate::content::cards::{
    get_card_definition, CardId, CardRarity, COLORLESS_RARE_POOL, COLORLESS_UNCOMMON_POOL,
};
use crate::content::potions::{self, PotionId};
use crate::content::relics::{RelicId, RelicTier};
use crate::state::core::{ClientInput, EngineState};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;
use crate::state::shop::merchant::{
    random_shop_colored_card_of_type_for_courier_restock, random_shop_colorless_card,
};
use crate::state::shop::{ShopCard, ShopPotion, ShopRelic, ShopState};

fn has_relic(run_state: &RunState, relic_id: RelicId) -> bool {
    run_state.relics.iter().any(|relic| relic.id == relic_id)
}

fn current_shop_price_multiplier(run_state: &RunState) -> f32 {
    let mut multiplier = 1.0;
    if has_relic(run_state, RelicId::Courier) {
        multiplier *= 0.8;
    }
    if has_relic(run_state, RelicId::MembershipCard) {
        multiplier *= 0.5;
    }
    multiplier
}

fn apply_restock_relic_or_potion_discounts(run_state: &RunState, mut price: i32) -> i32 {
    if has_relic(run_state, RelicId::Courier) {
        price = (price as f32 * 0.8).round() as i32;
    }
    if has_relic(run_state, RelicId::MembershipCard) {
        price = (price as f32 * 0.5).round() as i32;
    }
    price
}

fn base_purge_cost(run_state: &RunState) -> i32 {
    75 + run_state.shop_purge_count * 25
}

fn apply_shop_discount(
    shop: &mut ShopState,
    multiplier: f32,
    affect_purge: bool,
    smiling_mask_active: bool,
    purge_base_cost: i32,
) {
    for card in &mut shop.cards {
        card.price = (card.price as f32 * multiplier).round() as i32;
    }
    for relic in &mut shop.relics {
        relic.price = (relic.price as f32 * multiplier).round() as i32;
    }
    for potion in &mut shop.potions {
        potion.price = (potion.price as f32 * multiplier).round() as i32;
    }
    if smiling_mask_active {
        shop.purge_cost = 50;
    } else if affect_purge {
        shop.purge_cost = (purge_base_cost as f32 * multiplier).round() as i32;
    }
}

fn base_relic_price_for_tier(tier: RelicTier) -> i32 {
    match tier {
        RelicTier::Common => 150,
        RelicTier::Uncommon => 250,
        RelicTier::Rare => 300,
        RelicTier::Shop => 150,
        _ => 150,
    }
}

fn roll_shop_relic_tier(run_state: &mut RunState) -> RelicTier {
    let roll = run_state.rng_pool.merchant_rng.random(99);
    if roll < 48 {
        RelicTier::Common
    } else if roll < 82 {
        RelicTier::Uncommon
    } else {
        RelicTier::Rare
    }
}

fn reprice_relic_price(run_state: &mut RunState, base_price: i32) -> i32 {
    let jittered = (base_price as f32
        * run_state
            .rng_pool
            .merchant_rng
            .random_f32_min_max(0.95, 1.05))
    .round() as i32;
    apply_restock_relic_or_potion_discounts(run_state, jittered)
}

fn reprice_potion_price(run_state: &mut RunState, potion_id: PotionId) -> i32 {
    let base_price = potions::get_potion_price(potion_id) as f32;
    let jittered = (base_price
        * run_state
            .rng_pool
            .merchant_rng
            .random_f32_min_max(0.95, 1.05))
    .round() as i32;
    apply_restock_relic_or_potion_discounts(run_state, jittered)
}

fn reprice_card_price(run_state: &mut RunState, card_id: CardId) -> i32 {
    let def = get_card_definition(card_id);
    let mut price = match def.rarity {
        CardRarity::Common => 50.0,
        CardRarity::Uncommon => 75.0,
        CardRarity::Rare => 150.0,
        _ => 50.0,
    } * run_state.rng_pool.merchant_rng.random_f32_min_max(0.9, 1.1);
    if COLORLESS_UNCOMMON_POOL.contains(&card_id) || COLORLESS_RARE_POOL.contains(&card_id) {
        price *= 1.2;
    }
    price *= current_shop_price_multiplier(run_state);
    price.trunc() as i32
}

fn replace_shop_relic_slot(run_state: &mut RunState, shop: &mut ShopState, idx: usize) {
    let replacement = loop {
        let tier = roll_shop_relic_tier(run_state);
        let relic_id = run_state.random_relic_end_by_tier(tier);
        if matches!(
            relic_id,
            RelicId::OldCoin | RelicId::SmilingMask | RelicId::MawBank | RelicId::Courier
        ) {
            continue;
        }
        break ShopRelic {
            relic_id,
            price: reprice_relic_price(run_state, base_relic_price_for_tier(tier)),
            can_buy: true,
            blocked_reason: None,
        };
    };

    if idx <= shop.relics.len() {
        shop.relics.insert(idx, replacement);
    } else {
        shop.relics.push(replacement);
    }
}

fn replace_shop_potion_slot(run_state: &mut RunState, shop: &mut ShopState, idx: usize) {
    let potion_id = run_state.random_potion();
    let replacement = ShopPotion {
        potion_id,
        price: reprice_potion_price(run_state, potion_id),
        can_buy: true,
        blocked_reason: None,
    };
    if idx <= shop.potions.len() {
        shop.potions.insert(idx, replacement);
    } else {
        shop.potions.push(replacement);
    }
}

fn replace_shop_card_slot(
    run_state: &mut RunState,
    shop: &mut ShopState,
    idx: usize,
    purchased_card_id: CardId,
) {
    let replacement_id = if COLORLESS_UNCOMMON_POOL.contains(&purchased_card_id)
        || COLORLESS_RARE_POOL.contains(&purchased_card_id)
    {
        let rarity = if run_state.rng_pool.merchant_rng.random_boolean_chance(0.3) {
            CardRarity::Rare
        } else {
            CardRarity::Uncommon
        };
        random_shop_colorless_card(&mut run_state.rng_pool, rarity)
    } else {
        let card_type = get_card_definition(purchased_card_id).card_type;
        random_shop_colored_card_of_type_for_courier_restock(
            &mut run_state.rng_pool,
            run_state.player_class,
            run_state.card_blizz_randomizer,
            card_type,
        )
    };
    let replacement = ShopCard {
        card_id: replacement_id,
        upgrades: run_state.preview_obtain_card_upgrades(replacement_id, 0),
        price: reprice_card_price(run_state, replacement_id),
        can_buy: true,
        blocked_reason: None,
    };
    if idx <= shop.cards.len() {
        shop.cards.insert(idx, replacement);
    } else {
        shop.cards.push(replacement);
    }
}

fn preview_shop_cards_after_relic_purchase(run_state: &RunState, shop: &mut ShopState) {
    for card in &mut shop.cards {
        card.upgrades = run_state.preview_obtain_card_upgrades(card.card_id, card.upgrades);
    }
}

pub fn handle(
    run_state: &mut RunState,
    shop: &mut ShopState,
    input: Option<ClientInput>,
) -> Option<EngineState> {
    if let Some(in_val) = input {
        match in_val {
            ClientInput::OpenRewardOverlay => {
                if let Some(reward_state) = shop.pending_reward_overlay.take() {
                    return Some(EngineState::reward_overlay(
                        reward_state,
                        EngineState::Shop(shop.clone()),
                    ));
                }
            }
            ClientInput::BuyCard(idx) => {
                if idx < shop.cards.len()
                    && shop.cards[idx].can_buy
                    && run_state.gold >= shop.cards[idx].price
                {
                    let purchased = shop.cards.remove(idx);
                    run_state.change_gold_with_source(-purchased.price, DomainEventSource::Shop);
                    run_state.add_card_to_deck_with_upgrades_from(
                        purchased.card_id,
                        purchased.upgrades,
                        DomainEventSource::Shop,
                    );
                    if has_relic(run_state, RelicId::Courier) {
                        replace_shop_card_slot(run_state, shop, idx, purchased.card_id);
                    }
                }
            }
            ClientInput::BuyRelic(idx) => {
                if idx < shop.relics.len()
                    && shop.relics[idx].can_buy
                    && run_state.gold >= shop.relics[idx].price
                {
                    let purchased = shop.relics.remove(idx);
                    run_state.change_gold_with_source(-purchased.price, DomainEventSource::Shop);
                    let next_state = run_state.obtain_relic_with_source(
                        purchased.relic_id,
                        EngineState::Shop(shop.clone()),
                        DomainEventSource::Shop,
                    );

                    apply_post_relic_purchase_shop_updates(
                        run_state,
                        shop,
                        purchased.relic_id,
                        idx,
                    );
                    if let Some(next_state) = next_state {
                        return Some(with_updated_shop_return_state(next_state, shop.clone()));
                    }
                }
            }
            ClientInput::BuyPotion(idx) => {
                if idx < shop.potions.len()
                    && shop.potions[idx].can_buy
                    && run_state.gold >= shop.potions[idx].price
                {
                    if has_relic(run_state, RelicId::Sozu) {
                        return None;
                    } else if let Some(empty_slot) = run_state.find_empty_potion_slot() {
                        let purchased = shop.potions.remove(idx);
                        run_state
                            .change_gold_with_source(-purchased.price, DomainEventSource::Shop);
                        run_state.obtain_potion_with_source(
                            crate::content::potions::Potion::new(
                                purchased.potion_id,
                                empty_slot as u32,
                            ),
                            DomainEventSource::Shop,
                        );
                        if has_relic(run_state, RelicId::Courier) {
                            replace_shop_potion_slot(run_state, shop, idx);
                        }
                    }
                }
            }
            ClientInput::PurgeCard(idx) => {
                if shop.purge_available
                    && run_state.gold >= shop.purge_cost
                    && idx < run_state.master_deck.len()
                    && crate::state::core::master_deck_card_is_purgeable(
                        &run_state.master_deck[idx],
                    )
                    && !crate::state::core::master_deck_card_is_bottled(
                        &run_state.master_deck[idx],
                        &run_state.relics,
                    )
                {
                    let lineage = run_state.master_deck[idx].lineage;
                    run_state.change_gold_with_source(-shop.purge_cost, DomainEventSource::Shop);
                    shop.purge_available = false;
                    run_state.remove_card_from_deck_with_source(lineage, DomainEventSource::Shop);
                    run_state.shop_purge_count += 1;
                    shop.purge_cost = if has_relic(run_state, RelicId::SmilingMask) {
                        50
                    } else {
                        (base_purge_cost(run_state) as f32
                            * current_shop_price_multiplier(run_state))
                        .round() as i32
                    };
                }
            }
            ClientInput::Proceed | ClientInput::Cancel => {
                return Some(EngineState::MapNavigation);
            }
            _ => {}
        }
    }
    None
}

fn apply_post_relic_purchase_shop_updates(
    run_state: &mut RunState,
    shop: &mut ShopState,
    purchased_relic_id: RelicId,
    purchased_slot: usize,
) {
    if purchased_relic_id == RelicId::MembershipCard {
        apply_shop_discount(
            shop,
            0.5,
            true,
            has_relic(run_state, RelicId::SmilingMask),
            base_purge_cost(run_state),
        );
    }
    if purchased_relic_id == RelicId::SmilingMask {
        shop.purge_cost = 50;
    }
    if matches!(
        purchased_relic_id,
        RelicId::MoltenEgg | RelicId::ToxicEgg | RelicId::FrozenEgg
    ) {
        preview_shop_cards_after_relic_purchase(run_state, shop);
        if let Some(rewards) = &mut shop.pending_reward_overlay {
            crate::engine::reward_handler::preview_existing_card_rewards(run_state, rewards);
        }
    }
    if purchased_relic_id == RelicId::Courier || has_relic(run_state, RelicId::Courier) {
        replace_shop_relic_slot(run_state, shop, purchased_slot);
    }
}

fn with_updated_shop_return_state(state: EngineState, updated_shop: ShopState) -> EngineState {
    match state {
        EngineState::RewardOverlay {
            reward_state,
            return_state,
        } => {
            let return_state = if matches!(*return_state, EngineState::Shop(_)) {
                EngineState::Shop(updated_shop)
            } else {
                *return_state
            };
            EngineState::reward_overlay(reward_state, return_state)
        }
        EngineState::RunPendingChoice(mut choice)
            if matches!(*choice.return_state, EngineState::Shop(_)) =>
        {
            choice.return_state = Box::new(EngineState::Shop(updated_shop));
            EngineState::RunPendingChoice(choice)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_restock_relic_or_potion_discounts, handle, replace_shop_relic_slot};
    use crate::content::cards::CardId;
    use crate::content::potions::PotionId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::core::{ClientInput, EngineState};
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};
    use crate::state::shop::state::{ShopCard, ShopPotion, ShopRelic, ShopState};

    #[test]
    fn courier_membership_restock_relic_potion_discounts_round_sequentially() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.relics.clear();
        run_state.relics.push(RelicState::new(RelicId::Courier));
        run_state
            .relics
            .push(RelicState::new(RelicId::MembershipCard));

        assert_eq!(
            apply_restock_relic_or_potion_discounts(&run_state, 101),
            41,
            "Java ShopScreen.getNewPrice rounds after Courier, then rounds again after Membership Card"
        );
    }

    #[test]
    fn spending_gold_in_shop_uses_up_maw_bank() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 200;
        run_state.relics.clear();
        run_state.relics.push(RelicState::new(RelicId::MawBank));

        let mut shop = ShopState::new();
        shop.cards.push(ShopCard {
            card_id: CardId::Strike,
            upgrades: 0,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyCard(0)));
        assert!(next.is_none());
        let maw_bank = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::MawBank)
            .expect("MawBank should still be present");
        assert!(maw_bank.used_up);
        assert_eq!(maw_bank.counter, -2);
    }

    #[test]
    fn buying_shop_card_spends_gold_before_fast_obtain_hooks_and_card_obtained() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 200;
        run_state.relics.clear();
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        let mut shop = ShopState::new();
        shop.cards.push(ShopCard {
            card_id: CardId::PommelStrike,
            upgrades: 0,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyCard(0)));
        assert!(next.is_none());

        let events = run_state.take_emitted_events();
        let spend_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::GoldChanged {
                        delta: -50,
                        source: DomainEventSource::Shop,
                        ..
                    }
                )
            })
            .expect("Shop card purchase should spend gold");
        let fish_gold_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::GoldChanged {
                        delta: 9,
                        source: DomainEventSource::Shop,
                        ..
                    }
                )
            })
            .expect("Shop card purchase should run Ceramic Fish obtain hook");
        let obtained_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Shop,
                    } if card.id == CardId::PommelStrike
                )
            })
            .expect("Shop card purchase should obtain the bought card");

        assert!(
            spend_pos < fish_gold_pos && fish_gold_pos < obtained_pos,
            "Java ShopScreen.purchaseCard queues FastCardObtainEffect, then loses gold; the effect later runs onObtainCard before Soul.obtain"
        );
    }

    #[test]
    fn shop_handler_respects_blocked_item_flags_for_direct_inputs() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 500;
        let mut shop = ShopState::new();
        shop.cards.push(ShopCard {
            card_id: CardId::PommelStrike,
            upgrades: 0,
            price: 50,
            can_buy: false,
            blocked_reason: Some("blocked card".to_string()),
        });
        shop.relics.push(ShopRelic {
            relic_id: RelicId::Anchor,
            price: 150,
            can_buy: false,
            blocked_reason: Some("blocked relic".to_string()),
        });
        shop.potions.push(ShopPotion {
            potion_id: PotionId::FirePotion,
            price: 50,
            can_buy: false,
            blocked_reason: Some("blocked potion".to_string()),
        });

        assert!(handle(&mut run_state, &mut shop, Some(ClientInput::BuyCard(0))).is_none());
        assert!(handle(&mut run_state, &mut shop, Some(ClientInput::BuyRelic(0))).is_none());
        assert!(handle(&mut run_state, &mut shop, Some(ClientInput::BuyPotion(0))).is_none());

        assert_eq!(run_state.gold, 500);
        assert_eq!(shop.cards.len(), 1);
        assert_eq!(shop.relics.len(), 1);
        assert_eq!(shop.potions.len(), 1);
        assert!(!run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::PommelStrike));
        assert!(!run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::Anchor));
        assert!(run_state.potions.iter().all(Option::is_none));
    }

    #[test]
    fn buying_shop_curse_still_spends_gold_when_omamori_blocks_fast_obtain() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 200;
        run_state.relics.clear();
        run_state.relics.push(RelicState::new(RelicId::Omamori));
        let mut shop = ShopState::new();
        shop.cards.push(ShopCard {
            card_id: CardId::Regret,
            upgrades: 0,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });

        handle(&mut run_state, &mut shop, Some(ClientInput::BuyCard(0)));

        assert_eq!(
            run_state.gold, 150,
            "Java FastCardObtainEffect can be blocked by Omamori, but ShopScreen.purchaseCard still loses gold after constructing it"
        );
        assert!(!run_state
            .master_deck
            .iter()
            .any(|card| card.id == CardId::Regret));
        let omamori = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("Omamori should remain after blocking shop curse");
        assert_eq!(omamori.counter, 1);
    }

    #[test]
    fn buying_orrery_returns_to_updated_shop_after_overlay_rewards() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 500;
        let mut shop = ShopState::new();
        shop.relics.push(ShopRelic {
            relic_id: RelicId::Orrery,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyRelic(0)))
            .expect("Orrery should open an overlay reward screen");

        let EngineState::RewardOverlay {
            reward_state,
            return_state,
        } = next
        else {
            panic!("expected reward overlay");
        };
        assert_eq!(run_state.gold, 350);
        assert_eq!(shop.relics.len(), 0);
        assert_eq!(
            reward_state
                .items
                .iter()
                .filter(|item| matches!(item, crate::state::rewards::RewardItem::Card { .. }))
                .count(),
            5
        );
        let EngineState::Shop(return_shop) = *return_state else {
            panic!("Orrery overlay should return to shop");
        };
        assert!(
            return_shop.relics.is_empty(),
            "returning to shop must not resurrect the purchased Orrery slot"
        );
    }

    #[test]
    fn buying_cauldron_returns_to_updated_shop_after_overlay_rewards() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 500;
        let mut shop = ShopState::new();
        shop.relics.push(ShopRelic {
            relic_id: RelicId::Cauldron,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyRelic(0)))
            .expect("Cauldron should open an overlay reward screen");

        let EngineState::RewardOverlay {
            reward_state,
            return_state,
        } = next
        else {
            panic!("expected reward overlay");
        };
        assert_eq!(run_state.gold, 350);
        assert_eq!(shop.relics.len(), 0);
        assert_eq!(
            reward_state
                .items
                .iter()
                .filter(|item| matches!(item, crate::state::rewards::RewardItem::Potion { .. }))
                .count(),
            5
        );
        let EngineState::Shop(return_shop) = *return_state else {
            panic!("Cauldron overlay should return to shop");
        };
        assert!(
            return_shop.relics.is_empty(),
            "returning to shop must not resurrect the purchased Cauldron slot"
        );
    }

    #[test]
    fn shop_can_reopen_pending_reward_overlay_once() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        let mut shop = ShopState::new();
        let mut pending = crate::state::rewards::RewardState::new();
        pending.items = vec![crate::state::rewards::RewardItem::Gold { amount: 25 }];
        shop.pending_reward_overlay = Some(pending);

        let next = handle(
            &mut run_state,
            &mut shop,
            Some(ClientInput::OpenRewardOverlay),
        )
        .expect("pending shop reward overlay should reopen");

        let EngineState::RewardOverlay {
            reward_state,
            return_state,
        } = next
        else {
            panic!("expected reward overlay");
        };
        assert_eq!(
            reward_state.items,
            vec![crate::state::rewards::RewardItem::Gold { amount: 25 }]
        );
        let EngineState::Shop(return_shop) = *return_state else {
            panic!("overlay should return to shop");
        };
        assert!(
            return_shop.pending_reward_overlay.is_none(),
            "opening pending rewards must move, not clone, the overlay state"
        );
        assert!(shop.pending_reward_overlay.is_none());
    }

    #[test]
    fn leaving_shop_without_spending_keeps_maw_bank_active() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 200;
        run_state.relics.clear();
        run_state.relics.push(RelicState::new(RelicId::MawBank));

        let mut shop = ShopState::new();
        let next = handle(&mut run_state, &mut shop, Some(ClientInput::Proceed));
        assert_eq!(next, Some(EngineState::MapNavigation));
        let maw_bank = run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::MawBank)
            .expect("MawBank should still be present");
        assert!(!maw_bank.used_up);
        assert_eq!(maw_bank.counter, -1);
    }

    #[test]
    fn shop_purge_uses_java_non_bottled_purgeable_cards_and_shop_source() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 200;
        run_state.master_deck = vec![
            crate::runtime::combat::CombatCard::for_scenario(CardId::Strike, 10),
            crate::runtime::combat::CombatCard::for_scenario(CardId::AscendersBane, 11),
            crate::runtime::combat::CombatCard::for_scenario(CardId::Defend, 12),
        ];
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(12).to_runtime_i32_payload();
        run_state.relics.push(bottle);
        run_state.emitted_events.clear();

        let mut shop = ShopState::new();
        shop.purge_available = true;
        shop.purge_cost = 75;

        assert!(handle(&mut run_state, &mut shop, Some(ClientInput::PurgeCard(1))).is_none());
        assert_eq!(run_state.gold, 200);
        assert_eq!(run_state.master_deck.len(), 3);

        assert!(handle(&mut run_state, &mut shop, Some(ClientInput::PurgeCard(2))).is_none());
        assert_eq!(run_state.gold, 200);
        assert_eq!(run_state.master_deck.len(), 3);

        assert!(handle(&mut run_state, &mut shop, Some(ClientInput::PurgeCard(0))).is_none());
        assert_eq!(run_state.gold, 125);
        assert_eq!(run_state.master_deck.len(), 2);
        assert!(!shop.purge_available);
        assert_eq!(
            shop.purge_cost, 100,
            "ShopScreen.purgeCard refreshes actualPurgeCost even after the service is used"
        );
        assert!(run_state.emitted_events.iter().any(|event| matches!(
            event,
            crate::state::selection::DomainEvent::CardRemoved {
                card,
                source: crate::state::selection::DomainEventSource::Shop,
            } if card.id == CardId::Strike && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(10)
        )));
    }

    #[test]
    fn membership_card_purchase_discounts_remaining_shop_inventory() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 500;
        run_state.shop_purge_count = 1;
        run_state.relics.clear();

        let mut shop = ShopState::new();
        shop.purge_cost = 100;
        shop.cards.push(ShopCard {
            card_id: CardId::Strike,
            upgrades: 0,
            price: 100,
            can_buy: true,
            blocked_reason: None,
        });
        shop.relics.push(ShopRelic {
            relic_id: RelicId::MembershipCard,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });
        shop.relics.push(ShopRelic {
            relic_id: RelicId::Anchor,
            price: 200,
            can_buy: true,
            blocked_reason: None,
        });
        shop.potions.push(ShopPotion {
            potion_id: PotionId::BlockPotion,
            price: 60,
            can_buy: true,
            blocked_reason: None,
        });

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyRelic(0)));
        assert!(next.is_none());
        assert_eq!(shop.cards[0].price, 50);
        assert_eq!(shop.relics[0].price, 100);
        assert_eq!(shop.potions[0].price, 30);
        assert_eq!(shop.purge_cost, 50);
    }

    #[test]
    fn smiling_mask_purchase_sets_purge_cost_to_50() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 500;
        run_state.relics.clear();

        let mut shop = ShopState::new();
        shop.purge_cost = 125;
        shop.relics.push(ShopRelic {
            relic_id: RelicId::SmilingMask,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyRelic(0)));
        assert!(next.is_none());
        assert_eq!(shop.purge_cost, 50);
    }

    #[test]
    fn courier_keeps_relic_slot_filled_after_purchase() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 500;
        run_state.relics.clear();
        run_state.common_relic_pool = vec![RelicId::Anchor];
        run_state.uncommon_relic_pool = vec![RelicId::LetterOpener];
        run_state.rare_relic_pool = vec![RelicId::LizardTail];

        let mut shop = ShopState::new();
        shop.relics.push(ShopRelic {
            relic_id: RelicId::Courier,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyRelic(0)));
        assert!(next.is_none());
        assert_eq!(shop.relics.len(), 1);
        assert_ne!(shop.relics[0].relic_id, RelicId::Courier);
    }

    #[test]
    fn courier_relic_replacement_uses_end_path_and_can_spawn_filter() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.relics.clear();
        run_state.common_relic_pool = vec![RelicId::Anchor, RelicId::BottledFlame];
        run_state.uncommon_relic_pool = vec![RelicId::Anchor, RelicId::BottledFlame];
        run_state.rare_relic_pool = vec![RelicId::Anchor, RelicId::BottledFlame];

        let mut shop = ShopState::new();
        shop.relics.push(ShopRelic {
            relic_id: RelicId::Courier,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });

        replace_shop_relic_slot(&mut run_state, &mut shop, 0);

        assert_eq!(
            shop.relics[0].relic_id,
            RelicId::Anchor,
            "Java shop/end relic replacement draws from the end, but rejects Bottled Flame when canSpawn is false"
        );
    }

    #[test]
    fn courier_keeps_potion_slot_filled_after_purchase() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 500;
        run_state.relics.clear();
        run_state.relics.push(RelicState::new(RelicId::Courier));

        let mut shop = ShopState::new();
        shop.potions.push(ShopPotion {
            potion_id: PotionId::BlockPotion,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyPotion(0)));
        assert!(next.is_none());
        assert_eq!(shop.potions.len(), 1);
        assert!(run_state.potions.iter().any(|p| p.is_some()));
    }

    #[test]
    fn courier_restock_preserves_unpurchased_neighbor_offers() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 1_000;
        run_state.relics.clear();
        run_state.relics.push(RelicState::new(RelicId::Courier));

        let mut card_shop = ShopState::new();
        card_shop.cards = vec![
            ShopCard {
                card_id: CardId::Strike,
                upgrades: 0,
                price: 10,
                can_buy: true,
                blocked_reason: None,
            },
            ShopCard {
                card_id: CardId::Defend,
                upgrades: 0,
                price: 11,
                can_buy: true,
                blocked_reason: None,
            },
        ];
        handle(
            &mut run_state,
            &mut card_shop,
            Some(ClientInput::BuyCard(0)),
        );
        assert_eq!(card_shop.cards.len(), 2);
        assert_eq!(card_shop.cards[1].card_id, CardId::Defend);

        let mut potion_shop = ShopState::new();
        potion_shop.potions = vec![
            ShopPotion {
                potion_id: PotionId::BlockPotion,
                price: 10,
                can_buy: true,
                blocked_reason: None,
            },
            ShopPotion {
                potion_id: PotionId::StrengthPotion,
                price: 11,
                can_buy: true,
                blocked_reason: None,
            },
        ];
        handle(
            &mut run_state,
            &mut potion_shop,
            Some(ClientInput::BuyPotion(0)),
        );
        assert_eq!(potion_shop.potions.len(), 2);
        assert_eq!(potion_shop.potions[1].potion_id, PotionId::StrengthPotion);

        let mut relic_shop = ShopState::new();
        relic_shop.relics = vec![
            ShopRelic {
                relic_id: RelicId::Akabeko,
                price: 10,
                can_buy: true,
                blocked_reason: None,
            },
            ShopRelic {
                relic_id: RelicId::Anchor,
                price: 11,
                can_buy: true,
                blocked_reason: None,
            },
        ];
        handle(
            &mut run_state,
            &mut relic_shop,
            Some(ClientInput::BuyRelic(0)),
        );
        assert_eq!(relic_shop.relics.len(), 2);
        assert_eq!(relic_shop.relics[1].relic_id, RelicId::Anchor);
    }

    #[test]
    fn sozu_shop_potion_purchase_is_blocked_without_spending_or_removing_offer() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 200;
        run_state.relics.push(RelicState::new(RelicId::Sozu));
        let starting_potions = run_state.potions.clone();

        let mut shop = ShopState::new();
        shop.potions.push(ShopPotion {
            potion_id: PotionId::BlockPotion,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyPotion(0)));

        assert!(next.is_none());
        assert_eq!(run_state.gold, 200);
        assert_eq!(run_state.potions, starting_potions);
        assert_eq!(shop.potions.len(), 1);
    }

    #[test]
    fn courier_does_not_refill_sozu_blocked_shop_potion_purchase() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 200;
        run_state.relics.clear();
        run_state.relics.push(RelicState::new(RelicId::Sozu));
        run_state.relics.push(RelicState::new(RelicId::Courier));
        let starting_potions = run_state.potions.clone();

        let mut shop = ShopState::new();
        shop.potions.push(ShopPotion {
            potion_id: PotionId::BlockPotion,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyPotion(0)));

        assert!(next.is_none());
        assert_eq!(run_state.gold, 200);
        assert_eq!(run_state.potions, starting_potions);
        assert_eq!(shop.potions.len(), 1);
        assert_eq!(shop.potions[0].potion_id, PotionId::BlockPotion);
    }

    #[test]
    fn courier_keeps_card_slot_filled_after_purchase() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 500;
        run_state.relics.clear();
        run_state.relics.push(RelicState::new(RelicId::Courier));
        let starting_deck_len = run_state.master_deck.len();

        let mut shop = ShopState::new();
        shop.cards.push(ShopCard {
            card_id: CardId::Strike,
            upgrades: 0,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyCard(0)));
        assert!(next.is_none());
        assert_eq!(run_state.master_deck.len(), starting_deck_len + 1);
        assert_eq!(shop.cards.len(), 1);
    }

    #[test]
    fn shop_card_purchase_preserves_preview_upgrades() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 500;
        run_state.relics.clear();
        run_state.master_deck.clear();

        let mut shop = ShopState::new();
        shop.cards.push(ShopCard {
            card_id: CardId::SearingBlow,
            upgrades: 1,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyCard(0)));
        assert!(next.is_none());
        assert_eq!(run_state.master_deck.len(), 1);
        assert_eq!(run_state.master_deck[0].id, CardId::SearingBlow);
        assert_eq!(
            run_state.master_deck[0].upgrades, 1,
            "shop purchase must carry the visible card upgrade state into master_deck"
        );
    }

    #[test]
    fn buying_egg_relic_previews_shop_cards_and_unclaimed_rewards() {
        use crate::state::rewards::{RewardCard, RewardItem, RewardState};
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.gold = 500;
        run_state.relics.clear();

        let mut shop = ShopState::new();
        shop.cards.push(ShopCard {
            card_id: CardId::Strike,
            upgrades: 0,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });
        shop.cards.push(ShopCard {
            card_id: CardId::Defend,
            upgrades: 0,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });
        shop.relics.push(ShopRelic {
            relic_id: RelicId::MoltenEgg,
            price: 100,
            can_buy: true,
            blocked_reason: None,
        });
        let mut pending_rewards = RewardState::new();
        pending_rewards.items.push(RewardItem::Card {
            cards: vec![RewardCard::new(CardId::TwinStrike, 0)],
        });
        shop.pending_reward_overlay = Some(pending_rewards);

        let next = handle(&mut run_state, &mut shop, Some(ClientInput::BuyRelic(0)));
        assert!(next.is_none());
        assert_eq!(shop.cards[0].upgrades, 1);
        assert_eq!(
            shop.cards[1].upgrades, 0,
            "Molten Egg previews attack cards only; skill cards wait for Toxic Egg"
        );
        assert_eq!(
            shop.pending_reward_overlay.as_ref().unwrap().items,
            vec![RewardItem::Card {
                cards: vec![RewardCard::new(CardId::TwinStrike, 1)],
            }]
        );
    }
}
