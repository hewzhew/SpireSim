use super::*;

impl RunState {
    /// Primary entry point for adding a new relic to the run.
    /// Handles appending to the relics array and immediately dispatches to the RelicManager
    /// for onEquip hooks (e.g. increasing Max HP or interrupting the engine state with a UI).
    pub fn obtain_relic(
        &mut self,
        relic_id: crate::content::relics::RelicId,
        return_state: crate::state::core::EngineState,
    ) -> Option<crate::state::core::EngineState> {
        self.obtain_relic_with_source(relic_id, return_state, DomainEventSource::DeckMutation)
    }

    pub fn obtain_relic_with_source(
        &mut self,
        relic_id: crate::content::relics::RelicId,
        return_state: crate::state::core::EngineState,
        source: DomainEventSource,
    ) -> Option<crate::state::core::EngineState> {
        if relic_id == crate::content::relics::RelicId::Circlet {
            if let Some(circlet) = self
                .relics
                .iter_mut()
                .find(|relic| relic.id == crate::content::relics::RelicId::Circlet)
            {
                circlet.counter += 1;
                self.emit_event(DomainEvent::RelicObtained { relic_id, source });
                return None;
            }
        }

        self.relics
            .push(crate::content::relics::RelicState::new(relic_id));
        self.emit_event(DomainEvent::RelicObtained { relic_id, source });
        // Resource effects go through RunState's mutation owners, which emit
        // their own events. A second aggregate diff would duplicate them.
        crate::engine::relic_manager::on_equip(self, relic_id, return_state)
    }

    pub fn obtain_boss_relic_choice_with_source(
        &mut self,
        relic_id: crate::content::relics::RelicId,
        return_state: crate::state::core::EngineState,
        source: DomainEventSource,
    ) -> Option<crate::state::core::EngineState> {
        if is_starter_upgrade_boss_relic(relic_id) && !self.relics.is_empty() {
            let replaced_relic_id = self.relics[0].id;

            // Java BossRelicSelectScreen calls instantObtain(player, 0, true)
            // for these boss relics. That overwrites relic slot 0 and calls
            // onEquip, but does not run the old relic's onUnequip hook.
            self.relics[0] = crate::content::relics::RelicState::new(relic_id);
            self.emit_event(DomainEvent::RelicLost {
                relic_id: replaced_relic_id,
                source,
            });
            self.emit_event(DomainEvent::RelicObtained { relic_id, source });
            return crate::engine::relic_manager::on_equip(self, relic_id, return_state);
        }

        self.obtain_relic_with_source(relic_id, return_state, source)
    }

    pub fn obtain_relic_at_with_source(
        &mut self,
        relic_id: crate::content::relics::RelicId,
        index: usize,
        return_state: crate::state::core::EngineState,
        source: DomainEventSource,
    ) -> Option<crate::state::core::EngineState> {
        if relic_id == crate::content::relics::RelicId::Circlet {
            if let Some(circlet) = self
                .relics
                .iter_mut()
                .find(|relic| relic.id == crate::content::relics::RelicId::Circlet)
            {
                circlet.counter += 1;
                self.emit_event(DomainEvent::RelicObtained { relic_id, source });
                return None;
            }
        }

        let insert_index = index.min(self.relics.len());
        self.relics.insert(
            insert_index,
            crate::content::relics::RelicState::new(relic_id),
        );
        self.emit_event(DomainEvent::RelicObtained { relic_id, source });
        crate::engine::relic_manager::on_equip(self, relic_id, return_state)
    }

    pub fn remove_relic_at_with_source(
        &mut self,
        index: usize,
        source: DomainEventSource,
    ) -> Option<crate::content::relics::RelicId> {
        if index >= self.relics.len() {
            return None;
        }
        let relic = self.relics.remove(index);
        self.emit_event(DomainEvent::RelicLost {
            relic_id: relic.id,
            source,
        });
        crate::engine::relic_manager::on_unequip(self, relic.id, source);
        Some(relic.id)
    }

