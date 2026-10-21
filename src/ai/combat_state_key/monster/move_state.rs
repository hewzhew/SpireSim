use crate::runtime::combat::MonsterMoveState;
use crate::runtime::monster_move::{
    AddCardStep, ApplyPowerStep, AttackSpec, AttackStep, BlockStep, CardDestination, DamageKind,
    DebuffSpec, EffectStrength, HealStep, MonsterMoveSpec, MonsterTurnSteps, MoveStep, MoveTarget,
    PowerEffectKind, RandomBlockStep, RemovePowerStep, SpawnHpValue, SpawnMonsterStep,
    StealGoldStep, UpgradeCardsStep, UtilityStep,
};
pub(super) fn stable_move_state_signature(move_state: &MonsterMoveState) -> String {
    format!(
        "id{}:hist{}:steps{}:visible{}",
        move_state.planned_move_id,
        move_state
            .history
            .iter()
            .map(|move_id| move_id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        stable_turn_steps_signature(move_state.planned_steps.as_ref()),
        stable_move_spec_signature(move_state.planned_visible_spec.as_ref()),
    )
}

fn stable_turn_steps_signature(steps: Option<&MonsterTurnSteps>) -> String {
    match steps {
        Some(steps) => steps
            .iter()
            .map(stable_move_step_signature)
            .collect::<Vec<_>>()
            .join("|"),
        None => "_".to_string(),
    }
}

fn stable_move_step_signature(step: &MoveStep) -> String {
    match step {
        MoveStep::Attack(step) => format!("atk:{}", stable_attack_step_signature(step)),
        MoveStep::ApplyPower(step) => format!("pow:{}", stable_apply_power_step_signature(step)),
        MoveStep::Heal(step) => format!("heal:{}", stable_heal_step_signature(step)),
        MoveStep::GainBlock(step) => format!("blk:{}", stable_block_step_signature(step)),
        MoveStep::GainBlockRandomMonster(step) => {
            format!("blk_rand:{}", stable_random_block_step_signature(step))
        }
        MoveStep::AddCard(step) => format!("add:{}", stable_add_card_step_signature(step)),
        MoveStep::StealGold(step) => format!("steal:{}", stable_steal_gold_step_signature(step)),
        MoveStep::UpgradeCards(step) => {
            format!("upgrade:{}", stable_upgrade_cards_step_signature(step))
        }
        MoveStep::RemovePower(step) => {
            format!("rm_pow:{}", stable_remove_power_step_signature(step))
        }
        MoveStep::Utility(step) => format!("util:{}", stable_utility_step_signature(step)),
        MoveStep::SpawnMonster(step) => {
            format!("spawn:{}", stable_spawn_monster_step_signature(step))
        }
        MoveStep::Suicide => "suicide".to_string(),
        MoveStep::Charge => "charge".to_string(),
        MoveStep::Escape => "escape".to_string(),
        MoveStep::Magic => "magic".to_string(),
        MoveStep::Sleep => "sleep".to_string(),
        MoveStep::Stun => "stun".to_string(),
        MoveStep::Debug => "debug".to_string(),
        MoveStep::None => "none".to_string(),
    }
}

fn stable_move_spec_signature(spec: Option<&MonsterMoveSpec>) -> String {
    match spec {
        Some(spec) => stable_visible_move_spec_signature(spec),
        None => "_".to_string(),
    }
}

fn stable_visible_move_spec_signature(spec: &MonsterMoveSpec) -> String {
    match spec {
        MonsterMoveSpec::Attack(spec) => format!("attack:{}", stable_attack_spec_signature(spec)),
        MonsterMoveSpec::AttackAddCard(attack, add) => format!(
            "attack_add:{}:{}",
            stable_attack_spec_signature(attack),
            stable_add_card_step_signature(add)
        ),
        MonsterMoveSpec::AttackUpgradeCards(attack, upgrade) => format!(
            "attack_upgrade:{}:{}",
            stable_attack_spec_signature(attack),
            stable_upgrade_cards_step_signature(upgrade)
        ),
        MonsterMoveSpec::AttackBuff(attack, buff) => format!(
            "attack_buff:{}:{}:{}",
            stable_attack_spec_signature(attack),
            buff.power_id.stable_id(),
            buff.amount
        ),
        MonsterMoveSpec::AttackBuffIntent(attack) => {
            format!(
                "attack_buff_intent:{}",
                stable_attack_spec_signature(attack)
            )
        }
        MonsterMoveSpec::AttackDebuff(attack, debuff) => format!(
            "attack_debuff:{}:{}",
            stable_attack_spec_signature(attack),
            stable_debuff_spec_signature(debuff)
        ),
        MonsterMoveSpec::AttackDefend(attack, defend) => format!(
            "attack_defend:{}:blk{}",
            stable_attack_spec_signature(attack),
            defend.block
        ),
        MonsterMoveSpec::AddCard(add) => format!("add:{}", stable_add_card_step_signature(add)),
        MonsterMoveSpec::Buff(buff) => {
            format!("buff:{}:{}", buff.power_id.stable_id(), buff.amount)
        }
        MonsterMoveSpec::BuffIntent => "buff_intent".to_owned(),
        MonsterMoveSpec::StrongDebuffIntent => "strong_debuff_intent".to_owned(),
        MonsterMoveSpec::Debuff(debuff) => {
            format!("debuff:{}", stable_debuff_spec_signature(debuff))
        }
        MonsterMoveSpec::StrongDebuff(debuff) => {
            format!("strong_debuff:{}", stable_debuff_spec_signature(debuff))
        }
        MonsterMoveSpec::Defend(defend) => format!("defend:blk{}", defend.block),
        MonsterMoveSpec::DefendDebuff(defend, debuff) => format!(
            "defend_debuff:blk{}:{}",
            defend.block,
            stable_debuff_spec_signature(debuff)
        ),
        MonsterMoveSpec::DefendBuff(defend, buff) => {
            format!(
                "defend_buff:blk{}:{}:{}",
                defend.block,
                buff.power_id.stable_id(),
                buff.amount
            )
        }
        MonsterMoveSpec::Heal(heal) => {
            format!("heal:{}:{}", move_target_key(heal.target), heal.amount)
        }
        MonsterMoveSpec::Escape => "escape".to_string(),
        MonsterMoveSpec::Magic => "magic".to_string(),
        MonsterMoveSpec::Sleep => "sleep".to_string(),
        MonsterMoveSpec::Stun => "stun".to_string(),
        MonsterMoveSpec::Debug => "debug".to_string(),
        MonsterMoveSpec::None => "none".to_string(),
        MonsterMoveSpec::Unknown => "unknown".to_string(),
    }
}

fn stable_attack_spec_signature(spec: &AttackSpec) -> String {
    format!(
        "d{}:h{}:{}",
        spec.base_damage,
        spec.hits,
        damage_kind_key(spec.damage_kind)
    )
}

fn stable_attack_step_signature(step: &AttackStep) -> String {
    format!(
        "{}:{}",
        move_target_key(step.target),
        stable_attack_spec_signature(&step.attack)
    )
}

fn stable_apply_power_step_signature(step: &ApplyPowerStep) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        move_target_key(step.target),
        step.power_id.stable_id(),
        step.amount,
        power_effect_kind_key(step.effect),
        effect_strength_key(step.visible_strength)
    )
}

