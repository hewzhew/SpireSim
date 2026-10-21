use crate::content::cards::get_card_definition;
use crate::content::potions;
use crate::content::relics::{RelicId, RelicTier};
use crate::state::deck::context::DeckContext;
use crate::state::deck::manager::DeckManager;
use crate::state::shop::merchant::generate_cards;
use crate::state::shop::state::{ShopCard, ShopConfig, ShopPotion, ShopRelic, ShopState};

fn shop_deck_context(config: &ShopConfig) -> DeckContext {
    DeckContext {
        has_hoarder_mod: false,
        has_omamori: false,
        omamori_charges: 0,
        has_ceramic_fish: false,
        has_darkstone_periapt: false,
        has_molten_egg: config.has_molten_egg,
        has_toxic_egg: config.has_toxic_egg,
        has_frozen_egg: config.has_frozen_egg,
    }
}

fn preview_shop_upgrades(config: &ShopConfig, card_id: crate::content::cards::CardId) -> u8 {
    DeckManager::preview_obtain_upgrades(&shop_deck_context(config), card_id, 0)
}

/// Equivalent to Java's `ShopScreen.init(...)` + associated methods.
/// Consumes rng sequences and populates the ShopState, determining prices and discounts.
pub fn generate_shop<F>(
    rng_pool: &mut crate::runtime::rng::RngPool,
    config: &ShopConfig,
    mut get_relic: F,
) -> ShopState
where
    F: FnMut(RelicTier) -> RelicId,
{
    let mut shop = ShopState::new();

    // 1. Ask Merchant to generate the raw card slots from cardRng
    let (colored_cards, colorless_cards) =
        generate_cards(rng_pool, config.player_class, config.card_blizz_randomizer);

    // Process Colored Cards Jitter (merchantRng)
    for c in colored_cards {
        let rarity = get_card_definition(c).rarity;
        let base_price = match rarity {
            crate::content::cards::CardRarity::Common => 50.0,
            crate::content::cards::CardRarity::Uncommon => 75.0,
            crate::content::cards::CardRarity::Rare => 150.0,
            _ => 50.0,
        };
        let tmp_price = base_price * rng_pool.merchant_rng.random_f32_min_max(0.9, 1.1);
        let price = tmp_price.trunc() as i32;
        shop.cards.push(ShopCard {
            card_id: c,
            upgrades: preview_shop_upgrades(config, c),
            price,
            can_buy: true,
            blocked_reason: None,
        });
    }

    // Process Colorless Cards Jitter (merchantRng)
    for c in colorless_cards {
        let rarity = get_card_definition(c).rarity;
        let base_price = match rarity {
            crate::content::cards::CardRarity::Common => 50.0,
            crate::content::cards::CardRarity::Uncommon => 75.0,
            crate::content::cards::CardRarity::Rare => 150.0,
            _ => 50.0,
        };
        let mut tmp_price = base_price * rng_pool.merchant_rng.random_f32_min_max(0.9, 1.1);
        tmp_price *= 1.2; // Colorless bump
        let price = tmp_price.trunc() as i32;
        shop.cards.push(ShopCard {
            card_id: c,
            upgrades: preview_shop_upgrades(config, c),
            price,
            can_buy: true,
            blocked_reason: None,
        });
    }

    // Apply Sale tag via integer division on a random colored card (0-4)
    let sale_index = rng_pool.merchant_rng.random_range(0, 4) as usize;
    shop.cards[sale_index].price /= 2;

    // 2. Relics (3 slots via merchantRng)
    for i in 0..3 {
        let tier = if i != 2 {
            let roll = rng_pool.merchant_rng.random(99);
            if roll < 48 {
                RelicTier::Common
            } else if roll < 82 {
                RelicTier::Uncommon
            } else {
                RelicTier::Rare
            }
        } else {
            RelicTier::Shop
        };

        let relic_id = get_relic(tier);
        let base_price: f32 = match tier {
            RelicTier::Common => 150.0,
            RelicTier::Uncommon => 250.0,
            RelicTier::Rare => 300.0,
            RelicTier::Shop => 150.0,
            _ => 150.0,
        };

        let jitter_mult = rng_pool.merchant_rng.random_f32_min_max(0.95, 1.05);
        let price = (base_price * jitter_mult).round() as i32;
        shop.relics.push(ShopRelic {
            relic_id,
            price,
            can_buy: true,
            blocked_reason: None,
        });
    }

    // 3. Potions (3 slots via potionRng & merchantRng)
    let pc = config.potion_class;
    for _ in 0..3 {
        let potion_id = potions::random_potion(&mut rng_pool.potion_rng, pc, false);
        let base_price = potions::get_potion_price(potion_id) as f32;
        let jitter_mult = rng_pool.merchant_rng.random_f32_min_max(0.95, 1.05);
        let price = (base_price * jitter_mult).round() as i32;
        shop.potions.push(ShopPotion {
            potion_id,
            price,
            can_buy: true,
            blocked_reason: None,
        });
    }

    // 4. Initial Purge Cost
    let base_purge_cost = 75.0 + (config.previous_purge_count as f32 * 25.0);
    shop.purge_cost = base_purge_cost as i32;

    // 5. Apply Discounts Sequentially (ShopScreen.init logic)
    let ascension_level = config.ascension_level;
    let has_courier = config.has_courier;
    let has_membership_card = config.has_membership_card;
    let has_smiling_mask = config.has_smiling_mask;

    let mut pass = |mult: f32, affect_purge: bool| {
        for c in shop.cards.iter_mut() {
            c.price = (c.price as f32 * mult).round() as i32;
        }
        for r in shop.relics.iter_mut() {
            r.price = (r.price as f32 * mult).round() as i32;
        }
        for p in shop.potions.iter_mut() {
            p.price = (p.price as f32 * mult).round() as i32;
        }
        if affect_purge {
            shop.purge_cost = (base_purge_cost * mult).round() as i32;
        }
    };

    if ascension_level >= 16 {
        pass(1.1, false);
    }
    if has_courier {
        pass(0.8, true);
    }
    if has_membership_card {
        pass(0.5, true);
    }
    if has_smiling_mask {
        shop.purge_cost = 50;
    }

    shop
}

