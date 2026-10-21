use super::*;

impl RunState {
    pub fn change_gold_with_source(&mut self, delta: i32, source: DomainEventSource) -> i32 {
        if delta > 0
            && self
                .relics
                .iter()
                .any(|relic| crate::content::relics::mechanics::relic_prevents_gold_gain(relic.id))
        {
            return 0;
        }

        let old_gold = self.gold;
        self.gold = (self.gold + delta).max(0);
        let actual_delta = self.gold - old_gold;
        if actual_delta < 0 && matches!(source, DomainEventSource::Shop) {
            if let Some(relic) = self
                .relics
                .iter_mut()
                .find(|relic| relic.id == crate::content::relics::RelicId::MawBank)
            {
                relic.used_up = true;
                relic.counter = -2;
            }
        }
        if actual_delta != 0 {
            self.emit_event(DomainEvent::GoldChanged {
                delta: actual_delta,
                new_total: self.gold,
                source,
            });
        }
        if actual_delta > 0 {
            if let Some((relic, amount)) = self.relics.iter().find_map(|relic| {
                crate::content::relics::mechanics::relic_gold_gain_healing_v1(relic.id)
                    .map(|amount| (relic.id, amount))
            }) {
                self.heal_with_source(amount, DomainEventSource::Relic(relic));
            }
        }
        actual_delta
    }

    pub fn set_gold_with_source(&mut self, new_total: i32, source: DomainEventSource) -> i32 {
        self.change_gold_with_source(new_total - self.gold, source)
    }

    pub fn change_hp_with_source(&mut self, delta: i32, source: DomainEventSource) -> i32 {
        self.set_current_hp_with_source(self.current_hp + delta, source)
    }

    pub fn heal_with_source(&mut self, amount: i32, source: DomainEventSource) -> i32 {
        if amount <= 0 {
            return 0;
        }
        if self
            .relics
            .iter()
            .any(|r| r.id == crate::content::relics::RelicId::MarkOfTheBloom)
        {
            return 0;
        }
        self.change_hp_with_source(amount, source)
    }

    pub fn set_current_hp_with_source(
        &mut self,
        new_current_hp: i32,
        source: DomainEventSource,
    ) -> i32 {
        let old_hp = self.current_hp;
        self.current_hp = new_current_hp.clamp(0, self.max_hp.max(0));
        let actual_delta = self.current_hp - old_hp;
        if actual_delta != 0 {
            self.emit_event(DomainEvent::HpChanged {
                delta: actual_delta,
                current_hp: self.current_hp,
                max_hp: self.max_hp,
                source,
            });
        }
        actual_delta
    }

    pub fn gain_max_hp_with_source(
        &mut self,
        amount: i32,
        heal_amount: i32,
        source: DomainEventSource,
    ) -> i32 {
        if amount <= 0 {
            return 0;
        }
        self.max_hp += amount;
        self.heal_with_source(heal_amount, source);
        self.emit_event(DomainEvent::MaxHpChanged {
            delta: amount,
            current_hp: self.current_hp,
            max_hp: self.max_hp,
            source,
        });
        amount
    }

    pub fn lose_max_hp_with_source(&mut self, amount: i32, source: DomainEventSource) -> i32 {
        if amount <= 0 {
            return 0;
        }
        let old_max_hp = self.max_hp;
        self.max_hp = (self.max_hp - amount).max(1);
        self.current_hp = self.current_hp.min(self.max_hp);
        let actual_delta = self.max_hp - old_max_hp;
        if actual_delta != 0 {
            self.emit_event(DomainEvent::MaxHpChanged {
                delta: actual_delta,
                current_hp: self.current_hp,
                max_hp: self.max_hp,
                source,
            });
        }
        actual_delta
    }

    /// Triggers when the player enters a Rest Room (Campfire).
    pub fn on_enter_rest_room(&mut self) {
        for relic in &mut self.relics {
            let sub = crate::content::relics::get_relic_subscriptions(relic.id);
            if sub.on_enter_rest_room && relic.id == crate::content::relics::RelicId::AncientTeaSet
            {
                crate::content::relics::ancient_tea_set::AncientTeaSet::on_enter_rest_room(relic);
            }
        }
    }