fn stable_heal_step_signature(step: &HealStep) -> String {
    format!("{}:{}", move_target_key(step.target), step.amount)
}

fn stable_block_step_signature(step: &BlockStep) -> String {
    format!("{}:{}", move_target_key(step.target), step.amount)
}

fn stable_random_block_step_signature(step: &RandomBlockStep) -> String {
    step.amount.to_string()
}

fn stable_add_card_step_signature(step: &AddCardStep) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        step.card_id.stable_id(),
        step.amount,
        step.upgraded,
        card_destination_key(step.destination),
        effect_strength_key(step.visible_strength)
    )
}

fn stable_steal_gold_step_signature(step: &StealGoldStep) -> String {
    step.amount.to_string()
}

fn stable_upgrade_cards_step_signature(step: &UpgradeCardsStep) -> String {
    step.card_id.stable_id().to_owned()
}

fn stable_remove_power_step_signature(step: &RemovePowerStep) -> String {
    format!(
        "{}:{}",
        move_target_key(step.target),
        step.power_id.stable_id()
    )
}

fn stable_utility_step_signature(step: &UtilityStep) -> String {
    match step {
        UtilityStep::RemoveAllDebuffs { target } => {
            format!("rm_all_debuffs:{}", move_target_key(*target))
        }
    }
}

fn stable_spawn_monster_step_signature(step: &SpawnMonsterStep) -> String {
    format!(
        "{}:off{}:drawx{}:hp{}:{}:minion{}",
        step.enemy_id.stable_id(),
        step.logical_position_offset,
        serde_json::to_string(&step.protocol_draw_x_offset)
            .expect("spawn draw offset should serialize"),
        stable_spawn_hp_value_signature(step.hp.current),
        stable_spawn_hp_value_signature(step.hp.max),
        step.is_minion,
    )
}

fn stable_spawn_hp_value_signature(value: SpawnHpValue) -> String {
    match value {
        SpawnHpValue::Rolled => "rolled".to_string(),
        SpawnHpValue::Fixed(value) => format!("fixed{value}"),
        SpawnHpValue::SourceCurrentHp => "source_cur".to_string(),
        SpawnHpValue::SourceMaxHp => "source_max".to_string(),
    }
}

fn stable_debuff_spec_signature(spec: &DebuffSpec) -> String {
    format!(
        "{}:{}:{}",
        spec.power_id.stable_id(),
        spec.amount,
        effect_strength_key(spec.strength)
    )
}

fn move_target_key(target: MoveTarget) -> &'static str {
    match target {
        MoveTarget::Player => "player",
        MoveTarget::SelfTarget => "self",
        MoveTarget::AllMonsters => "all_monsters",
    }
}

fn damage_kind_key(kind: DamageKind) -> &'static str {
    match kind {
        DamageKind::Normal => "normal",
        DamageKind::HpLoss => "hp_loss",
        DamageKind::Thorns => "thorns",
        DamageKind::Unknown => "unknown",
    }
}

fn power_effect_kind_key(kind: PowerEffectKind) -> &'static str {
    match kind {
        PowerEffectKind::Buff => "buff",
        PowerEffectKind::Debuff => "debuff",
    }
}

fn effect_strength_key(strength: EffectStrength) -> &'static str {
    match strength {
        EffectStrength::Normal => "normal",
        EffectStrength::Strong => "strong",
    }
}

fn card_destination_key(destination: CardDestination) -> &'static str {
    match destination {
        CardDestination::Hand => "hand",
        CardDestination::Discard => "discard",
        CardDestination::DrawPileTop => "draw_pile_top",
        CardDestination::DrawPileRandom => "draw_pile_random",
    }
}
