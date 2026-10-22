use super::{
    add_card_action, apply_power_action, attack_actions, gain_block_action, set_next_move_action,
    upgrade_cards_action, PLAYER,
};
use crate::content::cards::CardId;
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AddCardStep, ApplyPowerStep, AttackSpec, AttackStep, BlockStep, BuffSpec, CardDestination,
    DamageKind, EffectStrength, MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget,
    PowerEffectKind, UpgradeCardsStep,
};

const DIVIDER: u8 = 1;
const TACKLE: u8 = 2;
const INFLAME: u8 = 3;
const SEAR: u8 = 4;
const ACTIVATE: u8 = 5;
const INFERNO: u8 = 6;
const SEAR_DMG: i32 = 6;
const STRENGTHEN_BLOCK: i32 = 12;

pub struct Hexaghost;

enum HexaghostTurn<'a> {
    Activate,
    Divider(&'a AttackSpec),
    Tackle(&'a AttackSpec),
    Inflame {
        block: &'a BlockStep,
        strength: &'a ApplyPowerStep,
    },
    Sear {
        attack: &'a AttackSpec,
        burn: &'a AddCardStep,
    },
    Inferno {
        attack: &'a AttackSpec,
        upgrade: &'a UpgradeCardsStep,
        burn: &'a AddCardStep,
    },
}

fn divider_damage(state: &CombatState) -> i32 {
    state.entities.player.current_hp / 12 + 1
}

fn tackle_damage(asc: u8) -> i32 {
    if asc >= 4 {
        6
    } else {
        5
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;

    #[test]
    fn first_roll_marks_hexaghost_activated_like_java_get_move() {
        let mut state = crate::test_support::blank_test_combat();
        let hexaghost = crate::test_support::test_monster(EnemyId::Hexaghost);
        state.entities.monsters = vec![hexaghost];

        crate::engine::action_handlers::execute_action(
            Action::RollMonsterMove { monster_id: 1 },
            &mut state,
        );

        let hexaghost = &state.entities.monsters[0];
        assert_eq!(hexaghost.planned_move_id(), ACTIVATE);
        assert!(
            hexaghost.hexaghost.activated,
            "Java Hexaghost.getMove sets activated=true while rolling the opening ACTIVATE move, before the ACTIVATE turn executes"
        );
        assert_eq!(
            hexaghost.turn_plan().visible_spec,
            Some(MonsterMoveSpec::Unknown),
            "Java Hexaghost exposes ACTIVATE with Intent.UNKNOWN"
        );
    }

    #[test]
    fn inferno_upgrades_existing_burns_then_adds_three_upgraded_burns() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 20;
        let hexaghost = crate::test_support::test_monster(EnemyId::Hexaghost);
        let plan = plan_for(INFERNO, 20, None, false);

        let actions = Hexaghost::take_turn_plan(&mut state, &hexaghost, &plan);

        assert!(matches!(actions.get(6), Some(Action::UpgradeAllBurns)));
        assert!(matches!(
            actions.get(7),
            Some(Action::MakeTempCardInDiscard {
                card_id: CardId::Burn,
                amount: 3,
                upgraded: true,
            })
        ));
        assert_eq!(
            plan.visible_spec,
            Some(MonsterMoveSpec::AttackUpgradeCards(
                AttackSpec {
                    base_damage: 3,
                    hits: 6,
                    damage_kind: DamageKind::Normal,
                },
                UpgradeCardsStep {
                    card_id: CardId::Burn,
                },
            )),
            "generated Burns are exact Inferno settlement, not a separately advertised intent"
        );
    }

    #[test]
    fn sixth_orb_activation_records_java_intermediate_inferno_move() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 20;
        let mut hexaghost = crate::test_support::test_monster(EnemyId::Hexaghost);
        hexaghost.hexaghost.activated = true;
        hexaghost.hexaghost.orb_active_count = 5;
        let plan = plan_for(SEAR, 20, None, false);
        state.entities.monsters = vec![hexaghost.clone()];

        let actions = Hexaghost::take_turn_plan(&mut state, &hexaghost, &plan);
        for action in actions {
            crate::engine::action_handlers::execute_action(action, &mut state);
        }

        let hexaghost = &state.entities.monsters[0];
        assert_eq!(hexaghost.hexaghost.orb_active_count, 6);
        assert_eq!(hexaghost.planned_move_id(), INFERNO);
        assert_eq!(
            hexaghost
                .move_history()
                .iter()
                .rev()
                .take(2)
                .copied()
                .collect::<Vec<_>>(),
            vec![INFERNO, INFERNO],
            "Java Activate Orb sets Inferno immediately before the queued RollMoveAction sets it again"
        );
    }
}

fn inferno_damage(asc: u8) -> i32 {
    if asc >= 4 {
        3
    } else {
        2
    }
}

fn strength_amount(asc: u8) -> i32 {
    if asc >= 19 {
        3
    } else {
        2
    }
}

fn sear_burn_count(asc: u8) -> u8 {
    if asc >= 19 {
        2
    } else {
        1
    }
}

