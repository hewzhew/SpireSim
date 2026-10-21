use crate::content::monsters::exordium::{attack_actions, PLAYER};
use crate::content::monsters::{EnemyId, MonsterBehavior, MonsterRollContext};
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::runtime::rng::StsRng;
use crate::semantics::combat::{
    AttackSpec, BuffSpec, DamageKind, DefendSpec, MonsterMoveSpec, MonsterTurnPlan, SpawnHpSpec,
    SpawnHpValue,
};

pub struct GremlinLeader;

const RALLY: u8 = 2;
const ENCOURAGE: u8 = 3;
const STAB: u8 = 4;
const STAB_DAMAGE: i32 = 6;
const STAB_HITS: u8 = 3;
pub const SUMMON_POOL: [EnemyId; 8] = [
    EnemyId::GremlinWarrior,
    EnemyId::GremlinWarrior,
    EnemyId::GremlinThief,
    EnemyId::GremlinThief,
    EnemyId::GremlinFat,
    EnemyId::GremlinFat,
    EnemyId::GremlinTsundere,
    EnemyId::GremlinWizard,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::runtime::combat::Intent;

    #[test]
    fn natural_rally_uses_constructor_slots_despite_wizard_offsets_and_corpses() {
        use crate::engine::action_handlers::spawning::handle_spawn_gremlin_leader_minion;
        let mut thief = crate::test_support::test_monster(EnemyId::GremlinThief);
        thief.id = 1;
        thief.current_hp = 0;
        thief.is_dying = true;
        thief.logical_position = -366;
        let mut wizard = crate::test_support::test_monster(EnemyId::GremlinWizard);
        wizard.id = 2;
        wizard.current_hp = 0;
        wizard.is_dying = true;
        wizard.logical_position = -205; // POSX[1] - 35 in GremlinWizard's constructor.
        let mut leader = crate::test_support::test_monster(EnemyId::GremlinLeader);
        leader.id = 3;
        leader.logical_position = 35;
        leader.gremlin_leader.gremlin_slots = [Some(1), Some(2), None];
        let mut state = crate::test_support::combat_with_monsters(vec![thief, wizard, leader]);
        for (slot, hp) in [(0, 13), (1, 12)] {
            let anchors = GremlinLeader::gremlin_slot_draw_xs(&state, 3);
            assert_eq!(anchors, [-366, -170, -532]);
            let x = GremlinLeader::protocol_draw_x_for_minion(anchors, slot, EnemyId::GremlinThief);
            handle_spawn_gremlin_leader_minion(
                3,
                slot as u8,
                EnemyId::GremlinThief,
                x,
                SpawnHpSpec {
                    current: SpawnHpValue::Fixed(hp),
                    max: SpawnHpValue::Fixed(hp),
                },
                Some(x),
                &mut state,
            );
        }
        assert_eq!(
            state
                .entities
                .monsters
                .iter()
                .map(|m| (m.enemy_id, m.current_hp, m.logical_position))
                .collect::<Vec<_>>(),
            vec![
                (EnemyId::GremlinThief, 13, -366),
                (EnemyId::GremlinThief, 0, -366),
                (EnemyId::GremlinWizard, 0, -205),
                (EnemyId::GremlinThief, 12, -170),
                (
                    EnemyId::GremlinLeader,
                    state.entities.monsters[4].current_hp,
                    35
                ),
            ]
        );
        // Later summons must not reinterpret these four bodies as new anchors.
        assert_eq!(
            GremlinLeader::gremlin_slot_draw_xs(&state, 3),
            [-366, -170, -532]
        );
        assert_eq!(
            GremlinLeader::natural_minion_draw_x(2, EnemyId::GremlinWizard),
            -567
        );
    }

    #[test]
    fn rally_keeps_original_unknown_icon_and_defers_two_summons_to_execution() {
        let plan = rally_plan();

        assert!(plan.steps.is_empty());
        assert_eq!(plan.summary_spec(), MonsterMoveSpec::Unknown);
        assert_eq!(
            Intent::from_visible_move_spec(&plan.summary_spec()),
            Intent::Unknown
        );
        let leader = crate::test_support::test_monster(EnemyId::GremlinLeader);
        let mut state = crate::test_support::combat_with_monsters(vec![leader.clone()]);
        let actions = GremlinLeader::take_turn_plan(&mut state, &leader, &plan);
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, Action::SpawnGremlinLeaderMinion { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn alive_gremlin_count_counts_zero_hp_non_dying_minions_like_java() {
        let leader = crate::test_support::test_monster(EnemyId::GremlinLeader);
        let mut minion = crate::test_support::test_monster(EnemyId::GremlinWarrior);
        minion.id = 2;
        minion.current_hp = 0;
        minion.is_dying = false;
        minion.is_escaped = true;

        assert_eq!(alive_gremlin_count(&[leader.clone(), minion], leader.id), 1);
    }

    #[test]
    fn encourage_burns_java_quote_rng_and_targets_zero_hp_non_dying_allies() {
        let leader = crate::test_support::test_monster(EnemyId::GremlinLeader);
        let mut minion = crate::test_support::test_monster(EnemyId::GremlinWarrior);
        minion.id = 2;
        minion.current_hp = 0;
        minion.is_dying = false;
        minion.is_escaped = true;
        let mut state = crate::test_support::combat_with_monsters(vec![leader.clone(), minion]);
        let before_counter = state.rng.ai_rng.counter;
        let plan = encourage_plan(state.meta.ascension_level);
        let trace = crate::runtime::rng::begin_sts_rng_trace().unwrap();

        let actions = GremlinLeader::take_turn_plan(&mut state, &leader, &plan);
        let calls = trace.finish().unwrap();
        assert!(matches!(
            calls.as_slice(),
            [crate::runtime::rng::StsRngTraceEvent {
                call: crate::runtime::rng::StsRngCall::RandomIntRange {
                    start: 0,
                    end: 2,
                    ..
                },
                ..
            }]
        ));

        assert_eq!(
            state.rng.ai_rng.counter,
            before_counter + 1,
            "Java GremlinLeader.getEncourageQuote consumes aiRng even though the quote is UI text"
        );
        assert!(
            actions.iter().any(|action| matches!(
                action,
                Action::ApplyPower {
                    target: 2,
                    power_id: PowerId::Strength,
                    ..
                }
            )),
            "Java Encourage applies Strength to every non-dying ally, without checking HP or escaped state"
        );
        assert!(
            actions.iter().any(|action| matches!(
                action,
                Action::GainBlock {
                    target: 2,
                    ..
                }
            )),
            "Java Encourage applies block to every non-dying ally, without checking HP or escaped state"
        );
    }

    #[test]
    fn leader_death_escapes_zero_hp_non_dying_allies_like_java() {
        let leader = crate::test_support::test_monster(EnemyId::GremlinLeader);
        let mut minion = crate::test_support::test_monster(EnemyId::GremlinWarrior);
        minion.id = 2;
        minion.current_hp = 0;
        minion.is_dying = false;
        minion.is_escaped = true;
        let mut state = crate::test_support::combat_with_monsters(vec![leader.clone(), minion]);

        let actions = GremlinLeader::on_death(&mut state, &leader);

        assert!(matches!(actions.as_slice(), [Action::Escape { target: 2 }]));
    }

    #[test]
    fn rally_uses_java_gremlins_slots_not_position_inference() {
        let mut leader = crate::test_support::test_monster(EnemyId::GremlinLeader);
        leader.gremlin_leader.gremlin_slots = [Some(2), None, None];
        let mut slot_zero = crate::test_support::test_monster(EnemyId::GremlinWarrior);
        slot_zero.id = 2;
        slot_zero.logical_position = GremlinLeader::GREMLIN_SLOT_LOGICAL_POSITIONS[0];
        let mut unrelated_alive_gremlin = crate::test_support::test_monster(EnemyId::GremlinFat);
        unrelated_alive_gremlin.id = 3;
        unrelated_alive_gremlin.logical_position = GremlinLeader::GREMLIN_SLOT_LOGICAL_POSITIONS[1];
        let mut state = crate::test_support::combat_with_monsters(vec![
            slot_zero,
            unrelated_alive_gremlin,
            leader.clone(),
        ]);

        let actions = GremlinLeader::take_turn_plan(&mut state, &leader, &rally_plan());

        assert!(
            actions.iter().any(|action| matches!(
                action,
                Action::SpawnGremlinLeaderMinion { slot: 1, .. }
            )),
            "Java SummonGremlinAction identifies the first empty GremlinLeader.gremlins slot, not the nearest occupied draw_x"
        );
    }

    #[test]
    fn rally_constructs_each_kind_and_hp_before_any_initial_move_roll() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};

        let leader = crate::test_support::test_monster(EnemyId::GremlinLeader);
        let mut state = crate::test_support::combat_with_monsters(vec![leader.clone()]);
        let trace = begin_sts_rng_trace().unwrap();
        let actions = GremlinLeader::take_turn_plan(&mut state, &leader, &rally_plan());
        let constructed = trace.finish().unwrap();
        assert_eq!(constructed.len(), 4);
        for pair in constructed.chunks_exact(2) {
            assert!(matches!(
                pair[0].call,
                StsRngCall::RandomIntRange {
                    start: 0,
                    end: 7,
                    ..
                }
            ));
            assert_eq!(pair[0].owner, Some("floor.ai"));
            assert_eq!(pair[1].owner, Some("floor.monster_hp"));
        }
        let hp_counter = state.rng.monster_hp_rng.counter;
        for action in actions {
            if let Action::SpawnGremlinLeaderMinion { hp, .. } = action {
                assert!(
                    matches!((hp.current, hp.max), (SpawnHpValue::Fixed(current), SpawnHpValue::Fixed(max)) if current == max && current > 0)
                );
            }
            crate::engine::action_handlers::execute_action(action, &mut state);
        }
        assert_eq!(
            state.rng.monster_hp_rng.counter, hp_counter,
            "spawn update must not reroll constructor HP"
        );
        assert_eq!(state.entities.monsters.len(), 3);
    }

    #[test]
    fn pre_battle_applies_minion_sentinel_amount_like_java() {
        let mut first = crate::test_support::test_monster(EnemyId::GremlinWarrior);
        first.id = 2;
        let mut second = crate::test_support::test_monster(EnemyId::GremlinFat);
        second.id = 3;
        let leader = crate::test_support::test_monster(EnemyId::GremlinLeader);
        let mut state =
            crate::test_support::combat_with_monsters(vec![first, second, leader.clone()]);

        let actions = GremlinLeader::use_pre_battle_actions(
            &mut state,
            &leader,
            crate::content::monsters::PreBattleLegacyRng::MonsterHp,
        );

        assert_eq!(
            actions,
            vec![
                Action::ApplyPower {
                    source: 2,
                    target: 2,
                    power_id: PowerId::Minion,
                    amount: -1,
                },
                Action::ApplyPower {
                    source: 3,
                    target: 3,
                    power_id: PowerId::Minion,
                    amount: -1,
                },
            ],
            "Java ApplyPowerAction uses the minion as source and target, while MinionPower amount remains the sentinel -1"
        );
    }

    #[test]
    fn encourage_queue_order_matches_java_loop() {
        let mut leader = crate::test_support::test_monster(EnemyId::GremlinLeader);
        leader.id = 10;
        let mut first = crate::test_support::test_monster(EnemyId::GremlinWarrior);
        first.id = 2;
        let mut dying = crate::test_support::test_monster(EnemyId::GremlinFat);
        dying.id = 3;
        dying.is_dying = true;
        let mut second = crate::test_support::test_monster(EnemyId::GremlinThief);
        second.id = 4;
        let mut state =
            crate::test_support::combat_with_monsters(vec![first, leader.clone(), dying, second]);
        state.meta.ascension_level = 18;

        let actions = GremlinLeader::take_turn_plan(&mut state, &leader, &encourage_plan(18));

        assert_eq!(
            actions,
            vec![
                Action::ApplyPower {
                    source: 10,
                    target: 10,
                    power_id: PowerId::Strength,
                    amount: 5,
                },
                Action::ApplyPower {
                    source: 10,
                    target: 2,
                    power_id: PowerId::Strength,
                    amount: 5,
                },
                Action::GainBlock {
                    target: 2,
                    amount: 10,
                },
                Action::ApplyPower {
                    source: 10,
                    target: 4,
                    power_id: PowerId::Strength,
                    amount: 5,
                },
                Action::GainBlock {
                    target: 4,
                    amount: 10,
                },
                Action::RollMonsterMove { monster_id: 10 },
            ],
            "Java Encourage buffs the leader first, skips only isDying allies, then applies Strength and block in monster-list order before RollMoveAction"
        );
    }

    #[test]
    fn stab_queues_three_hits_before_roll_like_java() {
        let leader = crate::test_support::test_monster(EnemyId::GremlinLeader);
        let mut state = crate::test_support::combat_with_monsters(vec![leader.clone()]);

        let actions = GremlinLeader::take_turn_plan(&mut state, &leader, &stab_plan());

        assert_eq!(actions.len(), 4);
        assert!(
            actions[..3]
                .iter()
                .all(|action| matches!(action, Action::MonsterAttack { .. })),
            "Java STAB queues three DamageActions before RollMoveAction"
        );
        assert_eq!(
            actions[3],
            Action::RollMonsterMove {
                monster_id: leader.id,
            }
        );
    }
}

