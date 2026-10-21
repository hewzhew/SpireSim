//! Inspected oracle decision state; no RNG or protocol identity is exposed.
use serde::{Deserialize, Serialize};
use sts_core::content::monsters::EnemyId;
use sts_core::runtime::combat::MonsterEntity;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MonsterDecisionObservation {
    pub planned_move: u8,
    /// Engine roll history includes the current unexecuted roll. It is not an
    /// executed/public history and must not be used as a NoSL observation.
    pub roll_history: Vec<u8>,
    pub facts: Vec<MonsterFact>,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[repr(u8)]
pub enum MonsterFactId {
    FirstMove = 0,
    HardMode = 1,
    BiteDamage = 2,
    UsedHex = 3,
    UsedBellow = 4,
    DamageThreshold = 5,
    DamageTaken = 6,
    Open = 7,
    CloseUpTriggered = 8,
    UsedHaste = 9,
    MoveCount = 10,
    BuffCount = 11,
    BloodHitCount = 12,
    Awake = 13,
    IdleCount = 14,
    DebuffTurnCount = 15,
    WakeTriggered = 16,
    SlashCount = 17,
    StolenGold = 18,
    UsedEntangle = 19,
    CurrentCharge = 20,
    SplitTriggered = 21,
    FirstForm = 22,
    InitialSpawn = 23,
    UsedUlt = 24,
    TurnsTaken = 25,
    TrackedMinionSlot = 26,
    Activated = 27,
    OrbActiveCount = 28,
    BurnUpgraded = 29,
    DividerDamage = 30,
}

impl MonsterFactId {
    pub fn scale(self) -> f32 {
        match self {
            Self::BiteDamage | Self::DamageThreshold | Self::DamageTaken | Self::DividerDamage => {
                50.
            }
            Self::MoveCount | Self::BuffCount | Self::BloodHitCount | Self::TurnsTaken => 10.,
            Self::IdleCount | Self::DebuffTurnCount | Self::SlashCount | Self::CurrentCharge => 10.,
            Self::StolenGold => 100.,
            _ => 1.,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MonsterFact {
    pub fact: MonsterFactId,
    pub value: i32,
    /// Source-local monster row, resolved from typed controller ownership.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_monster: Option<usize>,
}

pub fn decision(m: &MonsterEntity) -> Option<MonsterDecisionObservation> {
    use MonsterFactId as F;
    let entries = match m.enemy_id {
        EnemyId::JawWorm => vec![
            (F::FirstMove, i32::from(m.jaw_worm.first_move)),
            (F::HardMode, i32::from(m.jaw_worm.hard_mode)),
        ],
        EnemyId::Cultist => vec![(F::FirstMove, i32::from(m.cultist.first_move))],
        EnemyId::LouseNormal | EnemyId::LouseDefensive => m
            .louse
            .bite_damage
            .into_iter()
            .map(|n| (F::BiteDamage, n))
            .collect(),
        EnemyId::Sentry => vec![(F::FirstMove, i32::from(m.sentry.first_move))],
        EnemyId::Chosen => vec![
            (F::FirstMove, i32::from(m.chosen.first_turn)),
            (F::UsedHex, i32::from(m.chosen.used_hex)),
        ],
        EnemyId::GremlinNob => vec![(F::UsedBellow, i32::from(m.gremlin_nob.used_bellow))],
        EnemyId::Lagavulin => vec![
            (F::Awake, i32::from(m.lagavulin.is_out)),
            (F::IdleCount, i32::from(m.lagavulin.idle_count)),
            (F::DebuffTurnCount, i32::from(m.lagavulin.debuff_turn_count)),
            (F::WakeTriggered, i32::from(m.lagavulin.is_out_triggered)),
        ],
        EnemyId::Looter => vec![
            (F::SlashCount, i32::from(m.thief.slash_count)),
            (F::StolenGold, m.thief.stolen_gold),
        ],
        EnemyId::SlaverRed => vec![
            (F::FirstMove, i32::from(m.slaver_red.first_turn)),
            (F::UsedEntangle, i32::from(m.slaver_red.used_entangle)),
        ],
        EnemyId::GremlinWizard => {
            vec![(F::CurrentCharge, i32::from(m.gremlin_wizard.current_charge))]
        }
        EnemyId::AcidSlimeL | EnemyId::SpikeSlimeL => {
            vec![(F::SplitTriggered, i32::from(m.large_slime.split_triggered))]
        }
        EnemyId::SlimeBoss => {
            if !m.slime_boss.protocol_seeded {
                return None;
            }
            // Split readiness and child HP are already present in powers and
            // typed planned effects; the controller owns only first_turn.
            vec![(F::FirstMove, i32::from(m.slime_boss.first_turn))]
        }
        EnemyId::Hexaghost => {
            let mut facts = vec![
                (F::Activated, i32::from(m.hexaghost.activated)),
                (F::OrbActiveCount, i32::from(m.hexaghost.orb_active_count)),
                (F::BurnUpgraded, i32::from(m.hexaghost.burn_upgraded)),
            ];
            facts.extend(m.hexaghost.divider_damage.map(|n| (F::DividerDamage, n)));
            facts
        }
        // These inspected controllers use the current move, roll history and
        // already observed combat entities/powers, with no extra runtime fields.
        EnemyId::AcidSlimeM
        | EnemyId::AcidSlimeS
        | EnemyId::SpikeSlimeM
        | EnemyId::SpikeSlimeS
        | EnemyId::SlaverBlue
        | EnemyId::FungiBeast
        | EnemyId::GremlinFat
        | EnemyId::GremlinThief
        | EnemyId::GremlinTsundere
        | EnemyId::GremlinWarrior
        | EnemyId::TorchHead => vec![],
        EnemyId::TheCollector => {
            if !m.collector.protocol_seeded {
                return None;
            }
            vec![
                (F::InitialSpawn, i32::from(m.collector.initial_spawn)),
                (F::UsedUlt, i32::from(m.collector.ult_used)),
                (F::TurnsTaken, i32::from(m.collector.turns_taken)),
            ]
        }
        EnemyId::TheGuardian => vec![
            (F::DamageThreshold, m.guardian.damage_threshold),
            (F::DamageTaken, m.guardian.damage_taken),
            (F::Open, i32::from(m.guardian.is_open)),
            (
                F::CloseUpTriggered,
                i32::from(m.guardian.close_up_triggered),
            ),
        ],
        EnemyId::TimeEater => vec![(F::UsedHaste, i32::from(m.time_eater.used_haste))],
        EnemyId::AwakenedOne => {
            if !m.awakened_one.protocol_seeded {
                return None;
            }
            vec![
                (F::FirstForm, i32::from(m.awakened_one.form1)),
                (F::FirstMove, i32::from(m.awakened_one.first_turn)),
            ]
        }
        EnemyId::CorruptHeart => vec![
            (F::FirstMove, i32::from(m.corrupt_heart.first_move)),
            (F::MoveCount, i32::from(m.corrupt_heart.move_count)),
            (F::BuffCount, i32::from(m.corrupt_heart.buff_count)),
            (F::BloodHitCount, i32::from(m.corrupt_heart.blood_hit_count)),
        ],
        _ => return None,
    };
    Some(MonsterDecisionObservation {
        planned_move: m.planned_move_id(),
        roll_history: m.move_history().iter().copied().collect(),
        facts: entries
            .into_iter()
            .map(|(fact, value)| MonsterFact {
                fact,
                value,
                related_monster: None,
            })
            .collect(),
    })
}

pub fn decision_in_group(
    m: &MonsterEntity,
    monsters: &[MonsterEntity],
) -> Option<MonsterDecisionObservation> {
    let mut observation = decision(m)?;
    if m.enemy_id == EnemyId::TheCollector {
        for (slot, target) in m.collector.enemy_slots.iter().enumerate() {
            let related_monster = match target {
                Some(id) => Some(monsters.iter().position(|other| other.id == *id)?),
                None => None,
            };
            observation.facts.push(MonsterFact {
                fact: MonsterFactId::TrackedMinionSlot,
                value: slot as i32,
                related_monster,
            });
        }
    }
    Some(observation)
}
