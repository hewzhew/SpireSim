use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub const MECHANICS: &str = "Exhaust all non-Attack cards remaining in hand, then deal 16 damage (22 if upgraded) to one enemy. Attack cards stay in hand.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::SeverSoul,
        name: "Sever Soul",
        card_type: CardType::Attack,
        rarity: CardRarity::Uncommon,
        cost: 2,
        base_damage: 16,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 6,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn sever_soul_play(
    _state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Sever Soul requires a valid target!");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, _state, Some(target));
    smallvec::smallvec![
        ActionInfo {
            action: Action::ExhaustAllNonAttack,
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::Damage(DamageInfo {
                source: 0,
                target,
                base: evaluated.base_damage_mut,
                output: evaluated.base_damage_mut,
                damage_type: DamageType::Normal,
                is_modified: false,
            }),
            insertion_mode: AddTo::Bottom,
        }
    ]
}