fn stab_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        STAB,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: STAB_DAMAGE,
            hits: STAB_HITS,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn rally_plan() -> MonsterTurnPlan {
    // Java getMove uses Intent.UNKNOWN for Rally. The two summoned identities
    // are still chosen from aiRng during takeTurn, behind the visible icon.
    MonsterTurnPlan::with_visible_spec(RALLY, smallvec::smallvec![], MonsterMoveSpec::Unknown)
}

fn encourage_strength(ascension_level: u8) -> i32 {
    if ascension_level >= 18 {
        5
    } else if ascension_level >= 3 {
        4
    } else {
        3
    }
}

fn encourage_block(ascension_level: u8) -> i32 {
    if ascension_level >= 18 {
        10
    } else {
        6
    }
}

fn encourage_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        ENCOURAGE,
        smallvec::smallvec![],
        MonsterMoveSpec::DefendBuff(
            DefendSpec {
                block: encourage_block(ascension_level),
            },
            BuffSpec {
                power_id: PowerId::Strength,
                amount: encourage_strength(ascension_level),
            },
        ),
    )
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![("Stab", stab_plan())],
        special_moves: vec![
            ("Encourage", format!("Gain {} Strength for self. At execution, also give every other non-dying ally {} Strength and {} Block. The leader receives the Strength; Block goes to the allies. Visible icon: DefendBuff.", encourage_strength(asc), encourage_strength(asc), encourage_block(asc))),
            ("Rally", format!("Summon up to 2 gremlins into vacant minion slots, with at most 3 occupied slots. Each identity is sampled at execution from {} equally weighted pool entries: {}. Repeated entries carry repeated weight. Visible icon: Unknown.", SUMMON_POOL.len(), SUMMON_POOL.iter().map(|id| crate::content::identity::gameplay_display_label_v1(*id)).collect::<Vec<_>>().join(", "))),
        ],
        notes: "Starts with two random minions. With zero living allies, selection chooses Rally or Stab; with one, Rally/Encourage/Stab; with at least two, Encourage/Stab. Selection depends on living allies and the previous move, with rerolls when a repeated move is excluded. Allies present when Encourage executes receive its effects. When the leader dies, other non-dying allies escape.",
    }
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        RALLY => rally_plan(),
        ENCOURAGE => encourage_plan(ascension_level),
        STAB => stab_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

