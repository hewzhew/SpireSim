use super::{add_card_action, attack_actions, PLAYER};
use crate::content::cards::CardId;
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AddCardStep, AttackSpec, AttackStep, CardDestination, DamageKind, EffectStrength,
    MonsterTurnPlan, MoveStep, MoveTarget,
};

const BOLT: u8 = 3;
const BEAM: u8 = 4;

pub struct Sentry;

enum SentryTurn<'a> {
    Bolt(&'a AddCardStep),
    Beam(&'a AttackSpec),
}

fn beam_damage(asc: u8) -> i32 {
    if asc >= 3 {
        10
    } else {
        9
    }
}

fn dazed_amount(asc: u8) -> u8 {
    if asc >= 18 {
        3
    } else {
        2
    }
}

fn bolt_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        BOLT,
        MoveStep::AddCard(AddCardStep {
            card_id: CardId::Dazed,
            amount: dazed_amount(asc),
            upgraded: false,
            destination: CardDestination::Discard,
            // Original PC V2.3.4 Sentry.getMove exposes Bolt with
            // AbstractMonster.Intent.DEBUFF, not STRONG_DEBUFF.
            visible_strength: EffectStrength::Normal,
        }),
    )
}

fn beam_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        BEAM,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: beam_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        BOLT => bolt_plan(asc),
        BEAM => beam_plan(asc),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        special_moves: vec![],
        moves: vec![("Beam", beam_plan(asc)), ("Bolt", bolt_plan(asc))],
        notes: "Each Sentry alternates Beam and Bolt. In the standard three-Sentry encounter, the outer slots open with Bolt and the middle opens with Beam; these are opening phases, not permanent roles. Bolt adds Dazed to the discard pile. Each Sentry starts with 1 Artifact; inspect current powers for remaining charges.",
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> SentryTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (BOLT, [MoveStep::AddCard(add_card)])
            if add_card.card_id == CardId::Dazed
                && add_card.destination == CardDestination::Discard =>
        {
            SentryTurn::Bolt(add_card)
        }
        (
            BEAM,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => SentryTurn::Beam(attack),
        (_, []) => panic!("sentry plan missing locked truth"),
        (move_id, steps) => panic!("sentry plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

fn runtime(entity: &MonsterEntity) -> bool {
    assert!(
        entity.sentry.protocol_seeded,
        "sentry runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.sentry.first_move
}

fn sentry_runtime_update(entity: &MonsterEntity, first_move: Option<bool>) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Sentry {
            first_move,
            protocol_seeded: Some(true),
        },
    }
}

impl MonsterBehavior for Sentry {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        if runtime(entity) {
            if entity.slot.is_multiple_of(2) {
                bolt_plan(ascension_level)
            } else {
                beam_plan(ascension_level)
            }
        } else {
            match entity.move_history().back().copied() {
                Some(BEAM) => bolt_plan(ascension_level),
                _ => beam_plan(ascension_level),
            }
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if runtime(entity) {
            vec![sentry_runtime_update(entity, Some(false))]
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
            power_id: PowerId::Artifact,
            amount: 1,
        }]
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match decode_turn(plan) {
            SentryTurn::Bolt(add_card) => vec![
                add_card_action(add_card),
                Action::RollMonsterMove {
                    monster_id: entity.id,
                },
            ],
            SentryTurn::Beam(attack) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(Action::RollMonsterMove {
                    monster_id: entity.id,
                });
                actions
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{bolt_plan, Sentry, BEAM, BOLT};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::action::{Action, MonsterRuntimePatch};
    use crate::sim::combat_projection::{MonsterMovePreview, VisibleIntentKind};

    #[test]
    fn sentry_bolt_projects_the_original_games_debuff_intent() {
        let preview = MonsterMovePreview::from_plan(&bolt_plan(18), None);

        assert_eq!(preview.visible_intent, VisibleIntentKind::Debuff);
    }

    #[test]
    fn sentry_first_roll_uses_private_first_move_and_marks_it() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let monster = crate::testing::support::test_monster(EnemyId::Sentry);
        let plan = Sentry::roll_move_plan(&mut rng, &monster, 0, 99);

        assert_eq!(plan.move_id, BOLT);
        assert_eq!(
            Sentry::on_roll_move(0, &monster, 99, &plan),
            vec![Action::UpdateMonsterRuntime {
                monster_id: 1,
                patch: MonsterRuntimePatch::Sentry {
                    first_move: Some(false),
                    protocol_seeded: Some(true),
                },
            }]
        );
    }

    #[test]
    fn sentry_first_move_is_private_runtime_not_empty_history() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let mut monster = crate::testing::support::test_monster(EnemyId::Sentry);
        monster.sentry.first_move = false;

        assert_eq!(
            Sentry::roll_move_plan(&mut rng, &monster, 0, 99).move_id,
            BEAM,
            "Java uses private firstMove; empty imported history alone must not force opening parity"
        );
    }
}
