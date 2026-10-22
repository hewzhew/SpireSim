use crate::content::cards::CardId;
use crate::content::monsters::exordium::{
    add_card_action, attack_actions, set_next_move_action, spawn_action, PLAYER,
};
use crate::content::monsters::{EnemyId, MonsterBehavior};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AddCardStep, AttackSpec, AttackStep, CardDestination, DamageKind, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, SpawnHpSpec, SpawnHpValue,
    SpawnMonsterStep,
};

const SLAM: u8 = 1;
const PREP_SLAM: u8 = 2;
const SPLIT: u8 = 3;
const STICKY: u8 = 4;

pub struct SlimeBoss;

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    entity.slime_boss.protocol_seeded = true;
    entity.slime_boss.first_turn = true;
}

enum SlimeBossTurn<'a> {
    Sticky(&'a AddCardStep),
    PrepSlam,
    Slam(&'a AttackSpec),
    Split(&'a [MoveStep]),
}

fn slam_damage(asc: u8) -> i32 {
    if asc >= 4 {
        38
    } else {
        35
    }
}

fn sticky_count(asc: u8) -> u8 {
    if asc >= 19 {
        5
    } else {
        3
    }
}

fn sticky_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        STICKY,
        MoveStep::AddCard(AddCardStep {
            card_id: CardId::Slimed,
            amount: sticky_count(asc),
            upgraded: false,
            destination: CardDestination::Discard,
            visible_strength: EffectStrength::Strong,
        }),
    )
}

fn prep_slam_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        PREP_SLAM,
        smallvec::smallvec![MoveStep::Magic],
        MonsterMoveSpec::Unknown,
    )
}

fn slam_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        SLAM,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: slam_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

pub(crate) fn split_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        SPLIT,
        smallvec::smallvec![
            MoveStep::Suicide,
            MoveStep::SpawnMonster(SpawnMonsterStep {
                enemy_id: EnemyId::SpikeSlimeL,
                logical_position_offset: -1,
                protocol_draw_x_offset: Some(-385),
                hp: SpawnHpSpec {
                    current: SpawnHpValue::SourceCurrentHp,
                    max: SpawnHpValue::SourceCurrentHp,
                },
                is_minion: false,
            }),
            MoveStep::SpawnMonster(SpawnMonsterStep {
                enemy_id: EnemyId::AcidSlimeL,
                logical_position_offset: 1,
                protocol_draw_x_offset: Some(120),
                hp: SpawnHpSpec {
                    current: SpawnHpValue::SourceCurrentHp,
                    max: SpawnHpValue::SourceCurrentHp,
                },
                is_minion: false,
            }),
        ],
        MonsterMoveSpec::Unknown,
    )
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        STICKY => sticky_plan(asc),
        PREP_SLAM => prep_slam_plan(),
        SLAM => slam_plan(asc),
        SPLIT => split_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            (Text::new("slime_boss.goopSpray"), sticky_plan(asc)),
            (Text::new("slime_boss.slam"), slam_plan(asc)),
            (Text::new("slime_boss.split"), split_plan()),
        ],
        special_moves: vec![(
            Text::new("slime_boss.preparing"),
            Text::new("slime_boss.preparingRule"),
        )],
        notes: Text::new("slime_boss.notes1"),
    }
}

fn runtime(entity: &MonsterEntity) -> bool {
    assert!(
        entity.slime_boss.protocol_seeded,
        "slime boss runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.slime_boss.first_turn
}

fn slime_boss_runtime_update(entity: &MonsterEntity, first_turn: Option<bool>) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::SlimeBoss {
            first_turn,
            protocol_seeded: Some(true),
        },
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> SlimeBossTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (STICKY, [MoveStep::AddCard(add_card)])
            if add_card.card_id == CardId::Slimed
                && add_card.destination == CardDestination::Discard =>
        {
            SlimeBossTurn::Sticky(add_card)
        }
        (PREP_SLAM, [MoveStep::Magic]) => SlimeBossTurn::PrepSlam,
        (
            SLAM,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => SlimeBossTurn::Slam(attack),
        (SPLIT, steps)
            if steps
                .iter()
                .all(|step| matches!(step, MoveStep::Suicide | MoveStep::SpawnMonster(_))) =>
        {
            SlimeBossTurn::Split(steps)
        }
        (_, []) => panic!("slime boss plan missing locked truth"),
        (move_id, steps) => panic!("slime boss plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for SlimeBoss {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        if runtime(entity) {
            sticky_plan(ascension_level)
        } else {
            plan_for(entity.planned_move_id(), ascension_level)
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if runtime(entity) {
            vec![slime_boss_runtime_update(entity, Some(false))]
        } else {
            Vec::new()
        }
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_hp_rng, _ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![Action::ApplyPower {
            source: entity.id,
            target: entity.id,
            power_id: PowerId::Split,
            amount: -1,
        }]
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match decode_turn(plan) {
            SlimeBossTurn::Sticky(add_card) => vec![
                add_card_action(add_card),
                set_next_move_action(entity, prep_slam_plan()),
            ],
            SlimeBossTurn::PrepSlam => {
                vec![set_next_move_action(
                    entity,
                    slam_plan(state.meta.ascension_level),
                )]
            }
            SlimeBossTurn::Slam(attack) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(set_next_move_action(
                    entity,
                    sticky_plan(state.meta.ascension_level),
                ));
                actions
            }
            SlimeBossTurn::Split(steps) => {
                let mut actions = vec![set_next_move_action(entity, plan.clone())];
                for step in steps {
                    match step {
                        MoveStep::Suicide => actions.push(Action::Suicide {
                            target: entity.id,
                            trigger_relics: false,
                        }),
                        MoveStep::SpawnMonster(step) => {
                            actions.push(spawn_action(state, entity, step))
                        }
                        other => panic!("slime boss split step unsupported: {:?}", other),
                    }
                }
                actions
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{prep_slam_plan, SlimeBoss, PREP_SLAM, SLAM, STICKY};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::action::{Action, MonsterRuntimePatch};
    use crate::runtime::monster_move::MonsterMoveSpec;

    #[test]
    fn prep_slam_keeps_magic_execution_separate_from_unknown_visible_intent() {
        let plan = prep_slam_plan();
        assert_eq!(plan.move_id, PREP_SLAM);
        assert_eq!(plan.visible_spec, Some(MonsterMoveSpec::Unknown));
    }

    #[test]
    fn slime_boss_first_roll_uses_private_first_turn_and_marks_it() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let monster = crate::test_support::test_monster(EnemyId::SlimeBoss);
        let plan = SlimeBoss::roll_move_plan(&mut rng, &monster, 0, 99);

        assert_eq!(plan.move_id, STICKY);
        assert_eq!(
            SlimeBoss::on_roll_move(0, &monster, 99, &plan),
            vec![Action::UpdateMonsterRuntime {
                monster_id: 1,
                patch: MonsterRuntimePatch::SlimeBoss {
                    first_turn: Some(false),
                    protocol_seeded: Some(true),
                },
            }]
        );
    }

    #[test]
    fn slime_boss_non_first_roll_keeps_existing_move_instead_of_advancing_history_cycle() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let mut monster = crate::test_support::test_monster(EnemyId::SlimeBoss);
        monster.slime_boss.first_turn = false;
        monster.set_planned_move_id(SLAM);
        monster.move_history_mut().clear();

        assert_eq!(
            SlimeBoss::roll_move_plan(&mut rng, &monster, 0, 99).move_id,
            SLAM,
            "Java SlimeBoss.getMove() is a no-op after firstTurn is false; the normal cycle is set in takeTurn()"
        );
    }
}