fn last_move(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().back().copied() == Some(move_id)
}

fn alive_gremlin_count(monsters: &[MonsterEntity], leader_id: usize) -> usize {
    monsters
        .iter()
        .filter(|monster| monster.id != leader_id && !monster.is_dying)
        .count()
}

fn choose_move(rng: &mut StsRng, entity: &MonsterEntity, alive_gremlins: usize, num: i32) -> u8 {
    if alive_gremlins == 0 {
        if num < 75 {
            if !last_move(entity, RALLY) {
                RALLY
            } else {
                STAB
            }
        } else if !last_move(entity, STAB) {
            STAB
        } else {
            RALLY
        }
    } else if alive_gremlins < 2 {
        if num < 50 {
            if !last_move(entity, RALLY) {
                RALLY
            } else {
                let reroll = rng.random_range(50, 99);
                choose_move(rng, entity, alive_gremlins, reroll)
            }
        } else if num < 80 {
            if !last_move(entity, ENCOURAGE) {
                ENCOURAGE
            } else {
                STAB
            }
        } else if !last_move(entity, STAB) {
            STAB
        } else {
            let reroll = rng.random_range(0, 80);
            choose_move(rng, entity, alive_gremlins, reroll)
        }
    } else if num < 66 {
        if !last_move(entity, ENCOURAGE) {
            ENCOURAGE
        } else {
            STAB
        }
    } else if !last_move(entity, STAB) {
        STAB
    } else {
        ENCOURAGE
    }
}

