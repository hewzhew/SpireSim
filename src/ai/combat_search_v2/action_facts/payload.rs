use crate::content::cards;
use crate::runtime::action::{Action, DamageInfo};
use crate::runtime::combat::{CombatCard, CombatState};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ResolvedCardActionPayloadFacts {
    pub(super) damage_total_hint: i32,
    pub(super) damage_hit_count_hint: usize,
    pub(super) player_block_hint: i32,
}

pub(super) fn resolved_card_action_payload_facts(
    combat: &CombatState,
    card: &CombatCard,
    target: Option<usize>,
) -> ResolvedCardActionPayloadFacts {
    let actions = cards::resolve_card_play_with_context(
        card.id,
        combat,
        card,
        target,
        cards::CardUseContext {
            played_from_hand: true,
        },
    );
    let mut facts = ResolvedCardActionPayloadFacts::default();
    for action in actions {
        observe_resolved_action_payload(combat, &mut facts, action.action);
    }
    facts
}

fn observe_resolved_action_payload(
    combat: &CombatState,
    facts: &mut ResolvedCardActionPayloadFacts,
    action: Action,
) {
    match action {
        Action::Damage(info)
        | Action::PummelDamage(info)
        | Action::BaneDamage(info)
        | Action::WallopDamage(info)
        | Action::DamagePerAttackPlayed(info)
        | Action::HeelHook(info)
        | Action::Flechettes(info)
        | Action::DropkickDamageAndEffect {
            damage_info: info, ..
        }
        | Action::Ftl {
            damage_info: info, ..
        }
        | Action::Skewer {
            damage_info: info, ..
        }
        | Action::Sunder {
            damage_info: info, ..
        }
        | Action::FearNoEvil {
            damage_info: info, ..
        }
        | Action::FiendFire {
            damage_info: info, ..
        }
        | Action::Feed {
            damage_info: info, ..
        }
        | Action::LessonLearned {
            damage_info: info, ..
        }
        | Action::HandOfGreed {
            damage_info: info, ..
        }
        | Action::RitualDagger {
            damage_info: info, ..
        }
        | Action::VampireDamage(info)
        | Action::Barrage { damage: info } => {
            observe_damage_payload(combat, facts, &info);
        }
        Action::DamageAllEnemies { damages, .. }
        | Action::VampireDamageAllEnemies { damages, .. }
        | Action::Whirlwind { damages, .. } => {
            for (slot, damage) in damages.into_iter().enumerate() {
                if combat
                    .entities
                    .monsters
                    .get(slot)
                    .is_some_and(|monster| monster.is_alive_for_action())
                {
                    add_damage_payload(facts, damage);
                }
            }
        }
        Action::GainBlock { target: 0, amount } => {
            facts.player_block_hint = facts.player_block_hint.saturating_add(amount.max(0));
        }
        _ => {}
    }
}

fn observe_damage_payload(
    combat: &CombatState,
    facts: &mut ResolvedCardActionPayloadFacts,
    info: &DamageInfo,
) {
    if combat
        .entities
        .monsters
        .iter()
        .any(|monster| monster.id == info.target && monster.is_alive_for_action())
    {
        add_damage_payload(facts, info.output.max(0));
    }
}

fn add_damage_payload(facts: &mut ResolvedCardActionPayloadFacts, damage: i32) {
    let damage = damage.max(0);
    if damage == 0 {
        return;
    }
    facts.damage_total_hint = facts.damage_total_hint.saturating_add(damage);
    facts.damage_hit_count_hint = facts.damage_hit_count_hint.saturating_add(1);
}
