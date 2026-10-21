pub mod acid_slime;
pub mod cultist;
pub mod fungi_beast;
pub mod gremlin_fat;
pub mod gremlin_nob;
pub mod gremlin_thief;
pub mod gremlin_tsundere;
pub mod gremlin_warrior;
pub mod gremlin_wizard;
pub mod hexaghost;
pub mod jaw_worm;
pub mod lagavulin;
pub mod looter;
pub mod louse_defensive;
pub mod louse_normal;
pub mod sentry;
pub mod slaver_blue;
pub mod slaver_red;
pub mod slime_boss;
pub mod spike_slime;
pub mod the_guardian;

use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::MonsterEntity;
use crate::semantics::combat::{
    AddCardStep, ApplyPowerStep, AttackSpec, BlockStep, CardDestination, MonsterTurnPlan,
    MoveTarget, RandomBlockStep, RemovePowerStep, SpawnHpSpec, SpawnHpValue, SpawnMonsterStep,
    StealGoldStep, UpgradeCardsStep, UtilityStep,
};
use crate::EntityId;

pub const PLAYER: EntityId = 0;

/// Inclusive constructor ranges shared by spawn and public definitions.
pub(crate) fn louse_bite_damage_range(asc: u8) -> (i32, i32) {
    if asc >= 2 {
        (6, 8)
    } else {
        (5, 7)
    }
}

pub(crate) fn louse_curl_up_range(asc: u8) -> (i32, i32) {
    if asc >= 17 {
        (9, 12)
    } else if asc >= 7 {
        (4, 8)
    } else {
        (3, 7)
    }
}

fn louse_mechanics(
    asc: u8,
    moves: Vec<(&'static str, MonsterTurnPlan)>,
) -> super::mechanics::MonsterMechanics {
    let (min, max) = louse_bite_damage_range(asc);
    let (curl_min, curl_max) = louse_curl_up_range(asc);
    super::mechanics::MonsterMechanics {
        moves,
        special_moves: vec![
            ("Bite", format!("attack player once; base damage is sampled once at spawn from {min}..{max} inclusive and stays fixed for this monster, before Strength and other damage modifiers. Use the current visible intent for the actual modified damage.")),
            ("Starting Curl Up", format!("sampled once from {curl_min}..{curl_max} inclusive. {}", crate::content::powers::mechanics::get(crate::content::powers::PowerId::CurlUp).unwrap().effect)),
        ],
        notes: if asc >= 17 {
            "Randomly rolls Bite or its non-attack move, subject to history: cannot Bite three times consecutively; the non-attack move cannot repeat immediately."
        } else {
            "Randomly rolls Bite or its non-attack move, subject to history: neither move can occur three times consecutively."
        },
    }
}
pub(crate) fn initialize_large_slime_runtime_state(entity: &mut MonsterEntity) {
    entity.large_slime.protocol_seeded = true;
    entity.large_slime.split_triggered = false;
}

pub(crate) fn large_slime_split_triggered(entity: &MonsterEntity) -> bool {
    assert!(
        entity.large_slime.protocol_seeded,
        "large slime splitTriggered truth must be protocol-seeded or factory-seeded"
    );
    entity.large_slime.split_triggered
}

pub(crate) fn mark_large_slime_split_triggered_action(entity: &MonsterEntity) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::LargeSlime {
            split_triggered: Some(true),
            protocol_seeded: Some(true),
        },
    }
}

/// Builds the canonical action sequence for a simple monster attack.
///
/// This is the only helper simple monster files should use for ordinary attacks.
/// It forwards plan truth from `AttackSpec` into `Action::MonsterAttack` without
/// trying to predict modified output damage.
pub fn attack_actions(source: EntityId, target: EntityId, attack: &AttackSpec) -> Vec<Action> {
    let damage = Action::MonsterAttack {
        source,
        target,
        base_damage: attack.base_damage,
        damage_kind: attack.damage_kind,
    };
    std::iter::repeat_n(damage, attack.hits.max(1) as usize).collect()
}

