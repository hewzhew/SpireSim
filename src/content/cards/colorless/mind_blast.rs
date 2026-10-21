use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::MindBlast,
        name: "Mind Blast",
        card_type: CardType::Attack,
        rarity: CardRarity::Uncommon,
        cost: 2,
        base_damage: 0,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: true,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn mind_blast_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Mind Blast requires a valid target");
    let base = state.zones.draw_pile.len() as i32;
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    smallvec::smallvec![ActionInfo {
        action: Action::Damage(DamageInfo {
            source: 0,
            target,
            base,
            output: evaluated.base_damage_mut,
            damage_type: DamageType::Normal,
            is_modified: evaluated.base_damage_mut != base,
        }),
        insertion_mode: AddTo::Bottom,
    }]
}
