use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState, OrbId};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Chill,
        name: "Chill",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 1,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn chill_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let alive_monsters = state
        .entities
        .monsters
        .iter()
        .filter(|monster| !monster.is_dead_or_escaped())
        .count() as i32;
    let amount = alive_monsters * evaluated.base_magic_num_mut.max(0);

    let mut actions = SmallVec::new();
    for _ in 0..amount {
        actions.push(ActionInfo {
            action: Action::ChannelOrb(OrbId::Frost),
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
