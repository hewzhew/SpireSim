use super::*;
use crate::content::cards::{self, CardTarget};
use crate::content::potions::PotionId;

#[derive(Clone, Debug)]
pub(super) struct TargetFanoutCandidate {
    pub(super) kind: TargetFanoutKind,
    pub(super) source_key: String,
    pub(super) target_hp_with_block: i32,
    pub(super) damage_hint: i32,
    pub(super) lethal: bool,
    pub(super) action_key: String,
}

pub(super) fn target_fanout_candidate(
    combat: &CombatState,
    choice: &CombatActionChoice,
) -> Option<TargetFanoutCandidate> {
    match &choice.input {
        ClientInput::PlayCard {
            card_index,
            target: Some(target),
        } => card_target_candidate(combat, choice, *card_index, *target),
        ClientInput::UsePotion {
            potion_index,
            target: Some(target),
        } => potion_target_candidate(combat, choice, *potion_index, *target),
        _ => None,
    }
}

fn card_target_candidate(
    combat: &CombatState,
    choice: &CombatActionChoice,
    card_index: usize,
    target: usize,
) -> Option<TargetFanoutCandidate> {
    let card = combat.zones.hand.get(card_index)?;
    let target_hp_with_block = monster_hp_with_block(combat, target)?;
    let target_kind = cards::effective_target(card);
    if !matches!(target_kind, CardTarget::Enemy | CardTarget::SelfAndEnemy) {
        return None;
    }
    let evaluated = cards::evaluate_card_for_play(card, combat, Some(target));
    let damage_hint = evaluated.base_damage_mut.max(0);
    Some(TargetFanoutCandidate {
        kind: TargetFanoutKind::PlayCard,
        source_key: format!(
            "play_card/hand:{card_index}/card:{}+{}/lineage:{}/cost:{}",
            card.id.stable_id(),
            card.upgrades,
            card.lineage,
            card.cost_for_turn_java()
        ),
        target_hp_with_block,
        damage_hint,
        lethal: damage_hint >= target_hp_with_block && damage_hint > 0,
        action_key: choice.action_key.clone(),
    })
}

fn potion_target_candidate(
    combat: &CombatState,
    choice: &CombatActionChoice,
    potion_index: usize,
    target: usize,
) -> Option<TargetFanoutCandidate> {
    let potion = combat.entities.potions.get(potion_index)?.as_ref()?;
    let target_hp_with_block = monster_hp_with_block(combat, target)?;
    let damage_hint = potion_single_target_damage_hint(combat, potion.id);
    Some(TargetFanoutCandidate {
        kind: TargetFanoutKind::UsePotion,
        source_key: format!(
            "use_potion/slot:{potion_index}/potion:{:?}/uuid:{}",
            potion.id, potion.uuid
        ),
        target_hp_with_block,
        damage_hint,
        lethal: damage_hint >= target_hp_with_block && damage_hint > 0,
        action_key: choice.action_key.clone(),
    })
}

fn potion_single_target_damage_hint(combat: &CombatState, id: PotionId) -> i32 {
    match id {
        PotionId::FirePotion => potion_potency(combat, id),
        _ => 0,
    }
}

fn potion_potency(combat: &CombatState, id: PotionId) -> i32 {
    let mut potency = crate::content::potions::get_potion_definition(id).base_potency;
    if combat
        .entities
        .player
        .has_relic(crate::content::relics::RelicId::SacredBark)
    {
        potency *= 2;
    }
    potency
}

fn monster_hp_with_block(combat: &CombatState, entity_id: usize) -> Option<i32> {
    combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == entity_id && monster.is_alive_for_action())
        .map(|monster| monster.current_hp + monster.block)
}
