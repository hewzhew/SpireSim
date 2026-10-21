use crate::content::cards::{self, CardId, CardType};
use crate::runtime::combat::CombatCard;

const UNDESIRABLE_CARD_KEEP_VALUE: i32 = -1_000;
const UNDESIRABLE_CARD_REMOVAL_VALUE: i32 = 1_000;
const ATTACK_BASE_KEEP_VALUE: i32 = 300;
const SKILL_BASE_KEEP_VALUE: i32 = 275;
const POWER_BASE_KEEP_VALUE: i32 = 325;
const DAMAGE_KEEP_VALUE_FACTOR: i32 = 4;
const BLOCK_KEEP_VALUE_FACTOR: i32 = 4;
const MAGIC_KEEP_VALUE_FACTOR: i32 = 2;
const POWER_MAGIC_KEEP_VALUE_FACTOR: i32 = 3;
const COST_KEEP_VALUE_PENALTY: i32 = 10;
const UPGRADE_MAGIC_VALUE_FACTOR: i32 = 8;
const COST_UPGRADE_VALUE_FACTOR: i32 = 18;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CardSelectionFacts {
    pub(super) keep_value: i32,
    pub(super) removal_value: i32,
    pub(super) upgrade_value: i32,
}

pub(super) fn aggregate_card_facts(
    facts: impl Iterator<Item = CardSelectionFacts>,
) -> CardSelectionFacts {
    facts.fold(CardSelectionFacts::default(), |mut acc, fact| {
        acc.keep_value = acc.keep_value.saturating_add(fact.keep_value);
        acc.removal_value = acc.removal_value.saturating_add(fact.removal_value);
        acc.upgrade_value = acc.upgrade_value.saturating_add(fact.upgrade_value);
        acc
    })
}

impl CardSelectionFacts {
    pub(super) fn from_card(card: &CombatCard) -> Self {
        let def = cards::get_card_definition(card.id);
        let damage = card
            .base_damage_override
            .unwrap_or(def.base_damage + def.upgrade_damage * card.upgrades as i32)
            .max(0);
        let block = card
            .base_block_override
            .unwrap_or(def.base_block + def.upgrade_block * card.upgrades as i32)
            .max(0);
        let magic = (def.base_magic + def.upgrade_magic * card.upgrades as i32).max(0);
        let cost = card.cost_for_turn_java().max(0);
        let keep_value = keep_value_from_parts(def.card_type, damage, block, magic, cost);
        Self {
            keep_value,
            removal_value: removal_value_for_card_type(def.card_type),
            upgrade_value: upgrade_value_for_card(card, keep_value),
        }
    }

    pub(super) fn from_card_id(card_id: CardId) -> Self {
        let def = cards::get_card_definition(card_id);
        let keep_value = keep_value_from_parts(
            def.card_type,
            def.base_damage.max(0),
            def.base_block.max(0),
            def.base_magic.max(0),
            (def.cost as i32).max(0),
        );
        Self {
            keep_value,
            removal_value: removal_value_for_card_type(def.card_type),
            upgrade_value: upgrade_value_for_card_id(card_id, keep_value),
        }
    }
}

fn upgrade_value_for_card(card: &CombatCard, keep_value_before: i32) -> i32 {
    if !cards::can_upgrade_card_once(card) {
        return 0;
    }
    let damage_before = card_damage_value(card);
    let block_before = card_block_value(card);
    let magic_before = card_magic_value(card);
    let cost_before = card.cost_for_turn_java().max(0);
    let mut upgraded = card.clone();
    if !cards::upgrade_card_once_java(&mut upgraded) {
        return 0;
    }
    let damage_delta = card_damage_value(&upgraded).saturating_sub(damage_before);
    let block_delta = card_block_value(&upgraded).saturating_sub(block_before);
    let magic_delta = card_magic_value(&upgraded).saturating_sub(magic_before);
    let cost_delta = cost_before.saturating_sub(upgraded.cost_for_turn_java().max(0));
    damage_delta
        .saturating_mul(DAMAGE_KEEP_VALUE_FACTOR)
        .saturating_add(block_delta.saturating_mul(BLOCK_KEEP_VALUE_FACTOR))
        .saturating_add(magic_delta.saturating_mul(UPGRADE_MAGIC_VALUE_FACTOR))
        .saturating_add(cost_delta.saturating_mul(COST_UPGRADE_VALUE_FACTOR))
        .max(keep_value_for_upgrade_tiebreak(keep_value_before))
}

