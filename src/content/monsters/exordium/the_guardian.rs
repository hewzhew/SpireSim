use super::{
    apply_power_action, attack_actions, gain_block_action, remove_power_action,
    set_next_move_action, PLAYER,
};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, GuardianRuntimeState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, BlockStep, DamageKind, DebuffSpec, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind, RemovePowerStep,
};

const CLOSE_UP: u8 = 1;
const FIERCE_BASH: u8 = 2;
const ROLL_ATTACK: u8 = 3;
const TWIN_SLAM: u8 = 4;
const WHIRLWIND: u8 = 5;
const CHARGE_UP: u8 = 6;
const VENT_STEAM: u8 = 7;

const THRESHOLD_INCREASE: i32 = 10;
const DEFENSIVE_BLOCK: i32 = 20;
const CHARGE_UP_BLOCK: i32 = 9;
const WHIRLWIND_DAMAGE: i32 = 5;
const WHIRLWIND_HITS: u8 = 4;
const TWIN_SLAM_DAMAGE: i32 = 8;
const TWIN_SLAM_HITS: u8 = 2;
const VENT_DEBUFF: i32 = 2;

pub struct TheGuardian;

enum GuardianTurn<'a> {
    ChargeUp(&'a BlockStep),
    FierceBash(&'a AttackSpec),
    VentSteam {
        weak: &'a ApplyPowerStep,
        vulnerable: &'a ApplyPowerStep,
    },
    Whirlwind(&'a AttackSpec),
    CloseUp(&'a ApplyPowerStep),
    RollAttack(&'a AttackSpec),
    TwinSlam {
        mode_shift: &'a ApplyPowerStep,
        attack: &'a AttackSpec,
        sharp_hide: &'a RemovePowerStep,
    },
}

fn initial_threshold(asc: u8) -> i32 {
    if asc >= 19 {
        40
    } else if asc >= 9 {
        35
    } else {
        30
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::runtime::combat::{Power, PowerPayload};

    fn drain_actions(state: &mut CombatState) {
        while let Some(action) = state.pop_next_action() {
            crate::engine::action_handlers::execute_action(action, state);
        }
    }

    fn test_power(power_type: PowerId, amount: i32) -> Power {
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
    fn twin_slam_keeps_defensive_block_until_after_reflected_damage_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.player.current_hp = 80;

        let mut guardian = crate::test_support::test_monster(EnemyId::TheGuardian);
        guardian.current_hp = 50;
        guardian.max_hp = 250;
        guardian.block = 20;
        guardian.guardian = GuardianRuntimeState {
            damage_threshold: 30,
            damage_taken: 0,
            is_open: false,
            close_up_triggered: false,
        };
        guardian.set_planned_move_id(TWIN_SLAM);
        guardian.set_planned_steps(twin_slam_plan(30).steps);
        state.entities.monsters = vec![guardian.clone()];

        state
            .entities
            .power_db
            .insert(0, vec![test_power(PowerId::FlameBarrier, 4)]);
        state.entities.power_db.insert(
            guardian.id,
            vec![test_power(PowerId::SharpHide, sharp_hide_damage(0))],
        );

        let plan = twin_slam_plan(guardian.guardian.damage_threshold);
        for action in <TheGuardian as MonsterBehavior>::take_turn_plan(&mut state, &guardian, &plan)
        {
            state.queue_action_back(action);
        }
        drain_actions(&mut state);

        let guardian_after = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == guardian.id)
            .unwrap();
        assert_eq!(
            guardian_after.current_hp, 50,
            "Java ChangeState(Offensive Mode) queues LoseBlock after Twin Slam attacks, so reflected Flame Barrier damage is absorbed by existing defensive block"
        );
        assert_eq!(guardian_after.block, 0);
        assert!(guardian_after.guardian.is_open);
        assert!(!guardian_after.guardian.close_up_triggered);
        assert_eq!(
            crate::content::powers::store::power_amount(&state, guardian.id, PowerId::ModeShift),
            30
        );
    }

    #[test]
    fn mode_shift_threshold_keeps_power_until_defensive_change_state_resolves() {
        let mut state = crate::test_support::blank_test_combat();

        let mut guardian = crate::test_support::test_monster(EnemyId::TheGuardian);
        guardian.current_hp = 100;
        guardian.max_hp = 250;
        guardian.guardian = GuardianRuntimeState {
            damage_threshold: 5,
            damage_taken: 0,
            is_open: true,
            close_up_triggered: false,
        };
        guardian.set_planned_move_id(ROLL_ATTACK);
        guardian.set_planned_steps(roll_attack_plan(0).steps);
        let guardian_id = guardian.id;
        state.entities.monsters = vec![guardian];
        state
            .entities
            .power_db
            .insert(guardian_id, vec![test_power(PowerId::ModeShift, 5)]);

        crate::engine::action_handlers::execute_action(
            Action::Damage(crate::runtime::action::DamageInfo {
                source: 0,
                target: guardian_id,
                base: 5,
                output: 5,
                damage_type: crate::runtime::action::DamageType::Normal,
                is_modified: false,
            }),
            &mut state,
        );

        let marker = state.pop_next_action().expect("guardian threshold marker");
        assert!(matches!(
            marker,
            Action::GuardianModeShiftThresholdTriggered { .. }
        ));
        crate::engine::action_handlers::execute_action(marker, &mut state);

        let guardian_after_marker = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == guardian_id)
            .unwrap();
        assert!(
            crate::content::powers::store::has_power(&state, guardian_id, PowerId::ModeShift),
            "Java mutates Mode Shift amount directly and keeps the power until queued ChangeState removes it"
        );
        assert_eq!(
            crate::content::powers::store::power_amount(&state, guardian_id, PowerId::ModeShift),
            0
        );
        assert!(guardian_after_marker.guardian.is_open);
        assert!(guardian_after_marker.guardian.close_up_triggered);
        assert_eq!(guardian_after_marker.planned_move_id(), ROLL_ATTACK);

        let change_state = state
            .pop_next_action()
            .expect("queued defensive ChangeState action");
        assert!(matches!(
            change_state,
            Action::GuardianEnterDefensiveMode { .. }
        ));
        crate::engine::action_handlers::execute_action(change_state, &mut state);

        let guardian_after_change_state = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == guardian_id)
            .unwrap();
        assert!(!guardian_after_change_state.guardian.is_open);
        assert_eq!(guardian_after_change_state.guardian.damage_threshold, 15);
        assert_eq!(guardian_after_change_state.planned_move_id(), CLOSE_UP);
        assert_eq!(
            guardian_after_change_state.block, 0,
            "GainBlock is queued by ChangeState and has not resolved yet"
        );

        drain_actions(&mut state);
        let guardian_after_queued_effects = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == guardian_id)
            .unwrap();
        assert_eq!(guardian_after_queued_effects.block, DEFENSIVE_BLOCK);
        assert!(!crate::content::powers::store::has_power(
            &state,
            guardian_id,
            PowerId::ModeShift
        ));
    }
}

fn fierce_bash_damage(asc: u8) -> i32 {
    if asc >= 4 {
        36
    } else {
        32
    }
}

fn roll_damage(asc: u8) -> i32 {
    if asc >= 4 {
        10
    } else {
        9
    }
}

fn sharp_hide_damage(asc: u8) -> i32 {
    if asc >= 19 {
        4
    } else {
        3
    }
}

fn attack_plan(move_id: u8, damage: i32, hits: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        move_id,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: damage,
                hits,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

fn close_up_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        CLOSE_UP,
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::SelfTarget,
            power_id: PowerId::SharpHide,
            amount: sharp_hide_damage(asc),
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        }),
    )
}