impl GremlinLeader {
    pub const GREMLIN_SLOT_DRAW_X: [i32; 3] = [-366, -170, -532];
    pub const GREMLIN_SLOT_LOGICAL_POSITIONS: [i32; 3] = Self::GREMLIN_SLOT_DRAW_X;
    pub const LEADER_LOGICAL_POSITION: i32 = 35;

    pub(crate) fn natural_minion_draw_x(slot: usize, monster_id: EnemyId) -> i32 {
        Self::protocol_draw_x_for_minion(Self::GREMLIN_SLOT_DRAW_X, slot, monster_id)
    }

    fn gremlin_slot_draw_xs(state: &CombatState, leader_id: usize) -> [i32; 3] {
        // Natural runs use Java constructor coordinates. Corpses and newly
        // spawned Wizards have enemy-specific offsets, so their occupied positions
        // cannot be used to reconstruct GremlinLeader.POSX.
        if state
            .monster_protocol_identity(leader_id)
            .and_then(|identity| identity.draw_x)
            .is_none()
        {
            return Self::GREMLIN_SLOT_DRAW_X;
        }
        // Live snapshots store absolute draw_x values, while encounter templates use
        // Java constructor offsets. Reuse existing gremlin anchors so newly summoned
        // minions sort into the same coordinate frame as dead slot occupants.
        let mut positions: Vec<i32> = state
            .entities
            .monsters
            .iter()
            .filter(|monster| {
                monster.id != leader_id
                    && matches!(
                        monster.enemy_id,
                        EnemyId::GremlinWarrior
                            | EnemyId::GremlinThief
                            | EnemyId::GremlinFat
                            | EnemyId::GremlinTsundere
                            | EnemyId::GremlinWizard
                    )
            })
            .map(|monster| {
                state
                    .monster_protocol_identity(monster.id)
                    .and_then(|identity| identity.draw_x)
                    .unwrap_or(monster.logical_position)
            })
            .collect();
        positions.sort_unstable();
        positions.dedup();

        let mut slot_draw_xs = Self::GREMLIN_SLOT_DRAW_X;
        match positions.as_slice() {
            [slot_2, slot_0, slot_1, ..] => {
                slot_draw_xs[0] = *slot_0;
                slot_draw_xs[1] = *slot_1;
                slot_draw_xs[2] = *slot_2;
            }
            [slot_0, slot_1] => {
                slot_draw_xs[0] = *slot_0;
                slot_draw_xs[1] = *slot_1;
                let gap = slot_1 - slot_0;
                if gap > 0 {
                    // Java POSX deltas: slot0-slot2=166, slot1-slot0=196.
                    slot_draw_xs[2] = slot_0 - (gap * 166 / 196);
                }
            }
            [slot_0] => {
                slot_draw_xs[0] = *slot_0;
            }
            [] => {}
        }
        slot_draw_xs
    }

