use crate::content::cards::CardTarget;
use crate::content::monsters::EnemyId;
use crate::content::powers::{store, PowerId};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::sim::combat_projection::{project_monster_move_preview_in_combat, VisibleIntentKind};

/// Marks actions that must enter the child-generation corridor while living
/// thieves can still remove run resources.
///
/// Damage against a thief receives the ordinary urgent rank. Defensive cards
/// deliberately do not opt in: lifting one defensive action above every
/// attack changes the whole finite generation surface, while the useful
/// thief line is a complete-turn allocation claim rather than a context-free
/// fact about that card.
pub(super) fn recoverable_resource_urgency_for_play(
    combat: &CombatState,
    target_kind: CardTarget,
    target: Option<usize>,
    target_progress: i32,
    visible_loss_now: i32,
) -> i32 {
    if visible_loss_now >= combat.entities.player.current_hp
        || combat
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.is_alive_for_action())
            .any(|monster| {
                project_monster_move_preview_in_combat(combat, monster).visible_intent
                    == VisibleIntentKind::Unknown
            })
    {
        return 0;
    }

    if target_progress <= 0 {
        return 0;
    }

    let targets_thief_pressure = match target_kind {
        CardTarget::AllEnemy => combat
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.is_alive_for_action())
            .any(|monster| has_thief_resource_pressure(combat, monster)),
        CardTarget::Enemy | CardTarget::SelfAndEnemy => target
            .and_then(|entity_id| {
                combat
                    .entities
                    .monsters
                    .iter()
                    .find(|monster| monster.id == entity_id && monster.is_alive_for_action())
            })
            .is_some_and(|monster| has_thief_resource_pressure(combat, monster)),
        _ => false,
    };

    i32::from(targets_thief_pressure)
}

fn has_thief_resource_pressure(combat: &CombatState, monster: &MonsterEntity) -> bool {
    monster.thief.stolen_gold > 0
        || matches!(monster.enemy_id, EnemyId::Looter | EnemyId::Mugger)
        || store::has_power(combat, monster.id, PowerId::Thievery)
}