fn upgrade_value_for_card_id(card_id: CardId, keep_value_before: i32) -> i32 {
    if !cards::can_upgrade_card_once_for(card_id, 0) {
        return 0;
    }
    let definition = cards::get_card_definition(card_id);
    let cost_before = cards::upgraded_base_cost_override_for(card_id, 0)
        .unwrap_or(definition.cost)
        .max(0) as i32;
    let cost_after = cards::upgraded_base_cost_override_for(card_id, 1)
        .unwrap_or(definition.cost)
        .max(0) as i32;
    definition
        .upgrade_damage
        .max(0)
        .saturating_mul(DAMAGE_KEEP_VALUE_FACTOR)
        .saturating_add(
            definition
                .upgrade_block
                .max(0)
                .saturating_mul(BLOCK_KEEP_VALUE_FACTOR),
        )
        .saturating_add(
            definition
                .upgrade_magic
                .max(0)
                .saturating_mul(UPGRADE_MAGIC_VALUE_FACTOR),
        )
        .saturating_add(
            cost_before
                .saturating_sub(cost_after)
                .saturating_mul(COST_UPGRADE_VALUE_FACTOR),
        )
        .max(keep_value_for_upgrade_tiebreak(keep_value_before))
}

fn keep_value_for_upgrade_tiebreak(keep_value: i32) -> i32 {
    if keep_value <= 0 {
        0
    } else {
        keep_value / 100
    }
}

fn card_damage_value(card: &CombatCard) -> i32 {
    let def = cards::get_card_definition(card.id);
    card.base_damage_override
        .unwrap_or(def.base_damage + def.upgrade_damage * card.upgrades as i32)
        .max(0)
}

fn card_block_value(card: &CombatCard) -> i32 {
    let def = cards::get_card_definition(card.id);
    card.base_block_override
        .unwrap_or(def.base_block + def.upgrade_block * card.upgrades as i32)
        .max(0)
}

fn card_magic_value(card: &CombatCard) -> i32 {
    let def = cards::get_card_definition(card.id);
    (def.base_magic + def.upgrade_magic * card.upgrades as i32).max(0)
}

fn keep_value_from_parts(
    card_type: CardType,
    damage: i32,
    block: i32,
    magic: i32,
    cost: i32,
) -> i32 {
    match card_type {
        CardType::Status | CardType::Curse => UNDESIRABLE_CARD_KEEP_VALUE,
        CardType::Attack => {
            ATTACK_BASE_KEEP_VALUE + damage.saturating_mul(DAMAGE_KEEP_VALUE_FACTOR)
                - cost.saturating_mul(COST_KEEP_VALUE_PENALTY)
        }
        CardType::Skill => {
            SKILL_BASE_KEEP_VALUE
                + block.saturating_mul(BLOCK_KEEP_VALUE_FACTOR)
                + magic.saturating_mul(MAGIC_KEEP_VALUE_FACTOR)
                - cost.saturating_mul(COST_KEEP_VALUE_PENALTY)
        }
        CardType::Power => {
            POWER_BASE_KEEP_VALUE + magic.saturating_mul(POWER_MAGIC_KEEP_VALUE_FACTOR)
                - cost.saturating_mul(COST_KEEP_VALUE_PENALTY)
        }
    }
}

fn removal_value_for_card_type(card_type: CardType) -> i32 {
    match card_type {
        CardType::Status | CardType::Curse => UNDESIRABLE_CARD_REMOVAL_VALUE,
        CardType::Attack | CardType::Skill | CardType::Power => 0,
    }
}