fn steps_for(
    move_id: u8,
    asc: u8,
    locked_divider_damage: Option<i32>,
    burn_upgraded: bool,
) -> crate::semantics::combat::MonsterTurnSteps {
    match move_id {
        DIVIDER => smallvec::smallvec![MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: locked_divider_damage.expect("hexaghost divider damage missing"),
                hits: 6,
                damage_kind: DamageKind::Normal,
            },
        })],
        TACKLE => smallvec::smallvec![MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: tackle_damage(asc),
                hits: 2,
                damage_kind: DamageKind::Normal,
            },
        })],
        INFLAME => smallvec::smallvec![
            MoveStep::GainBlock(BlockStep {
                target: MoveTarget::SelfTarget,
                amount: STRENGTHEN_BLOCK,
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::Strength,
                amount: strength_amount(asc),
                effect: PowerEffectKind::Buff,
                visible_strength: EffectStrength::Normal,
            })
        ],
        SEAR => smallvec::smallvec![
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: SEAR_DMG,
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::AddCard(AddCardStep {
                card_id: CardId::Burn,
                amount: sear_burn_count(asc),
                upgraded: burn_upgraded,
                destination: CardDestination::Discard,
                visible_strength: EffectStrength::Normal,
            })
        ],
        ACTIVATE => smallvec::smallvec![MoveStep::Magic],
        INFERNO => smallvec::smallvec![
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: inferno_damage(asc),
                    hits: 6,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::UpgradeCards(UpgradeCardsStep {
                card_id: CardId::Burn,
            }),
            MoveStep::AddCard(AddCardStep {
                card_id: CardId::Burn,
                amount: 3,
                upgraded: true,
                destination: CardDestination::Discard,
                visible_strength: EffectStrength::Normal,
            })
        ],
        _ => smallvec::smallvec![],
    }
}

fn plan_for(
    move_id: u8,
    asc: u8,
    locked_divider_damage: Option<i32>,
    burn_upgraded: bool,
) -> MonsterTurnPlan {
    let steps = steps_for(move_id, asc, locked_divider_damage, burn_upgraded);
    let visible_spec = match move_id {
        ACTIVATE => Some(MonsterMoveSpec::Unknown),
        INFLAME => Some(MonsterMoveSpec::DefendBuff(
            crate::semantics::combat::DefendSpec {
                block: STRENGTHEN_BLOCK,
            },
            BuffSpec {
                power_id: PowerId::Strength,
                amount: strength_amount(asc),
            },
        )),
        SEAR => Some(MonsterMoveSpec::AttackAddCard(
            AttackSpec {
                base_damage: SEAR_DMG,
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            AddCardStep {
                card_id: CardId::Burn,
                amount: sear_burn_count(asc),
                upgraded: burn_upgraded,
                destination: CardDestination::Discard,
                visible_strength: EffectStrength::Normal,
            },
        )),
        INFERNO => Some(MonsterMoveSpec::AttackUpgradeCards(
            AttackSpec {
                base_damage: inferno_damage(asc),
                hits: 6,
                damage_kind: DamageKind::Normal,
            },
            UpgradeCardsStep {
                card_id: CardId::Burn,
            },
        )),
        _ => None,
    };
    match visible_spec {
        Some(visible_spec) => MonsterTurnPlan::with_visible_spec(move_id, steps, visible_spec),
        None => MonsterTurnPlan::new(move_id, steps),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            (
                Text::new("hexaghost.tackle"),
                plan_for(TACKLE, asc, None, false),
            ),
            (
                Text::new("hexaghost.inflame"),
                plan_for(INFLAME, asc, None, false),
            ),
            (
                Text::new("hexaghost.searBeforeFirstInferno"),
                plan_for(SEAR, asc, None, false),
            ),
            (
                Text::new("hexaghost.searAfterFirstInferno"),
                plan_for(SEAR, asc, None, true),
            ),
            (
                Text::new("hexaghost.inferno"),
                plan_for(INFERNO, asc, None, false),
            ),
        ],
        special_moves: vec![
            (
                Text::new("hexaghost.activate"),
                Text::new("hexaghost.activateRule"),
            ),
            (
                Text::new("hexaghost.divider"),
                Text::new("hexaghost.dividerRule"),
            ),
        ],
        notes: Text::new("hexaghost.notes"),
    }
}

