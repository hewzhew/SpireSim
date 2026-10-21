use super::*;

// Derived combat runtime state that is recomputed from other sources.
impl CombatState {
    pub fn recompute_turn_start_draw_modifier(&mut self) {
        let mut modifier = 0;
        if self
            .entities
            .player
            .has_relic(crate::content::relics::RelicId::RingOfTheSerpent)
        {
            modifier += 1;
        }
        if let Some(powers) = crate::content::powers::store::powers_for(self, 0) {
            for power in powers {
                match power.power_type {
                    PowerId::Draw => modifier += power.amount,
                    PowerId::DrawReduction => modifier -= power.amount,
                    _ => {}
                }
            }
        }
        self.turn.turn_start_draw_modifier = modifier;
    }
}

// Card-zone utilities used by action handlers to reconcile card movement.
impl CombatState {
    pub fn allocate_combat_generated_lineage(&mut self) -> CardLineageId {
        self.zones.combat_lineage_allocator.allocate()
    }

    pub fn next_power_instance_id(&mut self) -> u32 {
        self.runtime.power_instance_counter += 1;
        self.runtime.power_instance_counter
    }

    /// Java `MonsterGroup.areMonstersBasicallyDead()` only skips monsters that
    /// are `isDying` or `isEscaping`. It does not check current HP and does not
    /// treat `halfDead` as basically dead by itself.
    pub fn are_monsters_basically_dead_java(&self) -> bool {
        self.entities
            .monsters
            .iter()
            .all(|m| m.is_dying || m.is_escaped)
    }

    /// Java `MonsterGroup.haveMonstersEscaped()` returns true only when every
    /// monster has its `escaped` flag set. Dying/dead monsters do not count.
    pub fn have_monsters_escaped_java(&self) -> bool {
        self.entities.monsters.iter().all(|m| m.is_escaped)
    }

    /// Settled draw-pile entry through Java's Soul movement clears transient
    /// card attributes. Raw CardZones insertion remains available for setup
    /// and rearrangement that do not move a card through that lifecycle.
    pub fn add_card_to_draw_pile_top(&mut self, mut card: CombatCard) {
        card.reset_attributes_java();
        self.observe_card_entering_zone(&card, CombatCardBirthZone::Draw);
        self.zones.add_to_draw_pile_top(card);
    }

    pub fn add_card_to_draw_pile_bottom(&mut self, mut card: CombatCard) {
        card.reset_attributes_java();
        self.observe_card_entering_zone(&card, CombatCardBirthZone::Draw);
        self.zones.add_to_draw_pile_bottom(card);
    }

    pub fn add_card_to_draw_pile_random_spot(&mut self, mut card: CombatCard) {
        card.reset_attributes_java();
        self.observe_card_entering_zone(&card, CombatCardBirthZone::Draw);
        let java_insert_index = if self.zones.draw_pile.is_empty() {
            0
        } else {
            self.rng
                .card_random_rng
                .random(self.zones.draw_pile.len() as i32 - 1) as usize
        };
        self.zones
            .add_to_draw_pile_random_spot_from_java_index(card, java_insert_index);
    }

    pub fn draw_top_card(&mut self) -> Option<CombatCard> {
        self.zones.draw_top_card()
    }

    pub fn add_card_to_discard_pile_top(&mut self, card: CombatCard) {
        self.observe_card_entering_zone(&card, CombatCardBirthZone::Discard);
        self.zones.add_to_discard_pile_top(card);
    }

    pub fn add_card_to_exhaust_pile_top(&mut self, card: CombatCard) {
        self.observe_card_entering_zone(&card, CombatCardBirthZone::Exhaust);
        self.zones.add_to_exhaust_pile_top(card);
    }

    pub fn add_card_to_hand(&mut self, card: CombatCard) {
        self.observe_card_entering_zone(&card, CombatCardBirthZone::Hand);
        self.zones.hand.push(card);
    }

    pub fn add_card_to_limbo(&mut self, card: CombatCard) {
        self.observe_card_entering_zone(&card, CombatCardBirthZone::Limbo);
        self.zones.limbo.push(card);
    }

    pub fn shuffle_discard_pile_into_draw_pile(&mut self) {
        let mut discarded = std::mem::take(&mut self.zones.discard_pile).into_vec();
        let observation = self.runtime.oracle_shuffle_observation.0.as_ref().map(|_| {
            (
                discarded
                    .iter()
                    .map(|card| crate::state::DomainCardSnapshot {
                        id: card.id,
                        upgrades: card.upgrades,
                        lineage: card.lineage,
                    })
                    .collect(),
                self.rng.shuffle_rng.clone(),
            )
        });
        crate::runtime::rng::shuffle_with_random_long(&mut discarded, &mut self.rng.shuffle_rng);
        // EmptyDeckShuffleAction shuffles only discardPile.group, even when
        // empty. Soul.shuffle then appends each card to Java draw-pile top.
        // Rust draw top is index 0; preserve the old draw group's order below it.
        discarded.reverse();
        if let Some((input, rng_before)) = observation {
            self.runtime
                .oracle_shuffle_observation
                .0
                .as_mut()
                .unwrap()
                .push(OracleShuffleReceipt {
                    input,
                    output: discarded.iter().map(|card| card.lineage).collect(),
                    rng_before,
                    rng_after: self.rng.shuffle_rng.clone(),
                });
        }
        self.zones.draw_pile.prepend_shuffled(discarded);
    }