#[cfg(test)]
mod tests {
    use super::generate_shop;
    use crate::content::cards::{get_card_definition, CardType};
    use crate::content::potions::PotionClass;
    use crate::content::relics::{RelicId, RelicTier};
    use crate::runtime::rng::RngPool;
    use crate::state::shop::state::ShopConfig;

    fn fixed_relic(tier: RelicTier) -> RelicId {
        match tier {
            RelicTier::Common => RelicId::Anchor,
            RelicTier::Uncommon => RelicId::LetterOpener,
            RelicTier::Rare => RelicId::LizardTail,
            RelicTier::Shop => RelicId::MembershipCard,
            _ => RelicId::Anchor,
        }
    }

    #[test]
    fn initial_shop_uses_original_single_bound_rarity_rolls_and_two_bound_sale_roll() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};
        let mut rng_pool = RngPool::new(1);
        rng_pool.bind_trace_owners();
        let trace = begin_sts_rng_trace().unwrap();
        generate_shop(
            &mut rng_pool,
            &ShopConfig {
                ascension_level: 20,
                player_class: "Ironclad",
                has_courier: false,
                has_membership_card: false,
                has_smiling_mask: false,
                has_molten_egg: false,
                has_toxic_egg: false,
                has_frozen_egg: false,
                previous_purge_count: 0,
                potion_class: PotionClass::Ironclad,
                card_blizz_randomizer: 5,
            },
            fixed_relic,
        );
        let calls = trace.finish().unwrap();
        // AbstractDungeon.rollRarity and ShopScreen.rollRelicTier call
        // random(99); initCards chooses the sale with random(0, 4).
        assert!(matches!(
            calls[0].call,
            StsRngCall::RandomInt { range: 99, .. }
        ));
        assert_eq!(
            calls
                .iter()
                .filter(|call| call.owner == Some("run.merchant")
                    && matches!(call.call, StsRngCall::RandomInt { range: 99, .. }))
                .count(),
            2
        );
        assert_eq!(
            calls
                .iter()
                .filter(|call| call.owner == Some("run.merchant")
                    && matches!(
                        call.call,
                        StsRngCall::RandomIntRange {
                            start: 0,
                            end: 4,
                            ..
                        }
                    ))
                .count(),
            1
        );
        assert!(!calls.iter().any(
            |call| matches!(call.owner, Some("run.card" | "run.merchant"))
                && matches!(
                    call.call,
                    StsRngCall::RandomIntRange {
                        start: 0,
                        end: 99,
                        ..
                    }
                )
        ));
    }

    #[test]
    fn membership_card_overrides_courier_for_initial_purge_cost() {
        let mut rng_pool = RngPool::new(1);
        let shop = generate_shop(
            &mut rng_pool,
            &ShopConfig {
                ascension_level: 0,
                player_class: "Ironclad",
                has_courier: true,
                has_membership_card: true,
                has_smiling_mask: false,
                has_molten_egg: false,
                has_toxic_egg: false,
                has_frozen_egg: false,
                previous_purge_count: 1,
                potion_class: PotionClass::Ironclad,
                card_blizz_randomizer: 5,
            },
            fixed_relic,
        );
        assert_eq!(shop.purge_cost, 50);
    }

    #[test]
    fn smiling_mask_overrides_discounted_initial_purge_cost() {
        let mut rng_pool = RngPool::new(1);
        let shop = generate_shop(
            &mut rng_pool,
            &ShopConfig {
                ascension_level: 0,
                player_class: "Ironclad",
                has_courier: true,
                has_membership_card: true,
                has_smiling_mask: true,
                has_molten_egg: false,
                has_toxic_egg: false,
                has_frozen_egg: false,
                previous_purge_count: 3,
                potion_class: PotionClass::Ironclad,
                card_blizz_randomizer: 5,
            },
            fixed_relic,
        );
        assert_eq!(shop.purge_cost, 50);
    }

    #[test]
    fn initial_shop_card_previews_apply_existing_egg_relics() {
        let mut rng_pool = RngPool::new(1);
        let shop = generate_shop(
            &mut rng_pool,
            &ShopConfig {
                ascension_level: 0,
                player_class: "Ironclad",
                has_courier: false,
                has_membership_card: false,
                has_smiling_mask: false,
                has_molten_egg: true,
                has_toxic_egg: true,
                has_frozen_egg: true,
                previous_purge_count: 0,
                potion_class: PotionClass::Ironclad,
                card_blizz_randomizer: 5,
            },
            fixed_relic,
        );

        assert!(!shop.cards.is_empty());
        for card in &shop.cards {
            let def = get_card_definition(card.card_id);
            if matches!(
                def.card_type,
                CardType::Attack | CardType::Skill | CardType::Power
            ) {
                assert_eq!(card.upgrades, 1);
            }
        }
    }
}
