use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub const MECHANICS: &str = "Deal 18 damage (22 if upgraded). Base cost is 4 (3 if upgraded). Each positive HP-loss event reduces this instance's cost by 1 while it is in hand, draw or discard, to a minimum of 0, for this combat. Blocked or prevented HP loss does not reduce cost.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::BloodForBlood,
        name: "Blood for Blood",
        card_type: CardType::Attack,
        rarity: CardRarity::Uncommon,
        cost: 4,
        base_damage: 18,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 4,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn blood_for_blood_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Blood for Blood requires a valid target!");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    smallvec::smallvec![ActionInfo {
        action: Action::Damage(DamageInfo {
            source: 0,
            target,
            base: evaluated.base_damage_mut,
            output: evaluated.base_damage_mut,
            damage_type: DamageType::Normal,
            is_modified: true,
        }),
        insertion_mode: AddTo::Bottom,
    }]
}