    pub fn remove_first_relic_with_id_and_source(
        &mut self,
        relic_id: crate::content::relics::RelicId,
        source: DomainEventSource,
    ) -> Option<crate::content::relics::RelicId> {
        self.relics
            .iter()
            .position(|relic| relic.id == relic_id)
            .and_then(|index| self.remove_relic_at_with_source(index, source))
    }

    /// Initialize relic pools. Called at dungeon start.
    /// Java: initializeRelicList() + Collections.shuffle(pool, new Random(relicRng.randomLong()))
    pub fn init_relic_pools(&mut self) {
        use crate::content::relics::{build_relic_pool, RelicTier};
        let player_class = self.player_class;

        self.common_relic_pool = build_relic_pool(RelicTier::Common, player_class);
        self.uncommon_relic_pool = build_relic_pool(RelicTier::Uncommon, player_class);
        self.rare_relic_pool = build_relic_pool(RelicTier::Rare, player_class);
        self.shop_relic_pool = build_relic_pool(RelicTier::Shop, player_class);
        self.boss_relic_pool = build_relic_pool(RelicTier::Boss, player_class);

        // Shuffle each pool with relicRng.randomLong() as seed (Java pattern)
        crate::runtime::rng::shuffle_with_random_long(
            &mut self.common_relic_pool,
            &mut self.rng_pool.relic_rng,
        );
        crate::runtime::rng::shuffle_with_random_long(
            &mut self.uncommon_relic_pool,
            &mut self.rng_pool.relic_rng,
        );
        crate::runtime::rng::shuffle_with_random_long(
            &mut self.rare_relic_pool,
            &mut self.rng_pool.relic_rng,
        );
        crate::runtime::rng::shuffle_with_random_long(
            &mut self.shop_relic_pool,
            &mut self.rng_pool.relic_rng,
        );
        crate::runtime::rng::shuffle_with_random_long(
            &mut self.boss_relic_pool,
            &mut self.rng_pool.relic_rng,
        );

        // Java shuffles full pools first, then removes relicsToRemoveOnStart.
        // Removing before shuffle changes the order of the remaining relics.
        let owned: Vec<crate::content::relics::RelicId> =
            self.relics.iter().map(|r| r.id).collect();
        for &id in &owned {
            self.common_relic_pool.retain(|&r| r != id);
            self.uncommon_relic_pool.retain(|&r| r != id);
            self.rare_relic_pool.retain(|&r| r != id);
            self.shop_relic_pool.retain(|&r| r != id);
            self.boss_relic_pool.retain(|&r| r != id);
        }
    }

    /// Roll a random relic tier using relicRng.
    /// Java: returnRandomRelicTier() — roll 0..99, thresholds: Common 50, Uncommon 33, Rare 17.
    pub fn return_random_relic_tier(&mut self) -> crate::content::relics::RelicTier {
        use crate::content::relics::RelicTier;
        let roll = self.rng_pool.relic_rng.random_range(0, 99);
        if roll < 50 {
            RelicTier::Common
        } else if roll < 83 {
            RelicTier::Uncommon
        } else {
            RelicTier::Rare
        }
    }

    /// Pop a relic from the specified tier pool using Java's normal reward
    /// path. Common/uncommon/rare/shop pools consume from the front
    /// (`returnRandomRelicKey` / `remove(0)`), while boss relics also consume
    /// from the front. Shop screens use `random_relic_end_by_tier`.
    ///
    /// Java quirk: if a front candidate fails `canSpawn`, the fallback is
    /// `returnEndRandomRelicKey(tier)`, not another front draw.
    pub fn random_relic_by_tier(
        &mut self,
        tier: crate::content::relics::RelicTier,
    ) -> crate::content::relics::RelicId {
        let spawn_context = RelicSpawnContext::from_run(self);
        let mut relic_pools = RelicPoolsMut {
            common: &mut self.common_relic_pool,
            uncommon: &mut self.uncommon_relic_pool,
            rare: &mut self.rare_relic_pool,
            shop: &mut self.shop_relic_pool,
            boss: &mut self.boss_relic_pool,
        };
        random_relic_by_tier_from_pools(tier, &mut relic_pools, &spawn_context)
    }