    pub fn apply_java_initialize_deck_order_after_shuffle(&mut self) {
        let bottled_lineages: Vec<CardLineageId> = self
            .entities
            .player
            .relics
            .iter()
            .filter_map(|relic| match relic.id {
                crate::content::relics::RelicId::BottledFlame
                | crate::content::relics::RelicId::BottledLightning
                | crate::content::relics::RelicId::BottledTornado
                    if relic.amount > 0 =>
                {
                    CardLineageId::from_runtime_i32_payload(relic.amount)
                }
                _ => None,
            })
            .collect();
        let mut java_group_order = Vec::new();
        let mut place_on_top = Vec::new();
        for card in std::mem::take(&mut self.zones.draw_pile) {
            if crate::content::cards::is_innate_card(&card)
                || bottled_lineages.contains(&card.lineage)
            {
                place_on_top.push(card);
            } else {
                java_group_order.push(card);
            }
        }
        java_group_order.extend(place_on_top);
        java_group_order.reverse();
        self.zones.draw_pile = java_group_order.into();
    }

    /// Finds and removes a card with the requested lineage from one zone.
    pub fn remove_card_by_lineage(
        pile: &mut Vec<CombatCard>,
        lineage: CardLineageId,
    ) -> Option<CombatCard> {
        pile.iter()
            .position(|c| c.lineage == lineage)
            .map(|index| pile.remove(index))
    }

    /// Looks everywhere for a card and removes it. Useful for UseCard when we don't know exactly where the card went.
    pub fn take_card_by_lineage_from_anywhere(
        &mut self,
        lineage: CardLineageId,
    ) -> Option<CombatCard> {
        if let Some(c) = Self::remove_card_by_lineage(&mut self.zones.hand, lineage) {
            return Some(c);
        }
        if let Some(c) = Self::remove_card_by_lineage(&mut self.zones.limbo, lineage) {
            return Some(c);
        }
        if let Some(c) = self.zones.draw_pile.remove_by_lineage(lineage) {
            return Some(c);
        }
        if let Some(c) = self.zones.discard_pile.remove_by_lineage(lineage) {
            return Some(c);
        }
        if let Some(c) = Self::remove_card_by_lineage(&mut self.zones.exhaust_pile, lineage) {
            return Some(c);
        }
        None
    }
}

// Lightweight read helpers over combat-owned runtime state.
impl CombatState {
    /// Gets the current stack amount of a specific power on an entity
    pub fn get_power(&self, target: EntityId, power_id: PowerId) -> i32 {
        crate::content::powers::store::power_amount(self, target, power_id)
    }
}

// Queue-sensitive runtime helpers for Java cardQueue semantics.
impl CombatState {
    /// Best-effort approximation of Java's cardQueue membership for effects that
    /// should avoid already-queued cards (for example Mummified Hand).
    ///
    /// `zones.queued_cards` is the single Rust owner of Java
    /// `AbstractDungeon.actionManager.cardQueue`. Cards already in limbo or
    /// queued actions should not be treated as ordinary in-hand candidates.
    pub fn reserved_card_lineages_for_queue_sensitive_effects(&self) -> HashSet<CardLineageId> {
        let mut reserved = HashSet::new();
        for card in &self.zones.limbo {
            reserved.insert(card.lineage);
        }
        for queued in &self.zones.queued_cards {
            reserved.insert(queued.card.lineage);
        }
        for action in &self.engine.action_queue {
            match action {
                Action::EnqueueCardPlay { item, .. } => {
                    reserved.insert(item.card.lineage);
                }
                Action::PlayCardDirect { card, .. } => {
                    reserved.insert(card.lineage);
                }
                Action::UseCard { lineage, .. } => {
                    reserved.insert(*lineage);
                }
                _ => {}
            }
        }
        reserved
    }

    pub fn enqueue_card_play(&mut self, item: QueuedCardPlay, in_front: bool) {
        let was_empty = self.zones.queued_cards.is_empty();
        if in_front {
            self.zones.queued_cards.push_front(item);
        } else {
            self.zones.queued_cards.push_back(item);
        }
        if was_empty {
            self.queue_action_back(Action::FlushNextQueuedCard);
        }
    }

    pub fn colorless_combat_pool(&self) -> Vec<CardId> {
        if !self.runtime.colorless_combat_pool.is_empty() {
            self.runtime.colorless_combat_pool.clone()
        } else {
            crate::content::cards::random_colorless_in_combat_pool()
        }
    }
}

// Hand re-evaluation helpers used after state-changing effects.
impl CombatState {
    /// Reparses all cards in the hand to dynamically calculate damage, block, and magic numbers.
    /// Clones the hand to satisfy borrow-checker while allowing `PerfectedStrike` to read `&self.hand`.
    pub fn update_hand_cards(&mut self) {
        let mut new_hand = self.zones.hand.clone();
        for card in &mut new_hand {
            crate::content::cards::evaluate_card(card, self, None);
        }
        self.zones.hand = new_hand;
    }
}
