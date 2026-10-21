//! Shared validated loadouts for explicitly constructed first-act studies.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::{RunControlConfig, RunControlSession};
use crate::content::cards::{upgrade_card_once_java, CardId};
use crate::content::monsters::factory::EncounterId;
use crate::content::potions::{Potion, PotionId};
use crate::content::relics::{RelicId, RelicState};
use crate::runtime::combat::CombatCard;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedStrategicCardV1 {
    pub id: CardId,
    #[serde(default)]
    pub upgrades: u8,
    pub misc_value: Option<i32>,
    pub base_damage_override: Option<i32>,
    pub base_block_override: Option<i32>,
    #[serde(default)]
    pub cost_modifier: i8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedFirstActLoadoutV1 {
    pub seed: u64,
    pub ascension_level: u8,
    pub current_hp: i32,
    pub max_hp: i32,
    #[serde(default)]
    pub gold: i32,
    pub boss: EncounterId,
    pub cards: Vec<ConstructedStrategicCardV1>,
    pub relics: Vec<RelicId>,
    #[serde(default)]
    pub potions: Vec<Option<PotionId>>,
}

pub(super) fn constructed_first_act_session_v1(
    recipe: &ConstructedFirstActLoadoutV1,
) -> Result<RunControlSession, String> {
    if recipe.ascension_level > 20
        || !(1..=999).contains(&recipe.max_hp)
        || !(1..=recipe.max_hp).contains(&recipe.current_hp)
        || !(1..=128).contains(&recipe.cards.len())
        || recipe.relics.len() > 64
        || !(0..=1_000_000).contains(&recipe.gold)
    {
        return Err("constructed loadout exceeds HP/ascension/gold/card/relic bounds".into());
    }
    if !matches!(
        recipe.boss,
        EncounterId::Hexaghost | EncounterId::SlimeBoss | EncounterId::TheGuardian
    ) {
        return Err("constructed loadout requires a first-act boss".into());
    }
    let mut seen = BTreeSet::new();
    if recipe
        .relics
        .iter()
        .any(|relic| !seen.insert(relic.stable_ordinal()))
    {
        return Err("constructed loadout has duplicate relics".into());
    }
    let mut cards = Vec::with_capacity(recipe.cards.len());
    for (slot, item) in recipe.cards.iter().enumerate() {
        let mut card = CombatCard::for_scenario(item.id, slot as u32);
        apply_constructed_upgrades_v1(&mut card, item.upgrades)?;
        if let Some(value) = item.misc_value {
            card.misc_value = value;
        }
        card.base_damage_override = item.base_damage_override;
        card.base_block_override = item.base_block_override;
        card.cost_modifier = item.cost_modifier;
        cards.push(card);
    }
    let mut session = RunControlSession::new(RunControlConfig {
        seed: recipe.seed,
        ascension_level: recipe.ascension_level,
        final_act: true,
        ..RunControlConfig::default()
    });
    let run = &mut session.run_state;
    run.act_num = 1;
    run.current_hp = recipe.current_hp;
    run.max_hp = recipe.max_hp;
    run.gold = recipe.gold;
    run.master_deck = cards;
    run.relics = recipe
        .relics
        .iter()
        .map(|id| RelicState::new(*id))
        .collect();
    run.potions.fill(None);
    if recipe.relics.contains(&RelicId::PotionBelt) {
        crate::content::relics::potion_belt::on_equip(run);
    }
    if recipe.potions.len() > run.potions.len() {
        return Err("constructed potion inventory exceeds actual slot capacity".into());
    }
    for (slot, id) in recipe.potions.iter().enumerate() {
        run.potions[slot] = id.map(|id| Potion::new(id, slot as u32));
    }
    run.event_state = None;
    run.reward_state = None;
    run.shop_state = None;
    run.boss_key = Some(recipe.boss);
    Ok(session)
}

pub(super) fn apply_constructed_upgrades_v1(
    card: &mut CombatCard,
    upgrades: u8,
) -> Result<(), String> {
    if upgrades > 32 {
        return Err("constructed card upgrades exceed bound".into());
    }
    for _ in 0..upgrades {
        if !upgrade_card_once_java(card) {
            return Err(format!("constructed card cannot upgrade: {:?}", card.id));
        }
    }
    Ok(())
}