    fn protocol_draw_x_for_minion(slot_draw_xs: [i32; 3], slot: usize, monster_id: EnemyId) -> i32 {
        let slot_draw_x = slot_draw_xs[slot];
        match monster_id {
            EnemyId::GremlinWizard => slot_draw_x - 35,
            _ => slot_draw_x,
        }
    }

    fn occupied_summon_slots(state: &CombatState, leader_id: usize) -> [bool; 3] {
        let Some(leader) = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == leader_id)
        else {
            return [true, true, true];
        };
        assert!(
            leader.gremlin_leader.protocol_seeded,
            "gremlin leader slot truth must be protocol-seeded or factory-seeded"
        );
        let mut occupied = [false; 3];
        for (slot, monster_id) in leader.gremlin_leader.gremlin_slots.iter().enumerate() {
            occupied[slot] = monster_id
                .and_then(|monster_id| {
                    state
                        .entities
                        .monsters
                        .iter()
                        .find(|monster| monster.id == monster_id)
                })
                .is_some_and(|monster| !monster.is_dying);
        }
        occupied
    }

    fn living_ally_ids(state: &CombatState, leader_id: usize) -> Vec<usize> {
        state
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.id != leader_id && !monster.is_dying)
            .map(|monster| monster.id)
            .collect()
    }

    fn random_summoned_gremlin(rng: &mut StsRng) -> EnemyId {
        SUMMON_POOL[rng.random_range(0, (SUMMON_POOL.len() - 1) as i32) as usize]
    }

    fn roll_move_custom_plan(
        rng: &mut StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
        monsters: &[MonsterEntity],
    ) -> MonsterTurnPlan {
        let move_id = choose_move(rng, entity, alive_gremlin_count(monsters, entity.id), num);
        plan_for(move_id, ascension_level)
    }
}

