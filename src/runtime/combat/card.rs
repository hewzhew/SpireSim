use super::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
pub struct CombatCard {
    pub id: CardId,
    pub lineage: CardLineageId,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub upgrades: u8,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub misc_value: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_damage_override: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_block_override: Option<i32>,
    #[serde(default, skip_serializing_if = "is_zero_i8")]
    pub cost_modifier: i8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_for_turn: Option<u8>,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub base_damage_mut: i32,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub base_block_mut: i32,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub base_magic_num_mut: i32,
    #[serde(default, skip_serializing_if = "smallvec::SmallVec::is_empty")]
    pub multi_damage: smallvec::SmallVec<[i32; 5]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhaust_override: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retain_override: Option<bool>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub free_to_play_once: bool,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub energy_on_use: i32,
}

/// Identity-free recipe for a card instance that has not been created yet.
///
/// Queued generation actions carry this value until they resolve. Only the
/// owning combat allocator may turn a blueprint into a [`CombatCard`], so
/// production code never needs a fabricated scenario lineage as a placeholder.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CardBlueprint {
    pub id: CardId,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub upgrades: u8,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub misc_value: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_damage_override: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_block_override: Option<i32>,
    #[serde(default, skip_serializing_if = "is_zero_i8")]
    pub cost_modifier: i8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_for_turn: Option<u8>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub free_to_play_once: bool,
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

fn is_zero_i8(value: &i8) -> bool {
    *value == 0
}

fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl CombatCard {
    /// Builds a card belonging to an explicitly constructed scenario slot.
    ///
    /// Natural run and combat mechanics must call `with_lineage` with an ID
    /// obtained from their owning allocator.  The raw slot form remains only
    /// while existing hand-authored fixtures migrate to named recipe slots.
    pub fn for_scenario(id: CardId, constructed_recipe_slot: u32) -> Self {
        Self::with_lineage(
            id,
            CardLineageId::constructed_scenario(constructed_recipe_slot),
        )
    }

    pub fn with_lineage(id: CardId, lineage: CardLineageId) -> Self {
        let misc_value = match id {
            CardId::RitualDagger => 15,
            CardId::GeneticAlgorithm => 1,
            _ => 0,
        };
        Self {
            id,
            lineage,
            upgrades: 0,
            misc_value,
            base_damage_override: None,
            base_block_override: None,
            cost_modifier: 0,
            cost_for_turn: None,
            base_damage_mut: 0,
            base_block_mut: 0,
            base_magic_num_mut: 0,
            multi_damage: smallvec::smallvec![],
            exhaust_override: None,
            retain_override: None,
            free_to_play_once: false,
            energy_on_use: 0,
        }
    }

    /// Java `AbstractCard.makeStatEquivalentCopy()` preserves card identity
    /// state such as upgrades, misc, base damage mutation, cost-for-turn, and
    /// free-to-play state, but it does not preserve transient calculated
    /// damage/block/magic/multi-damage or queued play metadata.
    pub fn make_stat_equivalent_copy_with_lineage(&self, lineage: CardLineageId) -> Self {
        let mut card = self.clone();
        card.lineage = lineage;
        card.base_damage_mut = 0;
        card.base_block_mut = 0;
        card.base_magic_num_mut = 0;
        card.multi_damage.clear();
        card.exhaust_override = None;
        card.retain_override = None;
        card.energy_on_use = 0;
        card
    }

    /// Java `AbstractCard.resetAttributes()` restores transient rendered values
    /// and resets `costForTurn` back to the combat cost. It does not clear
    /// persistent combat cost changes, `freeToPlayOnce`, or card-placement
    /// flags such as one-turn Retain.
    pub fn reset_attributes_java(&mut self) {
        self.base_damage_mut = 0;
        self.base_block_mut = 0;
        self.base_magic_num_mut = 0;
        self.multi_damage.clear();
        self.cost_for_turn = None;
    }

    /// Java `AbstractCard.makeSameInstanceOf()` is a stat-equivalent copy with
    /// the original UUID restored. Replay effects such as Double Tap, Burst,
    /// Duplication Potion, and Necronomicon use this path.
    pub fn make_same_instance_of_java(&self) -> Self {
        self.make_stat_equivalent_copy_with_lineage(self.lineage)
    }

    pub fn get_cost(&self) -> i8 {
        if let Some(c) = self.cost_for_turn {
            c as i8
        } else {
            self.combat_cost_without_turn_override_java()
                .clamp(i8::MIN as i32, i8::MAX as i32) as i8
        }
    }

    pub fn base_cost_for_combat_java(&self) -> i32 {
        let def = crate::content::cards::get_card_definition(self.id);
        crate::content::cards::upgraded_base_cost_override(self).unwrap_or(def.cost) as i32
    }

    /// Java `AbstractCard.cost`: the combat copy's actual cost after
    /// cost-modifying effects, before `costForTurn` overrides.
    pub fn combat_cost_without_turn_override_java(&self) -> i32 {
        let base_cost = self.base_cost_for_combat_java();
        if base_cost < 0 {
            return base_cost;
        }
        (base_cost + self.cost_modifier as i32).max(0)
    }

    /// Java `AbstractCard.costForTurn`: the visible playable cost for this
    /// turn, falling back to the combat cost when no temporary override exists
    /// in Rust.
    pub fn cost_for_turn_java(&self) -> i32 {
        self.cost_for_turn
            .map(i32::from)
            .unwrap_or_else(|| self.combat_cost_without_turn_override_java())
    }

    pub fn set_cost_for_turn_java(&mut self, amount: i32) {
        if self.cost_for_turn_java() >= 0 {
            self.cost_for_turn = Some(clamp_turn_cost(amount));
        }
    }

    /// Mirrors Java `AbstractCard.updateCost(int)`: adjust combat cost and
    /// preserve any existing difference between `cost` and `costForTurn`.
    pub fn update_cost_java(&mut self, amount: i32) {
        let def = crate::content::cards::get_card_definition(self.id);
        if (def.card_type == crate::content::cards::CardType::Status && self.id != CardId::Slimed)
            || (def.card_type == crate::content::cards::CardType::Curse && self.id != CardId::Pride)
        {
            return;
        }

        let old_cost = self.combat_cost_without_turn_override_java();
        if old_cost < 0 {
            return;
        }
        let old_cost_for_turn = self.cost_for_turn_java();
        let cost_for_turn_diff = old_cost - old_cost_for_turn;
        let new_cost = (old_cost + amount).max(0);
        if new_cost == old_cost {
            return;
        }

        self.set_combat_cost_without_turn_override_java(new_cost);
        if self.cost_for_turn.is_some() || cost_for_turn_diff != 0 {
            self.cost_for_turn = Some(clamp_turn_cost(new_cost - cost_for_turn_diff));
        }
    }

    /// Mirrors Java `AbstractCard.modifyCostForCombat(int)`, used by effects
    /// such as Madness that mutate this combat copy's cost and visible
    /// cost-for-turn together.
    pub fn modify_cost_for_combat_java(&mut self, amount: i32) {
        let old_cost = self.combat_cost_without_turn_override_java();
        if old_cost < 0 {
            return;
        }

        let old_cost_for_turn = self.cost_for_turn_java();
        if old_cost_for_turn > 0 {
            let new_cost = (old_cost_for_turn + amount).max(0);
            self.set_combat_cost_without_turn_override_java(new_cost);
            self.cost_for_turn = Some(clamp_turn_cost(new_cost));
        } else {
            let new_cost = (old_cost + amount).max(0);
            self.set_combat_cost_without_turn_override_java(new_cost);
            self.cost_for_turn = Some(0);
        }
    }

    pub fn set_combat_cost_preserving_turn_java(&mut self, new_cost: i32) {
        if self.base_cost_for_combat_java() >= 0 {
            self.set_combat_cost_without_turn_override_java(new_cost.max(0));
        }
    }

    pub fn set_combat_and_turn_cost_java(&mut self, new_cost: i32) {
        if self.base_cost_for_combat_java() >= 0 {
            let new_cost = new_cost.max(0);
            self.set_combat_cost_without_turn_override_java(new_cost);
            self.cost_for_turn = Some(clamp_turn_cost(new_cost));
        }
    }

    fn set_combat_cost_without_turn_override_java(&mut self, new_cost: i32) {
        let base_cost = self.base_cost_for_combat_java();
        if base_cost < 0 {
            return;
        }
        self.cost_modifier = clamp_cost_modifier(new_cost - base_cost);
    }
}

