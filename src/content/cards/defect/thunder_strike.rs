use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTag, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState, OrbId};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::ThunderStrike,
        name: "Thunder Strike",
        card_type: CardType::Attack,
        rarity: CardRarity::Rare,
        cost: 3,
        base_damage: 7,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::AllEnemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[CardTag::Strike],
        upgrade_damage: 2,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn thunder_strike_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let lightning_count = state
        .turn
        .counters
        .orbs_channeled_this_combat
        .iter()
        .filter(|&&orb| orb == OrbId::Lightning)
        .count();
    let mut actions = SmallVec::new();
    for _ in 0..lightning_count {
        actions.push(ActionInfo {
            action: Action::AttackDamageRandomEnemyCard {
                card: Box::new(card.clone()),
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
