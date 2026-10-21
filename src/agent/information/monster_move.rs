//! Move mechanics inferred from public observations and original-game rules.
//! These annotations do not alter intent, execute effects, or read a locked move.

use serde::{Deserialize, Serialize};

use super::combat::ObservationEvidenceKindV1;
use super::state::{CombatLearningEnemyIdentityV1, CombatLearningMonsterStateV1};
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::runtime::combat::Intent;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnownMonsterMoveV1 {
    ShelledParasiteLifeSuck,
    BronzeAutomatonSpawnOrbs,
    WrithingMassImplant,
}

impl KnownMonsterMoveV1 {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ShelledParasiteLifeSuck => "Life Suck",
            Self::BronzeAutomatonSpawnOrbs => "Spawn Orbs",
            Self::WrithingMassImplant => "Implant",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MonsterMoveEffectV1 {
    HealSelfForDamageDealt,
    Summon { enemy_id: EnemyId, count: u8 },
    AddCardToMasterDeck { card_id: CardId, count: u8 },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MonsterMoveKnowledgeEvidenceV1 {
    VisibleIntentAndRules,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterMoveKnowledgeV1 {
    pub monster_slot: u8,
    pub move_id: KnownMonsterMoveV1,
    pub evidence: MonsterMoveKnowledgeEvidenceV1,
    pub effects: Vec<MonsterMoveEffectV1>,
}

/// Only a uniquely identifiable move receives an annotation. Lack of an
/// annotation says nothing about the effects of an unrecognized move.
pub fn public_monster_move_knowledge_v1(
    monster: &CombatLearningMonsterStateV1,
) -> Option<MonsterMoveKnowledgeV1> {
    if !monster.alive
        || monster.half_dead
        || monster.escaped
        || monster.intent.evidence != ObservationEvidenceKindV1::VisibleExact
        || monster.intent.hidden_reason.is_some()
    {
        return None;
    }
    let CombatLearningEnemyIdentityV1::Known { enemy_id } = monster.enemy;
    // ShelledParasite.getMove: Life Suck is its only ATTACK_BUFF move.
    // VampireDamageAction heals the source using the target's lastDamageTaken.
    match (enemy_id, monster.intent.intent.as_ref()?) {
        (EnemyId::ShelledParasite, Intent::AttackBuff { .. }) => Some(MonsterMoveKnowledgeV1 {
            monster_slot: monster.slot,
            move_id: KnownMonsterMoveV1::ShelledParasiteLifeSuck,
            evidence: MonsterMoveKnowledgeEvidenceV1::VisibleIntentAndRules,
            effects: vec![MonsterMoveEffectV1::HealSelfForDamageDealt],
        }),
        // BronzeAutomaton.getMove uses UNKNOWN only for its initial Spawn Orbs.
        // The icon itself does not encode summon count; that comes from the rule.
        (EnemyId::BronzeAutomaton, Intent::Unknown) => Some(MonsterMoveKnowledgeV1 {
            monster_slot: monster.slot,
            move_id: KnownMonsterMoveV1::BronzeAutomatonSpawnOrbs,
            evidence: MonsterMoveKnowledgeEvidenceV1::VisibleIntentAndRules,
            effects: vec![MonsterMoveEffectV1::Summon {
                enemy_id: EnemyId::BronzeOrb,
                count: 2,
            }],
        }),
        // WrithingMass.getMove uses STRONG_DEBUFF only for Implant; takeTurn
        // queues AddCardToDeckAction(Parasite), rather than applying a power.
        (EnemyId::WrithingMass, Intent::StrongDebuff) => Some(MonsterMoveKnowledgeV1 {
            monster_slot: monster.slot,
            move_id: KnownMonsterMoveV1::WrithingMassImplant,
            evidence: MonsterMoveKnowledgeEvidenceV1::VisibleIntentAndRules,
            effects: vec![MonsterMoveEffectV1::AddCardToMasterDeck {
                card_id: CardId::Parasite,
                count: 1,
            }],
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::information::state::public_combat_state_v1;
    use crate::content::monsters::{city::shelled_parasite::ShelledParasite, MonsterBehavior};
    use crate::content::relics::{RelicId, RelicState};
    use crate::test_support::{blank_test_combat, test_monster};

    #[test]
    fn implant_knowledge_requires_visible_writhing_mass_strong_debuff() {
        let mut combat = blank_test_combat();
        let mut mass = test_monster(EnemyId::WrithingMass);
        mass.set_planned_visible_spec(Some(
            crate::runtime::monster_move::MonsterMoveSpec::StrongDebuffIntent,
        ));
        combat.entities.monsters.push(mass);
        let mut observed = public_combat_state_v1(&combat).monsters[0].clone();
        let knowledge = public_monster_move_knowledge_v1(&observed).unwrap();
        assert_eq!(knowledge.move_id, KnownMonsterMoveV1::WrithingMassImplant);
        assert_eq!(
            knowledge.effects,
            [MonsterMoveEffectV1::AddCardToMasterDeck {
                card_id: CardId::Parasite,
                count: 1,
            }]
        );
        observed.enemy = CombatLearningEnemyIdentityV1::Known {
            enemy_id: EnemyId::BronzeOrb,
        };
        assert!(public_monster_move_knowledge_v1(&observed).is_none());
        observed = public_combat_state_v1(&combat).monsters[0].clone();
        observed.intent.intent = Some(Intent::Unknown);
        assert!(public_monster_move_knowledge_v1(&observed).is_none());
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicDome));
        assert!(
            public_monster_move_knowledge_v1(&public_combat_state_v1(&combat).monsters[0])
                .is_none()
        );
    }

    #[test]
    fn automaton_summon_knowledge_is_separate_from_its_unknown_icon() {
        let mut combat = blank_test_combat();
        let mut monster = test_monster(EnemyId::BronzeAutomaton);
        monster
            .set_planned_visible_spec(Some(crate::runtime::monster_move::MonsterMoveSpec::Unknown));
        combat.entities.monsters.push(monster);
        let mut observed = public_combat_state_v1(&combat).monsters[0].clone();
        assert_eq!(observed.intent.intent, Some(Intent::Unknown));
        let knowledge = public_monster_move_knowledge_v1(&observed).unwrap();
        assert_eq!(
            knowledge.move_id,
            KnownMonsterMoveV1::BronzeAutomatonSpawnOrbs
        );
        assert_eq!(
            knowledge.effects,
            [MonsterMoveEffectV1::Summon {
                enemy_id: EnemyId::BronzeOrb,
                count: 2,
            }]
        );
        observed.enemy = CombatLearningEnemyIdentityV1::Known {
            enemy_id: EnemyId::GremlinLeader,
        };
        assert!(public_monster_move_knowledge_v1(&observed).is_none());
        observed = public_combat_state_v1(&combat).monsters[0].clone();
        observed.intent.intent = Some(Intent::DefendBuff);
        assert!(public_monster_move_knowledge_v1(&observed).is_none());
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicDome));
        assert!(
            public_monster_move_knowledge_v1(&public_combat_state_v1(&combat).monsters[0])
                .is_none()
        );
    }

    #[test]
    fn life_suck_knowledge_uses_visible_intent_without_reading_the_locked_move() {
        let mut combat = blank_test_combat();
        let mut monster = test_monster(EnemyId::ShelledParasite);
        monster.set_planned_move_id(3);
        let plan = ShelledParasite::turn_plan(&combat, &monster);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters.push(monster);

        let public = public_combat_state_v1(&combat);
        let mut observed = public.monsters[0].clone();
        assert!(matches!(
            observed.intent.intent,
            Some(Intent::AttackBuff { .. })
        ));
        let knowledge = public_monster_move_knowledge_v1(&observed).unwrap();
        assert_eq!(
            knowledge.move_id,
            KnownMonsterMoveV1::ShelledParasiteLifeSuck
        );
        assert_eq!(
            knowledge.effects,
            [MonsterMoveEffectV1::HealSelfForDamageDealt]
        );

        // The same icon on another monster does not establish life-steal.
        observed.enemy = CombatLearningEnemyIdentityV1::Known {
            enemy_id: EnemyId::JawWorm,
        };
        assert!(public_monster_move_knowledge_v1(&observed).is_none());
        observed = public.monsters[0].clone();
        observed.intent.intent = Some(Intent::Attack { damage: 6, hits: 2 });
        assert!(public_monster_move_knowledge_v1(&observed).is_none());
        observed = public.monsters[0].clone();
        observed.half_dead = true;
        assert!(public_monster_move_knowledge_v1(&observed).is_none());

        // Dome keeps the actual plan in the simulator, but neither public
        // observation nor the rule annotation exposes its selected move.
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicDome));
        let hidden = public_combat_state_v1(&combat);
        assert!(hidden.monsters[0].intent.intent.is_none());
        assert!(public_monster_move_knowledge_v1(&hidden.monsters[0]).is_none());
    }
}