impl CardBlueprint {
    pub fn fresh(id: CardId) -> Self {
        let misc_value = match id {
            CardId::RitualDagger => 15,
            CardId::GeneticAlgorithm => 1,
            _ => 0,
        };
        Self {
            id,
            upgrades: 0,
            misc_value,
            base_damage_override: None,
            base_block_override: None,
            cost_modifier: 0,
            cost_for_turn: None,
            free_to_play_once: false,
        }
    }

    /// Captures exactly the state Java's `makeStatEquivalentCopy()` preserves,
    /// excluding identity because the future copy does not exist yet.
    pub fn from_stat_equivalent(card: &CombatCard) -> Self {
        Self {
            id: card.id,
            upgrades: card.upgrades,
            misc_value: card.misc_value,
            base_damage_override: card.base_damage_override,
            base_block_override: card.base_block_override,
            cost_modifier: card.cost_modifier,
            cost_for_turn: card.cost_for_turn,
            free_to_play_once: card.free_to_play_once,
        }
    }

    /// Creates the promised card at the allocator-owned identity boundary.
    pub fn materialize(&self, lineage: CardLineageId) -> CombatCard {
        let mut card = CombatCard::with_lineage(self.id, lineage);
        card.upgrades = self.upgrades;
        card.misc_value = self.misc_value;
        card.base_damage_override = self.base_damage_override;
        card.base_block_override = self.base_block_override;
        card.cost_modifier = self.cost_modifier;
        card.cost_for_turn = self.cost_for_turn;
        card.free_to_play_once = self.free_to_play_once;
        card
    }

    pub fn combat_cost_without_turn_override_java(&self) -> i32 {
        let definition = crate::content::cards::get_card_definition(self.id);
        let base_cost =
            crate::content::cards::upgraded_base_cost_override_for(self.id, self.upgrades)
                .unwrap_or(definition.cost) as i32;
        if base_cost < 0 {
            return base_cost;
        }
        (base_cost + i32::from(self.cost_modifier)).max(0)
    }

    pub fn set_cost_for_turn_java(&mut self, amount: i32) {
        if self
            .cost_for_turn
            .map(i32::from)
            .unwrap_or_else(|| self.combat_cost_without_turn_override_java())
            >= 0
        {
            self.cost_for_turn = Some(clamp_turn_cost(amount));
        }
    }

    pub fn cost_for_turn_java(&self) -> i32 {
        self.cost_for_turn
            .map(i32::from)
            .unwrap_or_else(|| self.combat_cost_without_turn_override_java())
    }

    pub fn update_cost_java(&mut self, amount: i32) {
        let old_cost = self.combat_cost_without_turn_override_java();
        let old_turn_cost = self.cost_for_turn.map(i32::from).unwrap_or(old_cost);
        if old_cost < 0 {
            return;
        }
        let new_cost = (old_cost + amount).max(0);
        let turn_delta = old_turn_cost - old_cost;
        let base_cost =
            crate::content::cards::upgraded_base_cost_override_for(self.id, self.upgrades)
                .unwrap_or_else(|| crate::content::cards::get_card_definition(self.id).cost)
                as i32;
        self.cost_modifier = (new_cost - base_cost).clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        if self.cost_for_turn.is_some() {
            self.cost_for_turn = Some(clamp_turn_cost(new_cost + turn_delta));
        }
    }
}

fn clamp_turn_cost(cost: i32) -> u8 {
    cost.clamp(0, u8::MAX as i32) as u8
}

fn clamp_cost_modifier(modifier: i32) -> i8 {
    modifier.clamp(i8::MIN as i32, i8::MAX as i32) as i8
}
