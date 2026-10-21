use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState, OrbId};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::DoomAndGloom,
        name: "Doom and Gloom",
        card_type: CardType::Attack,
        rarity: CardRarity::Uncommon,
        cost: 2,
        base_damage: 10,
        base_block: 0,
        base_magic: 1,
        target: CardTarget::AllEnemy,
        is_multi_damage: true,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 4,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn doom_and_gloom_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![
        ActionInfo {
            action: Action::DamageAllEnemies {
                source: 0,
                damages: evaluated.multi_damage.clone(),
                damage_type: DamageType::Normal,
                is_modified: false,
            },
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::ChannelOrb(OrbId::Dark),
            insertion_mode: AddTo::Bottom,
        },
    ]
}