pub fn add_card_action(step: &AddCardStep) -> Action {
    match step.destination {
        CardDestination::Hand => Action::MakeTempCardInHand {
            card_id: step.card_id,
            amount: step.amount,
            upgraded: step.upgraded,
        },
        CardDestination::Discard => Action::MakeTempCardInDiscard {
            card_id: step.card_id,
            amount: step.amount,
            upgraded: step.upgraded,
        },
        CardDestination::DrawPileTop => Action::MakeTempCardInDrawPile {
            card_id: step.card_id,
            amount: step.amount,
            random_spot: false,
            to_bottom: false,
            upgraded: step.upgraded,
        },
        CardDestination::DrawPileRandom => Action::MakeTempCardInDrawPile {
            card_id: step.card_id,
            amount: step.amount,
            random_spot: true,
            to_bottom: false,
            upgraded: step.upgraded,
        },
    }
}

pub fn steal_gold_action(source: &MonsterEntity, step: &StealGoldStep) -> Action {
    Action::StealPlayerGold {
        thief_id: source.id,
        amount: step.amount,
    }
}

fn resolve_target(source: &MonsterEntity, target: MoveTarget) -> EntityId {
    match target {
        MoveTarget::Player => PLAYER,
        MoveTarget::SelfTarget => source.id,
        MoveTarget::AllMonsters => {
            panic!("group target requires explicit monster-by-monster expansion")
        }
    }
}

pub fn apply_power_action(source: &MonsterEntity, step: &ApplyPowerStep) -> Action {
    Action::ApplyPower {
        source: source.id,
        target: resolve_target(source, step.target),
        power_id: step.power_id,
        amount: step.amount,
    }
}

pub fn gain_block_action(source: &MonsterEntity, step: &BlockStep) -> Action {
    Action::GainBlock {
        target: resolve_target(source, step.target),
        amount: step.amount,
    }
}

pub fn gain_block_random_monster_action(source: &MonsterEntity, step: &RandomBlockStep) -> Action {
    Action::GainBlockRandomMonster {
        source: source.id,
        amount: step.amount,
    }
}

pub fn remove_power_action(source: &MonsterEntity, step: &RemovePowerStep) -> Action {
    Action::RemovePower {
        target: resolve_target(source, step.target),
        power_id: step.power_id,
    }
}

pub fn utility_action(source: &MonsterEntity, step: &UtilityStep) -> Action {
    match step {
        UtilityStep::RemoveAllDebuffs { target } => Action::RemoveAllDebuffs {
            target: resolve_target(source, *target),
        },
    }
}

pub fn upgrade_cards_action(step: &UpgradeCardsStep) -> Action {
    match step.card_id {
        crate::content::cards::CardId::Burn => Action::UpgradeAllBurns,
        card_id => panic!(
            "unsupported upgrade-cards step for monster execution: {:?}",
            card_id
        ),
    }
}

pub fn set_next_move_action(source: &MonsterEntity, plan: MonsterTurnPlan) -> Action {
    Action::SetMonsterMove {
        monster_id: source.id,
        next_move_byte: plan.move_id,
        planned_steps: plan.steps,
        planned_visible_spec: plan.visible_spec,
    }
}

pub fn spawn_action(
    state: &crate::runtime::combat::CombatState,
    source: &MonsterEntity,
    step: &SpawnMonsterStep,
) -> Action {
    let resolve_hp = |value: SpawnHpValue| match value {
        SpawnHpValue::Rolled => SpawnHpValue::Rolled,
        SpawnHpValue::Fixed(hp) => SpawnHpValue::Fixed(hp.max(0)),
        SpawnHpValue::SourceCurrentHp => SpawnHpValue::Fixed(source.current_hp.max(0)),
        SpawnHpValue::SourceMaxHp => SpawnHpValue::Fixed(source.max_hp.max(0)),
    };
    let logical_position = source.logical_position + step.logical_position_offset;
    // Keep constructor coordinates through successive splits. Logical left/right
    // offsets can coincide across siblings and are too coarse for Java's
    // SpawnMonsterAction ordering (which includes dead monsters).
    let protocol_draw_x = step.protocol_draw_x_offset.map(|offset| {
        state
            .monster_protocol_identity(source.id)
            .and_then(|identity| identity.draw_x)
            .unwrap_or(source.logical_position)
            + offset
    });

    Action::SpawnMonsterSmart {
        monster_id: step.enemy_id,
        logical_position,
        hp: SpawnHpSpec {
            current: resolve_hp(step.hp.current),
            max: resolve_hp(step.hp.max),
        },
        protocol_draw_x,
        is_minion: step.is_minion,
    }
}