fn fierce_bash_plan(asc: u8) -> MonsterTurnPlan {
    attack_plan(FIERCE_BASH, fierce_bash_damage(asc), 1)
}

fn roll_attack_plan(asc: u8) -> MonsterTurnPlan {
    attack_plan(ROLL_ATTACK, roll_damage(asc), 1)
}

fn whirlwind_plan() -> MonsterTurnPlan {
    attack_plan(WHIRLWIND, WHIRLWIND_DAMAGE, WHIRLWIND_HITS)
}

fn charge_up_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        CHARGE_UP,
        MoveStep::GainBlock(BlockStep {
            target: MoveTarget::SelfTarget,
            amount: CHARGE_UP_BLOCK,
        }),
    )
}

fn vent_steam_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        VENT_STEAM,
        smallvec::smallvec![
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Weak,
                amount: VENT_DEBUFF,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Normal,
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Vulnerable,
                amount: VENT_DEBUFF,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Normal,
            }),
        ],
        MonsterMoveSpec::StrongDebuff(DebuffSpec {
            power_id: PowerId::Weak,
            amount: VENT_DEBUFF,
            strength: EffectStrength::Strong,
        }),
    )
}

fn twin_slam_plan(mode_shift_amount: i32) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        TWIN_SLAM,
        smallvec::smallvec![
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::ModeShift,
                amount: mode_shift_amount,
                effect: PowerEffectKind::Buff,
                visible_strength: EffectStrength::Normal,
            }),
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: TWIN_SLAM_DAMAGE,
                    hits: TWIN_SLAM_HITS,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::RemovePower(RemovePowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::SharpHide,
            }),
        ],
        MonsterMoveSpec::AttackBuff(
            AttackSpec {
                base_damage: TWIN_SLAM_DAMAGE,
                hits: TWIN_SLAM_HITS,
                damage_kind: DamageKind::Normal,
            },
            crate::semantics::combat::BuffSpec {
                power_id: PowerId::ModeShift,
                amount: mode_shift_amount,
            },
        ),
    )
}

