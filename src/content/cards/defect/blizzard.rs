use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState, OrbId};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Blizzard,
        name: "Blizzard",
        card_type: CardType::Attack,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 2,
        target: CardTarget::AllEnemy,
        is_multi_damage: true,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn blizzard_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let frost_count = state
        .turn
        .counters
        .orbs_channeled_this_combat
        .iter()
        .filter(|&&orb| orb == OrbId::Frost)
        .count() as i32;
    let mut dynamic_card = card.clone();
    dynamic_card.base_damage_override = Some(frost_count * evaluated.base_magic_num_mut);
    let evaluated = crate::content::cards::evaluate_card_for_play(&dynamic_card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::DamageAllEnemies {
            source: 0,
            damages: evaluated.multi_damage.clone(),
            damage_type: DamageType::Normal,
            is_modified: false,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
