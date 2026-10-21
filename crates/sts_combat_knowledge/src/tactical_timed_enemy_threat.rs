//! Visible finite-horizon enemy threats owned by a living encounter member.
//!
//! These are typed state facts. Consumers decide how urgency affects action
//! ordering, rollout, or reports.

use sts_core::content::powers::{store, PowerId};
use sts_core::runtime::combat::CombatState;
use sts_core::EntityId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatTacticalTimedEnemyThreatKindV1 {
    ForcedPlayerDamage,
}

impl CombatTacticalTimedEnemyThreatKindV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::ForcedPlayerDamage => "forced_player_damage",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CombatTacticalTimedEnemyThreatV1 {
    pub source_entity_id: EntityId,
    pub kind: CombatTacticalTimedEnemyThreatKindV1,
    pub owner_turns_until_trigger: u32,
    pub raw_player_damage: i32,
    pub canceled_by_owner_death: bool,
}

pub fn combat_tactical_timed_enemy_threats_v1(
    combat: &CombatState,
) -> Vec<CombatTacticalTimedEnemyThreatV1> {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .filter_map(|monster| combat_tactical_timed_enemy_threat_for_target_v1(combat, monster.id))
        .collect()
}

pub fn combat_tactical_timed_enemy_threat_for_target_v1(
    combat: &CombatState,
    entity_id: EntityId,
) -> Option<CombatTacticalTimedEnemyThreatV1> {
    let owner = combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == entity_id && monster.is_alive_for_action())?;
    let amount = store::power_amount(combat, owner.id, PowerId::Explosive);
    (amount > 0).then_some(CombatTacticalTimedEnemyThreatV1 {
        source_entity_id: owner.id,
        kind: CombatTacticalTimedEnemyThreatKindV1::ForcedPlayerDamage,
        owner_turns_until_trigger: amount as u32,
        raw_player_damage: sts_core::content::powers::core::explosive::EXPLOSION_DAMAGE,
        canceled_by_owner_death: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::monsters::EnemyId;
    use sts_core::runtime::combat::{Power, PowerPayload};
    use sts_core::test_support::{blank_test_combat, test_monster};

    fn combat_with_explosive(amount: i32) -> CombatState {
        let mut combat = blank_test_combat();
        let mut owner = test_monster(EnemyId::Exploder);
        owner.id = 7;
        combat.entities.monsters = vec![owner];
        combat.entities.power_db.insert(
            7,
            vec![Power {
                power_type: PowerId::Explosive,
                instance_id: None,
                amount,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        combat
    }

    #[test]
    fn timed_enemy_threat_reads_positive_explosive_power() {
        let combat = combat_with_explosive(3);

        assert_eq!(
            combat_tactical_timed_enemy_threats_v1(&combat),
            vec![CombatTacticalTimedEnemyThreatV1 {
                source_entity_id: 7,
                kind: CombatTacticalTimedEnemyThreatKindV1::ForcedPlayerDamage,
                owner_turns_until_trigger: 3,
                raw_player_damage: sts_core::content::powers::core::explosive::EXPLOSION_DAMAGE,
                canceled_by_owner_death: true,
            }]
        );
    }

    #[test]
    fn timed_enemy_threat_tracks_urgency_and_ignores_inactive_owners() {
        let amount_one = combat_with_explosive(1);
        assert_eq!(
            combat_tactical_timed_enemy_threat_for_target_v1(&amount_one, 7)
                .map(|fact| fact.owner_turns_until_trigger),
            Some(1)
        );

        let powerless = combat_with_explosive(0);
        assert!(combat_tactical_timed_enemy_threats_v1(&powerless).is_empty());

        let mut dead = combat_with_explosive(3);
        dead.entities.monsters[0].is_dying = true;
        assert!(combat_tactical_timed_enemy_threats_v1(&dead).is_empty());
    }
}