fn plan_for(move_id: u8, asc: u8, mode_shift_amount: i32) -> MonsterTurnPlan {
    match move_id {
        CLOSE_UP => close_up_plan(asc),
        FIERCE_BASH => fierce_bash_plan(asc),
        ROLL_ATTACK => roll_attack_plan(asc),
        TWIN_SLAM => twin_slam_plan(mode_shift_amount),
        WHIRLWIND => whirlwind_plan(),
        CHARGE_UP => charge_up_plan(),
        VENT_STEAM => vent_steam_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub const MODE_SHIFT_MECHANICS: crate::content::powers::mechanics::PowerMechanics = crate::content::powers::mechanics::PowerMechanics {
    amount_unit: "remaining HP damage to trigger Defensive Mode",
    effect: "Actual HP loss reduces the remaining amount; damage absorbed by Block does not. Reaching the threshold replaces the current intent with Close Up, removes Mode Shift and grants Defensive Block. The next threshold increases each time; see the Guardian definition for difficulty-specific amounts.",
    timing: "Active in Offensive Mode; restored with the new threshold after Twin Slam. No round decay.",
    round_duration: false,
};

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            ("Charge Up", charge_up_plan()),
            ("Fierce Bash", fierce_bash_plan(asc)),
            ("Vent Steam", vent_steam_plan()),
            ("Whirlwind", whirlwind_plan()),
            ("Close Up", close_up_plan(asc)),
            ("Roll Attack", roll_attack_plan(asc)),
        ],
        special_moves: vec![
            ("Mode Shift", format!("starts at {} HP damage; actual HP loss reduces the remaining counter, Blocked damage does not. At the threshold, replaces its current intent with Close Up and gains {DEFENSIVE_BLOCK} Block. Each trigger raises the next threshold by {THRESHOLD_INCREASE}.", initial_threshold(asc))),
            ("Twin Slam", format!("return to Offensive Mode; attack player for {TWIN_SLAM_DAMAGE} base damage/hit (hits: {TWIN_SLAM_HITS}); then remove Sharp Hide, restore Mode Shift to the increased threshold, and lose remaining own Block. Plans Whirlwind next.")),
            ("Sharp Hide", crate::content::powers::mechanics::get(PowerId::SharpHide).unwrap().effect.into()),
        ],
        notes: "Starts in Offensive Mode: Charge Up, Fierce Bash, Vent Steam, Whirlwind, then repeat. A Mode Shift interrupts that cycle with Defensive Mode: Close Up, Roll Attack, Twin Slam, then Offensive Whirlwind. Close Up grants Sharp Hide; it stays active until Twin Slam finishes. Current Mode Shift amount and visible intent are authoritative for this turn.",
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> GuardianTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (CHARGE_UP, [MoveStep::GainBlock(block)]) if block.target == MoveTarget::SelfTarget => {
            GuardianTurn::ChargeUp(block)
        }
        (
            FIERCE_BASH,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => GuardianTurn::FierceBash(attack),
        (VENT_STEAM, [MoveStep::ApplyPower(weak), MoveStep::ApplyPower(vulnerable)])
            if weak.target == MoveTarget::Player
                && weak.power_id == PowerId::Weak
                && weak.effect == PowerEffectKind::Debuff
                && vulnerable.target == MoveTarget::Player
                && vulnerable.power_id == PowerId::Vulnerable
                && vulnerable.effect == PowerEffectKind::Debuff =>
        {
            GuardianTurn::VentSteam { weak, vulnerable }
        }
        (
            WHIRLWIND,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => GuardianTurn::Whirlwind(attack),
        (CLOSE_UP, [MoveStep::ApplyPower(sharp_hide)])
            if sharp_hide.target == MoveTarget::SelfTarget
                && sharp_hide.power_id == PowerId::SharpHide
                && sharp_hide.effect == PowerEffectKind::Buff =>
        {
            GuardianTurn::CloseUp(sharp_hide)
        }
        (
            ROLL_ATTACK,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => GuardianTurn::RollAttack(attack),
        (
            TWIN_SLAM,
            [MoveStep::ApplyPower(mode_shift), MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::RemovePower(sharp_hide)],
        ) if mode_shift.target == MoveTarget::SelfTarget
            && mode_shift.power_id == PowerId::ModeShift
            && mode_shift.effect == PowerEffectKind::Buff
            && sharp_hide.target == MoveTarget::SelfTarget
            && sharp_hide.power_id == PowerId::SharpHide =>
        {
            GuardianTurn::TwinSlam {
                mode_shift,
                attack,
                sharp_hide,
            }
        }
        (_, []) => panic!("guardian plan missing locked truth"),
        (move_id, steps) => panic!("guardian plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

pub fn initialize_runtime_state(monster: &mut MonsterEntity, ascension_level: u8) {
    monster.guardian = GuardianRuntimeState {
        damage_threshold: initial_threshold(ascension_level),
        damage_taken: 0,
        is_open: true,
        close_up_triggered: false,
    };
}

fn guardian_runtime_update(
    entity: &MonsterEntity,
    damage_threshold: Option<i32>,
    damage_taken: Option<i32>,
    is_open: Option<bool>,
    close_up_triggered: Option<bool>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Guardian {
            damage_threshold,
            damage_taken,
            is_open,
            close_up_triggered,
        },
    }
}

pub fn handle_mode_shift_threshold_triggered(
    monster_id: crate::EntityId,
    hp_lost: i32,
    state: &mut CombatState,
) {
    if hp_lost <= 0 {
        return;
    }

    let _ = crate::content::powers::store::with_power_mut(
        state,
        monster_id,
        PowerId::ModeShift,
        |power| {
            // Java mutates ModeShiftPower.amount directly in TheGuardian.damage()
            // and does not route through ReducePowerAction's zero-removal rule.
            power.amount -= hp_lost;
        },
    );

    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|monster| monster.id == monster_id)
    {
        monster.guardian.damage_taken = 0;
        monster.guardian.close_up_triggered = true;
    }
}

pub fn handle_enter_defensive_mode(
    monster_id: crate::EntityId,
    next_threshold: i32,
    state: &mut CombatState,
) {
    let plan = close_up_plan(state.meta.ascension_level);
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|monster| monster.id == monster_id)
    {
        monster.guardian.damage_threshold = next_threshold;
        monster.guardian.is_open = false;
    }

    crate::engine::action_handlers::spawning::handle_set_monster_move(
        monster_id,
        plan.move_id,
        plan.steps,
        plan.visible_spec,
        state,
    );
    state.queue_action_back(Action::RemovePower {
        target: monster_id,
        power_id: PowerId::ModeShift,
    });
    state.queue_action_back(Action::GainBlock {
        target: monster_id,
        amount: DEFENSIVE_BLOCK,
    });
}

impl MonsterBehavior for TheGuardian {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        if entity.guardian.is_open {
            charge_up_plan()
        } else {
            roll_attack_plan(ascension_level)
        }
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(
            entity.planned_move_id(),
            state.meta.ascension_level,
            entity.guardian.damage_threshold,
        )
    }

    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_hp_rng, ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        let threshold = initial_threshold(ascension_level);
        vec![
            guardian_runtime_update(entity, Some(threshold), Some(0), Some(true), Some(false)),
            Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id: PowerId::ModeShift,
                amount: threshold,
            },
        ]
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match decode_turn(plan) {
            GuardianTurn::ChargeUp(block) => vec![
                gain_block_action(entity, block),
                set_next_move_action(entity, fierce_bash_plan(state.meta.ascension_level)),
            ],
            GuardianTurn::FierceBash(attack) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(set_next_move_action(entity, vent_steam_plan()));
                actions
            }
            GuardianTurn::VentSteam { weak, vulnerable } => vec![
                apply_power_action(entity, weak),
                apply_power_action(entity, vulnerable),
                set_next_move_action(entity, whirlwind_plan()),
            ],
            GuardianTurn::Whirlwind(attack) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(set_next_move_action(entity, charge_up_plan()));
                actions
            }
            GuardianTurn::CloseUp(sharp_hide) => vec![
                apply_power_action(entity, sharp_hide),
                set_next_move_action(entity, roll_attack_plan(state.meta.ascension_level)),
            ],
            GuardianTurn::RollAttack(attack) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(set_next_move_action(
                    entity,
                    twin_slam_plan(entity.guardian.damage_threshold),
                ));
                actions
            }
            GuardianTurn::TwinSlam {
                mode_shift,
                attack,
                sharp_hide,
            } => {
                let mut actions = vec![
                    guardian_runtime_update(entity, None, None, Some(true), Some(false)),
                    set_next_move_action(entity, whirlwind_plan()),
                ];
                actions.extend(attack_actions(entity.id, PLAYER, attack));
                actions.push(remove_power_action(entity, sharp_hide));
                actions.push(apply_power_action(entity, mode_shift));
                actions.push(guardian_runtime_update(entity, None, Some(0), None, None));
                if entity.block > 0 {
                    actions.push(Action::LoseBlock {
                        target: entity.id,
                        amount: entity.block,
                    });
                }
                actions
            }
        }
    }

    fn on_damaged(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        amount: i32,
    ) -> smallvec::SmallVec<[ActionInfo; 4]> {
        if amount <= 0
            || !entity.guardian.is_open
            || entity.guardian.close_up_triggered
            || entity.is_dying
            || entity.half_dead
        {
            return smallvec::smallvec![];
        }

        let new_damage_taken = entity.guardian.damage_taken + amount;
        if new_damage_taken >= entity.guardian.damage_threshold {
            let next_threshold = entity.guardian.damage_threshold + THRESHOLD_INCREASE;
            smallvec::smallvec![
                ActionInfo {
                    action: Action::GuardianModeShiftThresholdTriggered {
                        monster_id: entity.id,
                        hp_lost: amount,
                    },
                    insertion_mode: AddTo::Top,
                },
                ActionInfo {
                    action: Action::GuardianEnterDefensiveMode {
                        monster_id: entity.id,
                        next_threshold,
                    },
                    insertion_mode: AddTo::Bottom,
                }
            ]
        } else {
            smallvec::smallvec![
                ActionInfo {
                    action: Action::ReducePower {
                        target: entity.id,
                        power_id: PowerId::ModeShift,
                        amount,
                    },
                    insertion_mode: AddTo::Top,
                },
                ActionInfo {
                    action: guardian_runtime_update(
                        entity,
                        None,
                        Some(new_damage_taken),
                        None,
                        None
                    ),
                    insertion_mode: AddTo::Top,
                },
            ]
        }
    }
}
