use crate::content::monsters::exordium::{apply_power_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, DamageKind, DebuffSpec, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

pub struct SnakePlant;

const CHOMPY_CHOMPS: u8 = 1;
const SPORES: u8 = 2;
const STARTING_MALLEABLE: i32 = 3;

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![("Chompy Chomps", chomp_plan(asc)), ("Spores", spores_plan())],
        special_moves: vec![("Starting Malleable", format!("Starts with {STARTING_MALLEABLE} Malleable; see info power Malleable for its per-hit Block gain and reset."))],
        notes: if asc >= 17 {
            "Cannot use Chompy Chomps three times in a row. At A17+, Spores requires two intervening Chompy Chomps before it can be used again. Spores applies Frail then Weak separately; each can consume an Artifact charge. These rules describe possible moves, not a prediction of a hidden intent."
        } else {
            "Cannot use Chompy Chomps three times in a row or Spores twice in a row. Spores applies Frail then Weak separately; each can consume an Artifact charge. These rules describe possible moves, not a prediction of a hidden intent."
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::{EnemyId, MonsterBehavior};

    #[test]
    fn a17_spores_cooldown_checks_last_move_before_like_java() {
        let mut plant = crate::test_support::test_monster(EnemyId::SnakePlant);
        plant.move_history_mut().push_back(SPORES);
        plant.move_history_mut().push_back(CHOMPY_CHOMPS);

        let a17_plan =
            SnakePlant::roll_move_plan(&mut crate::runtime::rng::StsRng::new(0), &plant, 17, 65);
        let lower_asc_plan =
            SnakePlant::roll_move_plan(&mut crate::runtime::rng::StsRng::new(0), &plant, 16, 65);

        assert_eq!(
            a17_plan.move_id, CHOMPY_CHOMPS,
            "Java A17+ checks lastMoveBefore(SPORES) and blocks immediate Spores here"
        );
        assert_eq!(
            lower_asc_plan.move_id, SPORES,
            "Java below A17 only checks lastMove(SPORES)"
        );
    }

    #[test]
    fn chomp_queues_three_damage_actions_before_roll_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let plant = crate::test_support::test_monster(EnemyId::SnakePlant);

        let actions = SnakePlant::take_turn_plan(&mut state, &plant, &chomp_plan(0));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack { base_damage: 7, .. },
                Action::MonsterAttack { base_damage: 7, .. },
                Action::MonsterAttack { base_damage: 7, .. },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }
}

enum SnakePlantTurn<'a> {
    Chomp(&'a AttackSpec),
    Spores(&'a ApplyPowerStep, &'a ApplyPowerStep),
}

fn chomp_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        8
    } else {
        7
    }
}

fn chomp_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        CHOMPY_CHOMPS,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: chomp_damage(ascension_level),
            hits: 3,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn spores_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        SPORES,
        smallvec![
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Frail,
                amount: 2,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Strong,
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Weak,
                amount: 2,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Strong,
            }),
        ],
        MonsterMoveSpec::StrongDebuff(DebuffSpec {
            power_id: PowerId::Frail,
            amount: 2,
            strength: EffectStrength::Strong,
        }),
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        CHOMPY_CHOMPS => chomp_plan(ascension_level),
        SPORES => spores_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

fn last_move(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().back().copied() == Some(move_id)
}

fn last_move_before(entity: &MonsterEntity, move_id: u8) -> bool {
    let history = entity.move_history();
    history.len() >= 2 && history[history.len() - 2] == move_id
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let history = entity.move_history();
    history.len() >= 2
        && history[history.len() - 1] == move_id
        && history[history.len() - 2] == move_id
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> SnakePlantTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            CHOMPY_CHOMPS,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => SnakePlantTurn::Chomp(attack),
        (
            SPORES,
            [MoveStep::ApplyPower(
                frail @ ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Frail,
                    effect: PowerEffectKind::Debuff,
                    ..
                },
            ), MoveStep::ApplyPower(
                weak @ ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Weak,
                    effect: PowerEffectKind::Debuff,
                    ..
                },
            )],
        ) => SnakePlantTurn::Spores(frail, weak),
        (_, []) => panic!("snake plant plan missing locked truth"),
        (move_id, steps) => panic!("snake plant plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for SnakePlant {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_rng, _ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![Action::ApplyPower {
            source: entity.id,
            target: entity.id,
            power_id: PowerId::Malleable,
            amount: STARTING_MALLEABLE,
        }]
    }

    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if ascension_level >= 17 {
            if num < 65 {
                if last_two_moves(entity, CHOMPY_CHOMPS) {
                    spores_plan()
                } else {
                    chomp_plan(ascension_level)
                }
            } else if last_move(entity, SPORES) || last_move_before(entity, SPORES) {
                chomp_plan(ascension_level)
            } else {
                spores_plan()
            }
        } else if num < 65 {
            if last_two_moves(entity, CHOMPY_CHOMPS) {
                spores_plan()
            } else {
                chomp_plan(ascension_level)
            }
        } else if last_move(entity, SPORES) {
            chomp_plan(ascension_level)
        } else {
            spores_plan()
        }
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match decode_turn(plan) {
            SnakePlantTurn::Chomp(attack) => attack_actions(entity.id, PLAYER, attack),
            SnakePlantTurn::Spores(frail, weak) => vec![
                apply_power_action(entity, frail),
                apply_power_action(entity, weak),
            ],
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
