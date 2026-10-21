use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub const MECHANICS: &str = "Deal damage equal to this copy's stored misc value, initially 15. If this hit kills a non-Minion enemy that is not entering a half-dead phase, permanently increase this copy's stored damage by 3 (5 if upgraded). The increase follows its lineage into the master deck and matching combat copies. Costs 1 Energy. Exhaust. Upgrading increases kill growth, not the current damage.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::RitualDagger,
        name: "Ritual Dagger",
        card_type: CardType::Attack,
        rarity: CardRarity::Special,
        cost: 1,
        base_damage: 15,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 2,
    }
}

pub fn ritual_dagger_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Ritual Dagger requires a valid target");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    smallvec::smallvec![ActionInfo {
        action: Action::RitualDagger {
            target,
            damage_info: DamageInfo {
                source: 0,
                target,
                base: evaluated.base_damage_mut,
                output: evaluated.base_damage_mut,
                damage_type: DamageType::Normal,
                is_modified: evaluated.base_damage_mut
                    != crate::content::cards::get_card_definition(card.id).base_damage,
            },
            misc_amount: evaluated.base_magic_num_mut,
            card_lineage: card.lineage,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
