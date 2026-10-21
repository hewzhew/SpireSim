use crate::content::powers::store;
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RevivalSource {
    FairyPotion { slot: usize },
    LizardTail { relic_index: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct RevivalRequest {
    pub source: RevivalSource,
    /// Passed to the ordinary heal handler when this resource triggers.
    pub healing_request: i32,
    /// Healing modifiers and max-HP cap evaluated from zero HP in this state.
    /// This is a remaining resource, not current HP or a survival forecast.
    pub healing_from_zero: i32,
}

/// Remaining passive revivals in engine consumption order. Readable while
/// alive; only try_revive owns the lethal trigger, healing callbacks and spend.
pub fn available_revival_requests(
    state: &CombatState,
) -> impl Iterator<Item = RevivalRequest> + '_ {
    use crate::content::{potions::PotionId, relics::RelicId};
    let player = &state.entities.player;
    let enabled = !player.has_relic(RelicId::MarkOfTheBloom);
    let request = move |source, healing_request| RevivalRequest {
        source,
        healing_request,
        healing_from_zero: crate::content::relics::hooks::on_calculate_heal(state, healing_request)
            .min(player.max_hp)
            .max(0),
    };
    let mut potency = 0.3_f32;
    if player.has_relic(RelicId::SacredBark) {
        potency *= 2.0;
    }
    let fairy_heal = ((player.max_hp as f32 * potency) as i32).max(1);
    let fairies = state
        .entities
        .potions
        .iter()
        .enumerate()
        .filter_map(move |(slot, potion)| {
            (enabled
                && potion
                    .as_ref()
                    .is_some_and(|p| p.id == PotionId::FairyPotion))
            .then(|| request(RevivalSource::FairyPotion { slot }, fairy_heal))
        });
    let tail = player
        .relics
        .iter()
        .position(|r| r.id == RelicId::LizardTail)
        .filter(|&i| enabled && player.relics[i].counter == -1 && !player.relics[i].used_up)
        .map(|relic_index| {
            request(
                RevivalSource::LizardTail { relic_index },
                crate::content::relics::lizard_tail::revive_amount(player.max_hp),
            )
        });
    fairies.chain(tail)
}

/// Revival runs inside the lethal-damage check, before defeat. At zero HP it
/// still invokes ordinary healing modifiers and HP-threshold callbacks.
pub fn try_revive(state: &mut CombatState) {
    if state.entities.player.current_hp > 0 {
        return;
    }
    let Some(request) = available_revival_requests(state).next() else {
        return;
    };
    super::damage::handle_heal(0, request.healing_request, state);
    match request.source {
        RevivalSource::FairyPotion { slot } => state.entities.potions[slot] = None,
        RevivalSource::LizardTail { relic_index } => {
            let lt = &mut state.entities.player.relics[relic_index];
            lt.used_up = true;
            lt.counter = -2;
        }
    }
}

fn apply_awakened_one_rebirth_interrupt(state: &mut CombatState, target_id: usize) {
    // Java `AwakenedOne.damage()` performs these mutations immediately after
    // relic onMonsterDeath hooks, while also queuing a later SetMoveAction.
    store::retain_entity_powers(state, target_id, |power| {
        power.power_type != crate::content::powers::PowerId::Curiosity
            && power.power_type != crate::content::powers::PowerId::Unawakened
            && power.power_type != crate::content::powers::PowerId::Shackled
            && !crate::content::powers::is_debuff(power.power_type, power.amount)
    });

    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|monster| monster.id == target_id)
    {
        monster.set_planned_move_id(3);
        monster.set_planned_steps(
            crate::runtime::combat::Intent::Unknown
                .to_legacy_move_spec()
                .to_steps(),
        );
        monster.set_planned_visible_spec(None);
        monster.move_history_mut().push_back(3);
        monster.awakened_one.form1 = false;
        monster.awakened_one.first_turn = true;
        monster.awakened_one.protocol_seeded = true;
    }

    state.queue_action_front(Action::ClearCardQueue);
    state.queue_action_back(Action::SetMonsterMove {
        monster_id: target_id,
        next_move_byte: 3,
        planned_steps: crate::runtime::combat::Intent::Unknown
            .to_legacy_move_spec()
            .to_steps(),
        planned_visible_spec: None,
    });
}

fn clear_darkling_powers_after_death_relics(state: &mut CombatState) {
    let ids_to_clear: Vec<_> = state
        .entities
        .monsters
        .iter()
        .filter(|monster| {
            monster.enemy_id == crate::content::monsters::EnemyId::Darkling
                && (monster.current_hp <= 0 || monster.half_dead || monster.is_dying)
        })
        .map(|monster| monster.id)
        .collect();
    for id in ids_to_clear {
        store::remove_entity_powers(state, id);
    }
}

