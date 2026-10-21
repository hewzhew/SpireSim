use crate::engine::action_handlers::{cards, damage, powers, stances};

use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

pub(super) fn try_execute(action: Action, state: &mut CombatState) -> Result<(), Action> {
    match action {
        // === Damage domain ===
        Action::Damage(info) => damage::handle_damage(info, state),
        Action::Barrage { damage } => cards::handle_barrage(damage, state),
        Action::PummelDamage(info) => damage::handle_pummel_damage(info, state),
        Action::BaneDamage(info) => damage::handle_bane_damage(info, state),
        Action::WallopDamage(info) => damage::handle_wallop_damage(info, state),
        Action::Judgement { target, cutoff } => damage::handle_judgement(target, cutoff, state),
        Action::InstantKill { target } => damage::handle_instant_kill(target, state),
        Action::TriggerMarks { card_id } => damage::handle_trigger_marks(card_id, state),
        Action::DamagePerAttackPlayed(info) => damage::handle_damage_per_attack_played(info, state),
        Action::HeelHook(info) => damage::handle_heel_hook(info, state),
        Action::Flechettes(info) => damage::handle_flechettes(info, state),
        #[rustfmt::skip] Action::MonsterAttack { source, target, base_damage, damage_kind } => damage::handle_monster_attack(source, target, base_damage, damage_kind, state),
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            is_modified,
        } => damage::handle_damage_all_enemies(source, damages, damage_type, is_modified, state),
        Action::OrbDamage {
            source,
            target,
            base_damage,
        } => damage::handle_orb_damage(source, target, base_damage, state),
        Action::OrbDamageRandomEnemy {
            source,
            base_damage,
        } => damage::handle_orb_damage_random_enemy(source, base_damage, state),
        Action::OrbDamageAllEnemies {
            source,
            base_damage,
        } => damage::handle_orb_damage_all_enemies(source, base_damage, state),
        Action::Whirlwind {
            damages,
            damage_type,
            free_to_play_once,
            energy_on_use,
        } => damage::handle_whirlwind(
            damages,
            damage_type,
            free_to_play_once,
            energy_on_use,
            state,
        ),
        Action::Skewer {
            target,
            damage_info,
            free_to_play_once,
            energy_on_use,
        } => damage::handle_skewer(target, damage_info, free_to_play_once, energy_on_use, state),
        Action::Sunder {
            target,
            damage_info,
            energy_gain,
        } => damage::handle_sunder(target, damage_info, energy_gain, state),
        Action::DamageRandomEnemy {
            source,
            base_damage,
            damage_type,
        } => damage::handle_damage_random_enemy(source, base_damage, damage_type, state),
        Action::AttackDamageRandomEnemyCard { card } => {
            damage::handle_attack_damage_random_enemy_card(*card, state)
        }
        Action::DropkickDamageAndEffect {
            target,
            damage_info,
        } => damage::handle_dropkick(target, damage_info, state),
        Action::SpotWeakness { target, amount } => {
            powers::handle_spot_weakness(target, amount, state)
        }
        Action::ApplyWeakIfTargetAttacking { target, amount } => {
            powers::handle_apply_weak_if_target_attacking(target, amount, state)
        }
        Action::FearNoEvil {
            target,
            damage_info,
        } => stances::handle_fear_no_evil(target, damage_info, state),
        Action::Ftl {
            target,
            damage_info,
            card_play_count,
        } => damage::handle_ftl(target, damage_info, card_play_count, state),
        Action::Doppelganger {
            upgraded,
            free_to_play_once,
            energy_on_use,
        } => powers::handle_doppelganger(upgraded, free_to_play_once, energy_on_use, state),
        Action::Malaise {
            target,
            upgraded,
            free_to_play_once,
            energy_on_use,
        } => powers::handle_malaise(target, upgraded, free_to_play_once, energy_on_use, state),
        Action::Collect {
            upgraded,
            free_to_play_once,
            energy_on_use,
        } => powers::handle_collect(upgraded, free_to_play_once, energy_on_use, state),
        Action::ConjureBlade {
            free_to_play_once,
            energy_on_use,
        } => cards::handle_conjure_blade(free_to_play_once, energy_on_use, state),
        Action::Meditate { amount } => cards::handle_meditate(amount, state),
        Action::FiendFire {
            target,
            damage_info,
        } => damage::handle_fiend_fire(target, damage_info, state),
        Action::Feed {
            target,
            damage_info,
            max_hp_amount,
        } => damage::handle_feed(target, damage_info, max_hp_amount, state),
        Action::LessonLearned {
            target,
            damage_info,
        } => damage::handle_lesson_learned(target, damage_info, state),
        Action::HandOfGreed {
            target,
            damage_info,
            gold_amount,
        } => damage::handle_hand_of_greed(target, damage_info, gold_amount, state),
        Action::RitualDagger {
            target,
            damage_info,
            misc_amount,
            card_lineage,
        } => damage::handle_ritual_dagger(target, damage_info, misc_amount, card_lineage, state),
        Action::VampireDamage(info) => damage::handle_vampire_damage(info, state),
        Action::VampireDamageAllEnemies {
            source,
            damages,
            damage_type,
        } => damage::handle_vampire_damage_all_enemies(source, damages, damage_type, state),
        Action::LoseHp {
            target,
            amount,
            triggers_rupture,
        } => damage::handle_lose_hp(target, amount, triggers_rupture, state),
        Action::PoisonLoseHp {
            source,
            target,
            amount,
        } => damage::handle_poison_lose_hp(source, target, amount, state),
        Action::SetCurrentHp { target, hp } => damage::handle_set_current_hp(target, hp, state),
        Action::GainBlock { target, amount } => damage::handle_gain_block(target, amount, state),
        Action::DoubleBlock { target } => damage::handle_double_block(target, state),
        Action::GainBlockRandomMonster { source, amount } => {
            damage::handle_gain_block_random_monster(source, amount, state)
        }
        Action::LoseBlock { target, amount } => damage::handle_lose_block(target, amount, state),
        Action::RemoveAllBlock { target } => damage::handle_remove_all_block(target, state),
        Action::Heal { target, amount } => damage::handle_heal(target, amount, state),
        Action::GainGold { amount } => damage::handle_gain_gold(amount, state),
        Action::StealPlayerGold { thief_id, amount } => {
            damage::handle_steal_player_gold(thief_id, amount, state)
        }
        Action::LimitBreak => damage::handle_limit_break(state),
        Action::BlockPerNonAttack { block_per_card } => {
            damage::handle_block_per_non_attack(block_per_card, state)
        }
        Action::ExhaustAllNonAttack => damage::handle_exhaust_all_non_attack(state),
        Action::ExhaustRandomCard { amount } => damage::handle_exhaust_random_card(amount, state),

        other => return Err(other),
    }
    Ok(())
}
