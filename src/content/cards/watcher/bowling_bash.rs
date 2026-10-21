use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::BowlingBash,
        name: "Bowling Bash",
        card_type: CardType::Attack,
        rarity: CardRarity::Common,
        cost: 1,
        base_damage: 7,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 3,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn bowling_bash_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("BowlingBash requires a valid target");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    let alive_count = state
        .entities
        .monsters
        .iter()
        .filter(|monster| !monster.is_dead_or_escaped())
        .count();
    let mut actions = SmallVec::new();
    for _ in 0..alive_count {
        actions.push(ActionInfo {
            action: Action::Damage(DamageInfo {
                source: 0,
                target,
                base: evaluated.base_damage_mut,
                output: evaluated.base_damage_mut,
                damage_type: DamageType::Normal,
                is_modified: true,
            }),
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
