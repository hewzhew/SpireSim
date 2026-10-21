use super::*;

impl RunState {
    /// Java AbstractDungeon.getColorlessCardFromPool: rarity-filtered cards
    /// sorted by Java card ID, selected by one inclusive cardRng draw.
    pub fn random_colorless_card_from_pool(
        &mut self,
        rarity: crate::content::cards::CardRarity,
    ) -> crate::content::cards::CardId {
        use crate::content::cards::{colorless_pool_for_rarity, java_id, CardRarity};
        let mut pool = colorless_pool_for_rarity(rarity).to_vec();
        if pool.is_empty() && rarity == CardRarity::Rare {
            pool = colorless_pool_for_rarity(CardRarity::Uncommon).to_vec();
        }
        pool.sort_by_key(|id| java_id(*id));
        let index = self.rng_pool.card_rng.random(pool.len() as i32 - 1) as usize;
        pool[index]
    }

    /// Java 2.3.4 `AbstractDungeon.returnRandomCurse()` via
    /// `CardLibrary.getCurse()`: one inclusive `cardRng` index into the
    /// ordinary obtainable curse pool.
    pub fn random_obtainable_curse(&mut self) -> crate::content::cards::CardId {
        let pool = crate::content::cards::get_curse_pool();
        let index = self
            .rng_pool
            .card_rng
            .random_range(0, pool.len() as i32 - 1) as usize;
        pool[index]
    }

    /// Returns a random colorless card of the given rarity.
    /// Mirrors Java returnColorlessCard(rarity): shuffle pool, pick first matching rarity.
    pub fn random_colorless_card(
        &mut self,
        rarity: crate::content::cards::CardRarity,
    ) -> crate::content::cards::CardId {
        use crate::content::cards::*;
        let mut pool = COLORLESS_UNCOMMON_POOL
            .iter()
            .copied()
            .chain(COLORLESS_RARE_POOL.iter().copied())
            .collect::<Vec<_>>();
        let seed = self.rng_pool.shuffle_rng.random_long();
        let mut jur = crate::runtime::rng::JavaUtilRandom::new(seed);
        for i in (1..pool.len()).rev() {
            let j = jur.next_int((i + 1) as i32) as usize;
            pool.swap(i, j);
        }

        if let Some(card_id) = pool
            .iter()
            .copied()
            .find(|card_id| get_card_definition(*card_id).rarity == rarity)
        {
            return card_id;
        }
        if rarity == CardRarity::Rare {
            if let Some(card_id) = pool
                .iter()
                .copied()
                .find(|card_id| get_card_definition(*card_id).rarity == CardRarity::Uncommon)
            {
                return card_id;
            }
        }
        CardId::SwiftStrike
    }

    /// Returns a random card from the current class pool of the given rarity.
    /// Mirrors Java `getCard(rarity)` via `CardGroup.getRandomCard(true)`:
    /// one single-bound inclusive `cardRng.random(size - 1)` call.
    pub fn random_card_by_rarity(
        &mut self,
        rarity: crate::content::cards::CardRarity,
    ) -> crate::content::cards::CardId {
        use crate::content::cards::CardId;
        let pool = crate::engine::campfire_handler::nonempty_card_pool_for_class(
            self.player_class,
            rarity,
        );
        if pool.is_empty() {
            return match self.player_class {
                "Silent" => CardId::StrikeG,
                "Defect" => CardId::StrikeB,
                "Watcher" => CardId::StrikeP,
                _ => CardId::Strike,
            };
        }
        let idx = self.rng_pool.card_rng.random(pool.len() as i32 - 1) as usize;
        pool[idx]
    }

    /// Returns a random Ironclad card of the given CardType (Attack/Skill/Power).
    /// Mirrors Java `returnTrulyRandomCardInCombat(type)` — used by Attack/Skill/Power Potions.
    pub fn random_card_by_type(
        &mut self,
        card_type: crate::content::cards::CardType,
    ) -> crate::content::cards::CardId {
        use crate::content::cards::*;
        let pool = match self.player_class {
            "Silent" => silent_pool_for_type(card_type),
            "Defect" => defect_pool_for_type(card_type),
            "Watcher" => watcher_pool_for_type(card_type),
            _ => ironclad_pool_for_type(card_type),
        };
        if pool.is_empty() {
            return match self.player_class {
                "Silent" => CardId::StrikeG,
                "Defect" => CardId::StrikeB,
                "Watcher" => CardId::StrikeP,
                _ => CardId::Strike,
            };
        }
        let idx = self
            .rng_pool
            .misc_rng
            .random_range(0, pool.len() as i32 - 1) as usize;
        pool[idx]
    }
}
