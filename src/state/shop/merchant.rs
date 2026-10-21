use crate::content::cards::{colorless_pool_for_rarity, get_card_definition, java_id};
use crate::content::cards::{CardId, CardRarity, CardType};
use crate::runtime::rng::RngPool;

pub fn roll_shop_card_rarity(rng_pool: &mut RngPool, blizz_randomizer: i32) -> CardRarity {
    let roll = rng_pool.card_rng.random(99) + blizz_randomizer;
    if roll < 9 {
        CardRarity::Rare
    } else if roll < 46 {
        CardRarity::Uncommon
    } else {
        CardRarity::Common
    }
}

pub fn random_shop_colored_card_of_type(
    rng_pool: &mut RngPool,
    player_class: &str,
    blizz_randomizer: i32,
    card_type: CardType,
) -> CardId {
    random_shop_colored_card_of_type_using_card_rng(
        rng_pool,
        player_class,
        blizz_randomizer,
        card_type,
    )
}

pub fn random_shop_colored_card_of_type_for_courier_restock(
    rng_pool: &mut RngPool,
    player_class: &str,
    blizz_randomizer: i32,
    card_type: CardType,
) -> CardId {
    random_shop_colored_card_of_type_using_math_selection(
        rng_pool,
        player_class,
        blizz_randomizer,
        card_type,
    )
}

fn random_shop_colored_card_of_type_using_card_rng(
    rng_pool: &mut RngPool,
    player_class: &str,
    blizz_randomizer: i32,
    card_type: CardType,
) -> CardId {
    let rarity = roll_shop_card_rarity(rng_pool, blizz_randomizer);
    let mut typed_pool = shop_card_pool_from_java_rarity_path(
        player_class,
        rarity,
        card_type,
    )
    .unwrap_or_else(|| {
        panic!(
            "missing Java shop card pool for class {player_class}, rarity {rarity:?}, type {card_type:?}"
        )
    });

    typed_pool.sort_by_key(|&id| java_id(id));

    let idx = rng_pool.card_rng.random(typed_pool.len() as i32 - 1) as usize;
    typed_pool[idx]
}

fn random_shop_colored_card_of_type_using_math_selection(
    rng_pool: &mut RngPool,
    player_class: &str,
    blizz_randomizer: i32,
    card_type: CardType,
) -> CardId {
    let rarity = roll_shop_card_rarity(rng_pool, blizz_randomizer);
    let mut typed_pool = shop_card_pool_from_java_rarity_path(
        player_class,
        rarity,
        card_type,
    )
    .unwrap_or_else(|| {
        panic!(
            "missing Java shop card pool for class {player_class}, rarity {rarity:?}, type {card_type:?}"
        )
    });

    typed_pool.sort_by_key(|&id| java_id(id));

    let idx = rng_pool.math_rng.random(typed_pool.len() as i32 - 1) as usize;
    typed_pool[idx]
}

fn shop_card_pool_from_java_rarity_path(
    player_class: &str,
    rarity: CardRarity,
    card_type: CardType,
) -> Option<Vec<CardId>> {
    for &candidate_rarity in shop_rarity_path_like_java_get_card_from_pool(rarity, card_type) {
        let typed_pool: Vec<CardId> =
            crate::engine::campfire_handler::card_pool_for_class(player_class, candidate_rarity)
                .iter()
                .copied()
                .filter(|&id| get_card_definition(id).card_type == card_type)
                .collect();
        if !typed_pool.is_empty() {
            return Some(typed_pool);
        }
    }
    None
}

fn shop_rarity_path_like_java_get_card_from_pool(
    rarity: CardRarity,
    card_type: CardType,
) -> &'static [CardRarity] {
    use CardRarity::{Common, Rare, Uncommon};

    match (rarity, card_type) {
        (Rare, CardType::Power) => &[Rare, Uncommon],
        (Uncommon, CardType::Power) => &[Uncommon, Rare],
        (Common, CardType::Power) => &[Common, Uncommon, Rare],
        (Rare, _) => &[Rare, Uncommon, Common],
        (Uncommon, _) => &[Uncommon, Common],
        (Common, _) => &[Common],
        _ => &[],
    }
}

pub fn random_shop_colorless_card(rng_pool: &mut RngPool, rarity: CardRarity) -> CardId {
    let pool = colorless_pool_for_rarity(rarity);
    let mut typed_pool = pool.to_vec();
    typed_pool.sort_by_key(|&id| java_id(id));
    let idx = rng_pool.card_rng.random(typed_pool.len() as i32 - 1) as usize;
    typed_pool[idx]
}