    /// Generates ShopState with randomized prices, accounting for merchant Relics
    pub fn generate_shop(&mut self) -> crate::state::shop::ShopState {
        let config = crate::state::shop::state::ShopConfig {
            ascension_level: self.ascension_level as i32,
            player_class: self.player_class,
            has_courier: self
                .relics
                .iter()
                .any(|r| r.id == crate::content::relics::RelicId::Courier),
            has_membership_card: self
                .relics
                .iter()
                .any(|r| r.id == crate::content::relics::RelicId::MembershipCard),
            has_smiling_mask: self
                .relics
                .iter()
                .any(|r| r.id == crate::content::relics::RelicId::SmilingMask),
            has_molten_egg: self
                .relics
                .iter()
                .any(|r| r.id == crate::content::relics::RelicId::MoltenEgg),
            has_toxic_egg: self
                .relics
                .iter()
                .any(|r| r.id == crate::content::relics::RelicId::ToxicEgg),
            has_frozen_egg: self
                .relics
                .iter()
                .any(|r| r.id == crate::content::relics::RelicId::FrozenEgg),
            previous_purge_count: self.shop_purge_count,
            potion_class: self.potion_class(),
            card_blizz_randomizer: self.card_blizz_randomizer,
        };

        let spawn_context = RelicSpawnContext::from_run(self);

        let crate::state::run::RunState {
            ref mut rng_pool,
            ref mut common_relic_pool,
            ref mut uncommon_relic_pool,
            ref mut rare_relic_pool,
            ref mut shop_relic_pool,
            ref mut boss_relic_pool,
            ..
        } = self;

        let mut relic_pools = RelicPoolsMut {
            common: common_relic_pool,
            uncommon: uncommon_relic_pool,
            rare: rare_relic_pool,
            shop: shop_relic_pool,
            boss: boss_relic_pool,
        };

        crate::state::shop::shop_screen::generate_shop(rng_pool, &config, |tier| {
            random_relic_end_by_tier_from_pools(tier, &mut relic_pools, &spawn_context)
        })
    }

    /// Returns a random potion, weighted by rarity and filtered to the current player class.
    /// Delegates to the canonical `random_potion()` with Java-accurate rarity weights.
    pub fn random_potion(&mut self) -> crate::content::potions::PotionId {
        let potion_class = self.potion_class_from_player();
        crate::content::potions::random_potion(&mut self.rng_pool.potion_rng, potion_class, false)
    }

    /// Java `PotionHelper.getRandomPotion()`: pick one potion uniformly from
    /// the current class potion pool using `AbstractDungeon.potionRng`.
    ///
    /// This is not the same as `AbstractDungeon.returnRandomPotion()`, which
    /// rolls rarity first and then rejection-samples by rarity.
    pub fn random_potion_flat(&mut self) -> crate::content::potions::PotionId {
        let potion_class = self.potion_class_from_player();
        crate::content::potions::random_potion_any(&mut self.rng_pool.potion_rng, potion_class)
    }

    /// Maps player_class string to PotionClass enum.
    pub(super) fn potion_class_from_player(&self) -> crate::content::potions::PotionClass {
        match self.player_class {
            "Silent" => crate::content::potions::PotionClass::Silent,
            "Defect" => crate::content::potions::PotionClass::Defect,
            "Watcher" => crate::content::potions::PotionClass::Watcher,
            _ => crate::content::potions::PotionClass::Ironclad, // default
        }
    }

    /// Attempt to place a potion into the first empty slot, matching Java's
    /// `AbstractPlayer.obtainPotion()`. Returns true if placed, false if full.
    /// This is the ONLY correct way to add potions — never use `potions.push()`.
    pub fn obtain_potion(&mut self, potion: crate::content::potions::Potion) -> bool {
        self.obtain_potion_with_source(potion, DomainEventSource::DeckMutation)
    }

    pub fn obtain_potion_with_source(
        &mut self,
        potion: crate::content::potions::Potion,
        source: DomainEventSource,
    ) -> bool {
        if let Some(slot) = self.potions.iter().position(|p| p.is_none()) {
            let potion_id = potion.id;
            self.potions[slot] = Some(potion);
            self.emit_event(DomainEvent::PotionObtained {
                potion_id,
                slot,
                source,
            });
            true
        } else {
            false
        }
    }

    pub fn remove_potion_at_with_source(
        &mut self,
        slot: usize,
        source: DomainEventSource,
    ) -> Option<crate::content::potions::PotionId> {
        let potion = self.potions.get_mut(slot)?.take()?;
        let potion_id = potion.id;
        self.emit_event(DomainEvent::PotionLost {
            potion_id,
            slot,
            source,
        });
        Some(potion_id)
    }

    /// Find first empty potion slot index, or None if full.
    pub fn find_empty_potion_slot(&self) -> Option<usize> {
        self.potions.iter().position(|p| p.is_none())
    }
}
