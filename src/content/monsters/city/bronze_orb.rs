use crate::content::monsters::MonsterBehavior;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{DefendSpec, MonsterMoveSpec, MonsterTurnPlan};

pub struct BronzeOrb;

const BEAM: u8 = 1;
const SUPPORT_BEAM: u8 = 2;
const STASIS: u8 = 3;
const BLOCK_AMOUNT: i32 = 12;
const BEAM_DAMAGE: i32 = 8;

pub(crate) fn roll_constructor_hp(rng: &mut crate::runtime::rng::StsRng, ascension: u8) -> i32 {
    // BronzeOrb rolls the super argument, then replaces it through setHp.
    let _ = rng.random_range(52, 58);
    if ascension >= 9 {
        rng.random_range(54, 60)
    } else {
        rng.random_range(52, 58)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;

    #[test]
    fn support_beam_targets_automaton_by_id_without_hp_or_escape_filter_like_java() {
        let mut automaton = crate::test_support::test_monster(EnemyId::BronzeAutomaton);
        automaton.current_hp = 0;
        automaton.is_dying = false;
        automaton.is_escaped = true;
        let mut orb = crate::test_support::test_monster(EnemyId::BronzeOrb);
        orb.id = 2;
        let mut state = crate::test_support::combat_with_monsters(vec![automaton, orb.clone()]);

        let actions = BronzeOrb::take_turn_plan(&mut state, &orb, &support_beam_plan());

        assert!(matches!(
            actions.as_slice(),
            [
                Action::GainBlock {
                    target: 1,
                    amount: BLOCK_AMOUNT
                },
                Action::RollMonsterMove { monster_id: 2 }
            ]
        ));
    }

    #[test]
    fn roll_move_sets_used_stasis_only_when_stasis_is_selected_like_java() {
        let mut orb = crate::test_support::test_monster(EnemyId::BronzeOrb);

        let stasis =
            BronzeOrb::roll_move_plan(&mut crate::runtime::rng::StsRng::new(0), &orb, 0, 25);
        assert_eq!(stasis.move_id, STASIS);
        assert_eq!(
            BronzeOrb::on_roll_move(0, &orb, 25, &stasis),
            vec![orb_runtime_update(&orb, Some(true))],
            "Java BronzeOrb sets usedStasis inside getMove only when it chooses STASIS"
        );

        orb.bronze_orb.used_stasis = true;
        let later =
            BronzeOrb::roll_move_plan(&mut crate::runtime::rng::StsRng::new(0), &orb, 0, 25);
        assert_eq!(
            later.move_id, BEAM,
            "After usedStasis is true, Java falls through to Support/Beam selector"
        );
        assert!(BronzeOrb::on_roll_move(0, &orb, 25, &later).is_empty());
    }

    #[test]
    fn support_and_beam_last_two_gates_match_java() {
        let mut orb = crate::test_support::test_monster(EnemyId::BronzeOrb);
        orb.bronze_orb.used_stasis = true;

        let support =
            BronzeOrb::roll_move_plan(&mut crate::runtime::rng::StsRng::new(0), &orb, 0, 70);
        assert_eq!(support.move_id, SUPPORT_BEAM);

        orb.move_history_mut().push_back(SUPPORT_BEAM);
        orb.move_history_mut().push_back(SUPPORT_BEAM);
        let beam_after_two_supports =
            BronzeOrb::roll_move_plan(&mut crate::runtime::rng::StsRng::new(0), &orb, 0, 70);
        assert_eq!(
            beam_after_two_supports.move_id, BEAM,
            "Java num>=70 Support branch is blocked only by lastTwoMoves(SUPPORT_BEAM)"
        );

        orb.move_history_mut().clear();
        orb.move_history_mut().push_back(BEAM);
        orb.move_history_mut().push_back(BEAM);
        let support_after_two_beams =
            BronzeOrb::roll_move_plan(&mut crate::runtime::rng::StsRng::new(0), &orb, 0, 10);
        assert_eq!(
            support_after_two_beams.move_id, SUPPORT_BEAM,
            "Java falls back to Support when the Beam branch is blocked by lastTwoMoves(BEAM)"
        );
    }

    #[test]
    fn stasis_take_turn_queues_apply_stasis_before_roll_like_java() {
        let orb = crate::test_support::test_monster(EnemyId::BronzeOrb);
        let mut state = crate::test_support::combat_with_monsters(vec![orb.clone()]);

        let actions = BronzeOrb::take_turn_plan(&mut state, &orb, &stasis_plan());

        assert_eq!(
            actions,
            vec![
                Action::ApplyStasis { target_id: orb.id },
                Action::RollMonsterMove { monster_id: orb.id },
            ]
        );
    }
}

fn current_used_stasis(entity: &MonsterEntity) -> bool {
    assert!(
        entity.bronze_orb.protocol_seeded,
        "bronze orb runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.bronze_orb.used_stasis
}

fn orb_runtime_update(entity: &MonsterEntity, used_stasis: Option<bool>) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::BronzeOrb {
            used_stasis,
            protocol_seeded: Some(true),
        },
    }
}