impl MonsterBehavior for GremlinLeader {
    fn roll_move_plan_with_context(
        rng: &mut StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
        ctx: MonsterRollContext<'_>,
    ) -> MonsterTurnPlan {
        Self::roll_move_custom_plan(rng, entity, ascension_level, num, ctx.monsters)
    }

    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        _legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        Self::living_ally_ids(state, entity.id)
            .into_iter()
            .map(|ally_id| Action::ApplyPower {
                source: ally_id,
                target: ally_id,
                power_id: PowerId::Minion,
                amount: -1,
            })
            .collect()
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match plan.move_id {
            RALLY => {
                let mut occupied_slots = Self::occupied_summon_slots(state, entity.id);
                let slot_draw_xs = Self::gremlin_slot_draw_xs(state, entity.id);
                let mut actions = Vec::new();
                for _ in 0..2 {
                    let Some(slot) = occupied_slots.iter().position(|occupied| !occupied) else {
                        break;
                    };
                    occupied_slots[slot] = true;
                    let monster_id = Self::random_summoned_gremlin(&mut state.rng.ai_rng);
                    // SummonGremlinAction constructs its monster immediately:
                    // identity and HP are rolled before constructing the next
                    // summon. Only init()/the first move wait for action update.
                    let (hp_min, hp_max) = crate::content::monsters::get_hp_range(
                        monster_id,
                        state.meta.ascension_level,
                    );
                    let hp = state.rng.monster_hp_rng.random_range(hp_min, hp_max);
                    let draw_x = Self::protocol_draw_x_for_minion(slot_draw_xs, slot, monster_id);
                    actions.push(Action::SpawnGremlinLeaderMinion {
                        leader_id: entity.id,
                        slot: slot as u8,
                        monster_id,
                        logical_position: draw_x,
                        hp: SpawnHpSpec {
                            current: SpawnHpValue::Fixed(hp),
                            max: SpawnHpValue::Fixed(hp),
                        },
                        protocol_draw_x: Some(draw_x),
                    });
                }
                actions
            }
            ENCOURAGE => {
                let _quote_idx = state.rng.ai_rng.random_range(0, 2);
                let mut actions = vec![Action::ApplyPower {
                    source: entity.id,
                    target: entity.id,
                    power_id: PowerId::Strength,
                    amount: encourage_strength(state.meta.ascension_level),
                }];
                for ally_id in Self::living_ally_ids(state, entity.id) {
                    actions.push(Action::ApplyPower {
                        source: entity.id,
                        target: ally_id,
                        power_id: PowerId::Strength,
                        amount: encourage_strength(state.meta.ascension_level),
                    });
                    actions.push(Action::GainBlock {
                        target: ally_id,
                        amount: encourage_block(state.meta.ascension_level),
                    });
                }
                actions
            }
            STAB => attack_actions(
                entity.id,
                PLAYER,
                &AttackSpec {
                    base_damage: STAB_DAMAGE,
                    hits: STAB_HITS,
                    damage_kind: DamageKind::Normal,
                },
            ),
            _ => panic!(
                "gremlin leader take_turn received unsupported move {}",
                plan.move_id
            ),
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }

    fn on_death(state: &mut CombatState, entity: &MonsterEntity) -> Vec<Action> {
        state
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.id != entity.id && !monster.is_dying)
            .map(|monster| Action::Escape { target: monster.id })
            .collect()
    }
}