fn activate_orb_actions(entity: &MonsterEntity, ascension_level: u8) -> Vec<Action> {
    let next_orb_count = entity.hexaghost.orb_active_count.saturating_add(1).min(6);
    let mut actions = vec![Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Hexaghost {
            activated: None,
            orb_active_count: Some(next_orb_count),
            burn_upgraded: None,
            divider_damage: None,
            clear_divider_damage: false,
        },
    }];
    if next_orb_count == 6 {
        // Java `changeState("Activate Orb")` calls `setMove(INFERNO)` as soon as
        // the sixth orb lights. The already queued `RollMoveAction` then calls
        // `getMove` and sets Inferno a second time, so both writes belong in
        // `moveHistory` even though they advertise the same visible intent.
        actions.push(set_next_move_action(
            entity,
            plan_for(
                INFERNO,
                ascension_level,
                None,
                entity.hexaghost.burn_upgraded,
            ),
        ));
    }
    actions
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> HexaghostTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (ACTIVATE, [MoveStep::Magic]) => HexaghostTurn::Activate,
        (
            DIVIDER,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => HexaghostTurn::Divider(attack),
        (
            TACKLE,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => HexaghostTurn::Tackle(attack),
        (
            SEAR,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::AddCard(burn)],
        ) if burn.card_id == CardId::Burn && burn.destination == CardDestination::Discard => {
            HexaghostTurn::Sear { attack, burn }
        }
        (
            INFERNO,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::UpgradeCards(upgrade), MoveStep::AddCard(burn)],
        ) if burn.card_id == CardId::Burn
            && burn.amount == 3
            && burn.upgraded
            && burn.destination == CardDestination::Discard =>
        {
            HexaghostTurn::Inferno {
                attack,
                upgrade,
                burn,
            }
        }
        (INFLAME, [MoveStep::GainBlock(block), MoveStep::ApplyPower(strength)])
            if block.target == MoveTarget::SelfTarget
                && strength.target == MoveTarget::SelfTarget
                && strength.power_id == PowerId::Strength
                && strength.effect == PowerEffectKind::Buff =>
        {
            HexaghostTurn::Inflame { block, strength }
        }
        (_, []) => panic!("hexaghost plan missing locked truth"),
        (move_id, steps) => panic!("hexaghost plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for Hexaghost {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        if !entity.hexaghost.activated {
            return plan_for(
                ACTIVATE,
                ascension_level,
                None,
                entity.hexaghost.burn_upgraded,
            );
        }
        let move_id = match entity.hexaghost.orb_active_count {
            0 | 2 | 5 => SEAR,
            1 | 4 => TACKLE,
            3 => INFLAME,
            6 => INFERNO,
            count => panic!("hexaghost orb_active_count invalid: {}", count),
        };
        plan_for(
            move_id,
            ascension_level,
            entity.hexaghost.divider_damage,
            entity.hexaghost.burn_upgraded,
        )
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if !entity.hexaghost.activated && plan.move_id == ACTIVATE {
            vec![Action::UpdateMonsterRuntime {
                monster_id: entity.id,
                patch: MonsterRuntimePatch::Hexaghost {
                    activated: Some(true),
                    orb_active_count: None,
                    burn_upgraded: None,
                    divider_damage: None,
                    clear_divider_damage: false,
                },
            }]
        } else {
            Vec::new()
        }
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        if entity.is_dying || entity.half_dead {
            return entity.turn_plan();
        }
        plan_for(
            entity.planned_move_id(),
            state.meta.ascension_level,
            entity.hexaghost.divider_damage,
            entity.hexaghost.burn_upgraded,
        )
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let asc = state.meta.ascension_level;
        let mut actions = Vec::new();
        match decode_turn(plan) {
            HexaghostTurn::Activate => {
                let locked = divider_damage(state);
                actions.push(Action::UpdateMonsterRuntime {
                    monster_id: entity.id,
                    patch: MonsterRuntimePatch::Hexaghost {
                        activated: Some(true),
                        orb_active_count: Some(6),
                        burn_upgraded: None,
                        divider_damage: Some(locked),
                        clear_divider_damage: false,
                    },
                });
                actions.push(set_next_move_action(
                    entity,
                    plan_for(DIVIDER, asc, Some(locked), entity.hexaghost.burn_upgraded),
                ));
                return actions;
            }
            HexaghostTurn::Divider(attack) => {
                actions.extend(attack_actions(entity.id, PLAYER, attack));
                actions.push(Action::UpdateMonsterRuntime {
                    monster_id: entity.id,
                    patch: MonsterRuntimePatch::Hexaghost {
                        activated: None,
                        orb_active_count: Some(0),
                        burn_upgraded: None,
                        divider_damage: None,
                        clear_divider_damage: true,
                    },
                });
            }
            HexaghostTurn::Tackle(attack) => {
                actions.extend(attack_actions(entity.id, PLAYER, attack));
                actions.extend(activate_orb_actions(entity, asc));
            }
            HexaghostTurn::Inflame { block, strength } => {
                actions.push(gain_block_action(entity, block));
                actions.push(apply_power_action(entity, strength));
                actions.extend(activate_orb_actions(entity, asc));
            }
            HexaghostTurn::Sear { attack, burn } => {
                actions.extend(attack_actions(entity.id, PLAYER, attack));
                actions.push(add_card_action(burn));
                actions.extend(activate_orb_actions(entity, asc));
            }
            HexaghostTurn::Inferno {
                attack,
                upgrade,
                burn,
            } => {
                actions.extend(attack_actions(entity.id, PLAYER, attack));
                actions.push(upgrade_cards_action(upgrade));
                actions.push(add_card_action(burn));
                actions.push(Action::UpdateMonsterRuntime {
                    monster_id: entity.id,
                    patch: MonsterRuntimePatch::Hexaghost {
                        activated: None,
                        orb_active_count: Some(0),
                        burn_upgraded: Some(true),
                        divider_damage: None,
                        clear_divider_damage: false,
                    },
                });
            }
        }
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
