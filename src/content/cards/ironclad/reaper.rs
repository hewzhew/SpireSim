use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTag, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: &str = "Deal 4 base Attack damage to all enemies (5 if upgraded), modified by Strength, Weak, Vulnerable and other damage effects. Heal for their total actual HP lost after Block, remaining HP and damage limits such as Invincible. Healing modifiers such as Magic Flower apply to that total. Exhaust; using it once removes this copy from later combat draws unless another effect returns it.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Reaper,
        name: "Reaper",
        card_type: CardType::Attack,
        rarity: CardRarity::Rare,
        cost: 2,
        base_damage: 4,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::AllEnemy,
        is_multi_damage: true,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[CardTag::Healing],
        upgrade_damage: 1,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn reaper_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::VampireDamageAllEnemies {
            source: 0,
            damages: evaluated.multi_damage.clone(),
            damage_type: DamageType::Normal,
        },
        insertion_mode: AddTo::Bottom,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::{store, PowerId};
    use crate::content::relics::{RelicId, RelicState};
    use crate::engine::action_handlers::execute_action;
    use crate::runtime::combat::{Power, PowerPayload};

    fn power(power_type: PowerId, amount: i32) -> Power {
        Power {
            power_type,
            instance_id: None,
            amount,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }
    }

    #[test]
    fn reaper_heal_respects_block_damage_allowance_and_magic_flower() {
        for allowance in [0, 2, 200] {
            for block in [0, 3] {
                let mut state = crate::test_support::blank_test_combat();
                state.entities.player.current_hp = 50;
                state
                    .entities
                    .player
                    .add_relic(RelicState::new(RelicId::MagicFlower));
                let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
                monster.current_hp = 40;
                monster.max_hp = 40;
                monster.block = block;
                let target = monster.id;
                state.entities.monsters = vec![monster];
                store::set_powers_for(&mut state, 0, vec![power(PowerId::Strength, 2)]);
                store::set_powers_for(
                    &mut state,
                    target,
                    vec![power(PowerId::Invincible, allowance)],
                );
                let card = CombatCard::for_scenario(CardId::Reaper, 1);
                for action in reaper_play(&state, &card) {
                    execute_action(action.action, &mut state);
                }
                while let Some(action) = state.pop_next_action() {
                    execute_action(action, &mut state);
                }
                let damage = (6 - block).min(allowance);
                assert_eq!(state.entities.monsters[0].current_hp, 40 - damage);
                assert_eq!(state.entities.player.current_hp, 50 + (damage * 3 + 1) / 2);
            }
        }
    }
}