/// Centralized monster death handler.
/// Fires power on_death hooks, monster on_death, relic hooks, and Darkling specials.
pub fn check_and_trigger_monster_death(state: &mut CombatState, target_id: usize) {
    let mut is_awakened_rebirth = false;
    let mut triggered_death = false;
    let mut dying_enemy_id: Option<crate::content::monsters::EnemyId> = None;

    if let Some(m) = state.entities.monsters.iter().find(|m| m.id == target_id) {
        if m.current_hp <= 0 && !m.is_dying && !m.half_dead {
            let m_id = m.enemy_id;
            dying_enemy_id = Some(m_id);
            let has_rebirth_power = store::powers_for(state, target_id).is_some_and(|powers| {
                powers.iter().any(|p| {
                    matches!(
                        p.power_type,
                        crate::content::powers::PowerId::Regrow
                            | crate::content::powers::PowerId::Unawakened
                    )
                })
            });
            is_awakened_rebirth =
                has_rebirth_power && m_id == crate::content::monsters::EnemyId::AwakenedOne;
            triggered_death = true;
        }
    }

    if triggered_death {
        if let Some(m) = state
            .entities
            .monsters
            .iter_mut()
            .find(|m| m.id == target_id)
        {
            if is_awakened_rebirth {
                m.half_dead = true;
                m.is_dying = false;
            } else if dying_enemy_id == Some(crate::content::monsters::EnemyId::Darkling) {
                // Java Darkling.damage() marks the monster half-dead before
                // power onDeath and relic onMonsterDeath hooks. Darkling.die()
                // is a no-op while the room cannot lose, so isDying remains
                // false for those hooks.
                m.half_dead = true;
                m.is_dying = false;
            } else {
                m.is_dying = true;
            }
        }
    }

    if triggered_death {
        // Fire power on_death hooks BEFORE clearing (SporeCloud, Stasis, Unawakened, etc.)
        for power in &store::powers_snapshot_for(state, target_id) {
            let death_actions = crate::content::powers::resolve_power_on_death(
                power.power_type,
                state,
                target_id,
                power.amount,
                power.extra_data,
            );
            for a in death_actions {
                state.queue_action_back(a);
            }
        }

        if let Some(m_id) = dying_enemy_id {
            if !is_awakened_rebirth && m_id != crate::content::monsters::EnemyId::Darkling {
                let m_clone = state
                    .entities
                    .monsters
                    .iter()
                    .find(|m| m.id == target_id)
                    .unwrap()
                    .clone();
                let death_actions_on_entity =
                    crate::content::monsters::resolve_on_death(m_id, state, &m_clone);
                if matches!(
                    m_id,
                    crate::content::monsters::EnemyId::Reptomancer
                        | crate::content::monsters::EnemyId::BronzeAutomaton
                        | crate::content::monsters::EnemyId::TheCollector
                ) {
                    // These Java die() overrides add minion suicides to the top,
                    // before the lethal card's UseCardAction can exhaust it.
                    // on_death already returns their reversed execution order.
                    for a in death_actions_on_entity.into_iter().rev() {
                        state.queue_action_front(a);
                    }
                } else {
                    for a in death_actions_on_entity {
                        state.queue_action_back(a);
                    }
                }
            }
        }

        let death_actions = crate::content::relics::hooks::on_monster_death(state, target_id);
        state.queue_actions(death_actions);
        if dying_enemy_id == Some(crate::content::monsters::EnemyId::Darkling) {
            // Java Darkling.damage(): halfDead mutation happens before relic
            // onMonsterDeath hooks, but powers.clear(), all-dead handling, and
            // setMove(COUNT)/SetMoveAction(COUNT) happen after those hooks.
            clear_darkling_powers_after_death_relics(state);
            let m_clone = state
                .entities
                .monsters
                .iter()
                .find(|m| m.id == target_id)
                .unwrap()
                .clone();
            let darkling_actions = crate::content::monsters::resolve_on_death(
                crate::content::monsters::EnemyId::Darkling,
                state,
                &m_clone,
            );
            for a in darkling_actions {
                state.queue_action_back(a);
            }
        }
        if is_awakened_rebirth {
            apply_awakened_one_rebirth_interrupt(state, target_id);
            let mut cleared_protocol_monster_id = None;
            if let Some(m) = state
                .entities
                .monsters
                .iter_mut()
                .find(|m| m.id == target_id)
            {
                m.current_hp = 0;
                m.is_dying = false;
                m.half_dead = true;
                cleared_protocol_monster_id = Some(m.id);
            }
            if let Some(monster_id) = cleared_protocol_monster_id {
                state.clear_monster_protocol_observation(monster_id);
            }
        }
    }
}