    /// Pop a relic using Java's shop/end path
    /// (`returnEndRandomRelicKey`). Common/uncommon/rare/shop pools consume
    /// from the end. Empty common/uncommon/shop pools fall back to the normal
    /// front path of the next tier, matching Java's odd mixed fallback.
    pub fn random_relic_end_by_tier(
        &mut self,
        tier: crate::content::relics::RelicTier,
    ) -> crate::content::relics::RelicId {
        let spawn_context = RelicSpawnContext::from_run(self);
        let mut relic_pools = RelicPoolsMut {
            common: &mut self.common_relic_pool,
            uncommon: &mut self.uncommon_relic_pool,
            rare: &mut self.rare_relic_pool,
            shop: &mut self.shop_relic_pool,
            boss: &mut self.boss_relic_pool,
        };
        random_relic_end_by_tier_from_pools(tier, &mut relic_pools, &spawn_context)
    }

    #[cfg(test)]
    pub(super) fn relic_can_spawn_now(&self, id: crate::content::relics::RelicId) -> bool {
        crate::state::relic_pool::relic_can_spawn_in_context(id, &RelicSpawnContext::from_run(self))
    }

    /// Returns a random "screenless" relic of the given tier.
    /// Skips relics that require UI interaction (BottledFlame/Lightning/Tornado/Whetstone).
    /// Java: returnRandomScreenlessRelic(tier)
    pub fn random_screenless_relic(
        &mut self,
        tier: crate::content::relics::RelicTier,
    ) -> crate::content::relics::RelicId {
        use crate::content::relics::RelicId;
        loop {
            let id = self.random_relic_by_tier(tier);
            match id {
                RelicId::BottledFlame
                | RelicId::BottledLightning
                | RelicId::BottledTornado
                | RelicId::Whetstone => {
                    // Skip — these need grid select. Pop next from same tier.
                    continue;
                }
                _ => return id,
            }
        }
    }

    /// Returns a random non-campfire relic of the given tier.
    /// Java: `AbstractDungeon.returnRandomNonCampfireRelic(tier)`.
    ///
    /// This is used by Black Star's second elite relic. Java repeatedly draws
    /// from the same tier until the result is not Peace Pipe, Shovel, or Girya;
    /// skipped relics are consumed from the pool.
    pub fn random_noncampfire_relic(
        &mut self,
        tier: crate::content::relics::RelicTier,
    ) -> crate::content::relics::RelicId {
        use crate::content::relics::RelicId;
        loop {
            let id = self.random_relic_by_tier(tier);
            match id {
                RelicId::PeacePipe | RelicId::Shovel | RelicId::Girya => continue,
                _ => return id,
            }
        }
    }

    /// Roll a relic tier, then return a Java screenless relic from that tier.
    /// Used by events that call `AbstractDungeon.returnRandomScreenlessRelic`
    /// rather than room reward generation.
    pub fn random_screenless_relic_reward(&mut self) -> crate::content::relics::RelicId {
        let tier = self.return_random_relic_tier();
        self.random_screenless_relic(tier)
    }

    /// Roll a relic tier, then return a Java non-campfire relic from that tier.
    /// Used by elite Black Star rewards.
    pub fn random_noncampfire_relic_reward(&mut self) -> crate::content::relics::RelicId {
        let tier = self.return_random_relic_tier();
        self.random_noncampfire_relic(tier)
    }

    /// Default random relic reward: roll tier then return a normal relic from
    /// that tier. Java combat/chest reward paths use `returnRandomRelic`, not
    /// `returnRandomScreenlessRelic`, so screen-interrupting relics such as
    /// Bottled Flame can appear here.
    pub fn random_relic(&mut self) -> crate::content::relics::RelicId {
        let tier = self.return_random_relic_tier();
        self.random_relic_by_tier(tier)
    }
}

fn is_starter_upgrade_boss_relic(relic_id: crate::content::relics::RelicId) -> bool {
    matches!(
        relic_id,
        crate::content::relics::RelicId::BlackBlood
            | crate::content::relics::RelicId::RingOfTheSerpent
            | crate::content::relics::RelicId::FrozenCore
            | crate::content::relics::RelicId::HolyWater
    )
}