fn beam_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        BEAM,
        MonsterMoveSpec::Attack(crate::semantics::combat::AttackSpec {
            base_damage: BEAM_DAMAGE,
            hits: 1,
            damage_kind: crate::semantics::combat::DamageKind::Normal,
        }),
    )
}

fn support_beam_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        SUPPORT_BEAM,
        smallvec::smallvec![],
        MonsterMoveSpec::Defend(DefendSpec {
            block: BLOCK_AMOUNT,
        }),
    )
}

fn stasis_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        STASIS,
        smallvec::smallvec![],
        MonsterMoveSpec::StrongDebuff(crate::semantics::combat::DebuffSpec {
            power_id: crate::content::powers::PowerId::Stasis,
            amount: 1,
            strength: crate::semantics::combat::EffectStrength::Strong,
        }),
    )
}

fn plan_for(move_id: u8) -> MonsterTurnPlan {
    match move_id {
        BEAM => beam_plan(),
        SUPPORT_BEAM => support_beam_plan(),
        STASIS => stasis_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics() -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::super::mechanics::MonsterMechanics {
        moves: vec![(Text::new("bronze_orb.beam"), beam_plan())],
        special_moves: vec![
            (
                Text::new("bronze_orb.supportBeam"),
                Text::new("bronze_orb.supportRule").with("block", BLOCK_AMOUNT),
            ),
            (
                Text::new("bronze_orb.stasis"),
                Text::new("bronze_orb.stasisRule"),
            ),
        ],
        notes: Text::new("bronze_orb.notes"),
    }
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let mut history = entity.move_history().iter().rev();
    matches!(
        (history.next().copied(), history.next().copied()),
        (Some(a), Some(b)) if a == move_id && b == move_id
    )
}

fn automaton_id(state: &CombatState) -> Option<usize> {
    state.entities.monsters.iter().find_map(|monster| {
        (monster.enemy_id == crate::content::monsters::EnemyId::BronzeAutomaton)
            .then_some(monster.id)
    })
}

impl MonsterBehavior for BronzeOrb {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        _ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if !current_used_stasis(entity) && num >= 25 {
            return stasis_plan();
        }
        if num >= 70 && !last_two_moves(entity, SUPPORT_BEAM) {
            return support_beam_plan();
        }
        if !last_two_moves(entity, BEAM) {
            return beam_plan();
        }
        support_beam_plan()
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if plan.move_id == STASIS {
            vec![orb_runtime_update(entity, Some(true))]
        } else {
            Vec::new()
        }
    }

    fn turn_plan(_state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id())
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match plan.move_id {
            BEAM => vec![Action::MonsterAttack {
                source: entity.id,
                target: 0,
                base_damage: BEAM_DAMAGE,
                damage_kind: crate::semantics::combat::DamageKind::Normal,
            }],
            SUPPORT_BEAM => automaton_id(state)
                .map(|target| {
                    vec![Action::GainBlock {
                        target,
                        amount: BLOCK_AMOUNT,
                    }]
                })
                .unwrap_or_default(),
            STASIS => vec![Action::ApplyStasis {
                target_id: entity.id,
            }],
            _ => panic!(
                "bronze orb take_turn received unsupported move {}",
                plan.move_id
            ),
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