/// Equivalent to Java's `Merchant` class constructor.
/// Generates 5 colored cards (2 Attack, 2 Skill, 1 Power) and 2 colorless cards (1 Uncommon, 1 Rare).
pub fn generate_cards(
    rng_pool: &mut RngPool,
    player_class: &str,
    blizz_randomizer: i32,
) -> (Vec<CardId>, Vec<CardId>) {
    let c1_atk = random_shop_colored_card_of_type(
        rng_pool,
        player_class,
        blizz_randomizer,
        CardType::Attack,
    );
    let mut c2_atk = random_shop_colored_card_of_type(
        rng_pool,
        player_class,
        blizz_randomizer,
        CardType::Attack,
    );
    while c2_atk == c1_atk {
        c2_atk = random_shop_colored_card_of_type(
            rng_pool,
            player_class,
            blizz_randomizer,
            CardType::Attack,
        );
    }

    let c3_skl =
        random_shop_colored_card_of_type(rng_pool, player_class, blizz_randomizer, CardType::Skill);
    let mut c4_skl =
        random_shop_colored_card_of_type(rng_pool, player_class, blizz_randomizer, CardType::Skill);
    while c4_skl == c3_skl {
        c4_skl = random_shop_colored_card_of_type(
            rng_pool,
            player_class,
            blizz_randomizer,
            CardType::Skill,
        );
    }

    let c5_pwr =
        random_shop_colored_card_of_type(rng_pool, player_class, blizz_randomizer, CardType::Power);

    let colored_cards = vec![c1_atk, c2_atk, c3_skl, c4_skl, c5_pwr];

    let c6_clr_unc = random_shop_colorless_card(rng_pool, CardRarity::Uncommon);
    let c7_clr_rar = random_shop_colorless_card(rng_pool, CardRarity::Rare);

    let colorless_cards = vec![c6_clr_unc, c7_clr_rar];

    (colored_cards, colorless_cards)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(rarity: CardRarity, card_type: CardType) -> Vec<CardRarity> {
        shop_rarity_path_like_java_get_card_from_pool(rarity, card_type).to_vec()
    }

    #[test]
    fn shop_attack_and_skill_rarity_paths_match_java_fallthrough() {
        assert_eq!(
            path(CardRarity::Rare, CardType::Attack),
            vec![CardRarity::Rare, CardRarity::Uncommon, CardRarity::Common]
        );
        assert_eq!(
            path(CardRarity::Uncommon, CardType::Attack),
            vec![CardRarity::Uncommon, CardRarity::Common]
        );
        assert_eq!(
            path(CardRarity::Common, CardType::Attack),
            vec![CardRarity::Common]
        );
        assert_eq!(
            path(CardRarity::Rare, CardType::Skill),
            vec![CardRarity::Rare, CardRarity::Uncommon, CardRarity::Common]
        );
        assert_eq!(
            path(CardRarity::Uncommon, CardType::Skill),
            vec![CardRarity::Uncommon, CardRarity::Common]
        );
        assert_eq!(
            path(CardRarity::Common, CardType::Skill),
            vec![CardRarity::Common]
        );
    }

    #[test]
    fn shop_power_rarity_paths_match_java_recursive_power_fallbacks() {
        assert_eq!(
            path(CardRarity::Rare, CardType::Power),
            vec![CardRarity::Rare, CardRarity::Uncommon]
        );
        assert_eq!(
            path(CardRarity::Uncommon, CardType::Power),
            vec![CardRarity::Uncommon, CardRarity::Rare]
        );
        assert_eq!(
            path(CardRarity::Common, CardType::Power),
            vec![CardRarity::Common, CardRarity::Uncommon, CardRarity::Rare]
        );
    }

    #[test]
    fn courier_colored_restock_uses_card_rng_for_rarity_and_math_rng_for_card_selection() {
        let mut initial = RngPool::new(7);
        let mut courier = RngPool::new(7);

        let _ = random_shop_colored_card_of_type(&mut initial, "Ironclad", 0, CardType::Attack);
        let _ = random_shop_colored_card_of_type_for_courier_restock(
            &mut courier,
            "Ironclad",
            0,
            CardType::Attack,
        );

        assert_eq!(
            courier.card_rng.counter, 1,
            "Courier colored restock should consume cardRng only for AbstractDungeon.rollRarity()"
        );
        assert_eq!(
            courier.math_rng.counter, 1,
            "Courier colored restock should consume the isolated MathUtils selection stream"
        );
        assert_eq!(
            initial.card_rng.counter, 2,
            "Initial Merchant colored cards use cardRng for rarity and CardGroup selection"
        );
        assert_eq!(initial.math_rng.counter, 0);
    }
}
