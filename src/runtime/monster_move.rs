use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::powers::PowerId;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};

/// Runtime representation of a monster's selected move.
///
/// This is a core combat type, not a separate strategy/semantic layer. Monster
/// content produces these plans; runtime actions store and execute them; search
/// and observation code may project them into public intent summaries.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum DamageKind {
    Normal,
    HpLoss,
    Thorns,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum EffectStrength {
    Normal,
    Strong,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum MoveTarget {
    Player,
    SelfTarget,
    AllMonsters,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum PowerEffectKind {
    Buff,
    Debuff,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum CardDestination {
    Hand,
    Discard,
    DrawPileTop,
    DrawPileRandom,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AttackSpec {
    pub base_damage: i32,
    pub hits: u8,
    pub damage_kind: DamageKind,
}

impl AttackSpec {
    pub fn total_base_damage(&self) -> i32 {
        self.base_damage.saturating_mul(self.hits.max(1) as i32)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BuffSpec {
    pub power_id: PowerId,
    pub amount: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct DebuffSpec {
    pub power_id: PowerId,
    pub amount: i32,
    pub strength: EffectStrength,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct DefendSpec {
    pub block: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AttackStep {
    pub target: MoveTarget,
    pub attack: AttackSpec,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ApplyPowerStep {
    pub target: MoveTarget,
    pub power_id: PowerId,
    pub amount: i32,
    pub effect: PowerEffectKind,
    pub visible_strength: EffectStrength,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BlockStep {
    pub target: MoveTarget,
    pub amount: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RandomBlockStep {
    pub amount: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AddCardStep {
    pub card_id: CardId,
    pub amount: u8,
    pub upgraded: bool,
    pub destination: CardDestination,
    pub visible_strength: EffectStrength,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct StealGoldStep {
    pub amount: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct UpgradeCardsStep {
    pub card_id: CardId,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RemovePowerStep {
    pub target: MoveTarget,
    pub power_id: PowerId,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum UtilityStep {
    RemoveAllDebuffs { target: MoveTarget },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HealStep {
    pub target: MoveTarget,
    pub amount: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HealSpec {
    pub target: MoveTarget,
    pub amount: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum SpawnHpValue {
    Rolled,
    Fixed(i32),
    SourceCurrentHp,
    SourceMaxHp,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SpawnHpSpec {
    pub current: SpawnHpValue,
    pub max: SpawnHpValue,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SpawnMonsterStep {
    pub enemy_id: EnemyId,
    pub logical_position_offset: i32,
    pub protocol_draw_x_offset: Option<i32>,
    pub hp: SpawnHpSpec,
    pub is_minion: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum MoveStep {
    Attack(AttackStep),
    ApplyPower(ApplyPowerStep),
    Heal(HealStep),
    GainBlock(BlockStep),
    GainBlockRandomMonster(RandomBlockStep),
    AddCard(AddCardStep),
    StealGold(StealGoldStep),
    UpgradeCards(UpgradeCardsStep),
    RemovePower(RemovePowerStep),
    Utility(UtilityStep),
    SpawnMonster(SpawnMonsterStep),
    Suicide,
    Charge,
    Escape,
    Magic,
    Sleep,
    Stun,
    Debug,
    None,
}

pub type MonsterTurnSteps = SmallVec<[MoveStep; 4]>;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum MonsterMoveSpec {
    Attack(AttackSpec),
    AttackAddCard(AttackSpec, AddCardStep),
    AttackUpgradeCards(AttackSpec, UpgradeCardsStep),
    AttackBuff(AttackSpec, BuffSpec),
    /// The original ATTACK_BUFF icon without a declared power application.
    /// Additional effects come from the locked plan and monster executor.
    AttackBuffIntent(AttackSpec),
    AttackDebuff(AttackSpec, DebuffSpec),
    AttackDefend(AttackSpec, DefendSpec),
    AddCard(AddCardStep),
    Buff(BuffSpec),
    Debuff(DebuffSpec),
    StrongDebuff(DebuffSpec),
    /// The original STRONG_DEBUFF icon without a declared power application.
    StrongDebuffIntent,
    Defend(DefendSpec),
    DefendDebuff(DefendSpec, DebuffSpec),
    DefendBuff(DefendSpec, BuffSpec),
    Heal(HealSpec),
    Escape,
    Magic,
    Sleep,
    Stun,
    Debug,
    None,
    Unknown,
    /// A public BUFF icon without a declared power effect. Execution must come
    /// from the plan's locked steps, as with the Healer's group heal.
    BuffIntent,
}

impl MonsterMoveSpec {
    pub fn attack(&self) -> Option<&AttackSpec> {
        match self {
            Self::Attack(spec)
            | Self::AttackAddCard(spec, _)
            | Self::AttackUpgradeCards(spec, _)
            | Self::AttackBuff(spec, _)
            | Self::AttackBuffIntent(spec)
            | Self::AttackDebuff(spec, _)
            | Self::AttackDefend(spec, _) => Some(spec),
            _ => None,
        }
    }

    pub fn to_steps(&self) -> MonsterTurnSteps {
        match self {
            Self::Attack(attack) => smallvec![MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: attack.clone(),
            })],
            Self::AttackAddCard(attack, add_card) => smallvec![
                MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack: attack.clone(),
                }),
                MoveStep::AddCard(add_card.clone()),
            ],
            Self::AttackUpgradeCards(attack, upgrade) => smallvec![
                MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack: attack.clone(),
                }),
                MoveStep::UpgradeCards(upgrade.clone()),
            ],
            Self::AttackBuff(attack, buff) => smallvec![
                MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack: attack.clone(),
                }),
                MoveStep::ApplyPower(ApplyPowerStep {
                    target: MoveTarget::SelfTarget,
                    power_id: buff.power_id,
                    amount: buff.amount,
                    effect: PowerEffectKind::Buff,
                    visible_strength: EffectStrength::Normal,
                })
            ],
            Self::AttackBuffIntent(attack) => smallvec![MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: attack.clone(),
            })],
            Self::AttackDebuff(attack, debuff) => smallvec![
                MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack: attack.clone(),
                }),
                MoveStep::ApplyPower(ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: debuff.power_id,
                    amount: debuff.amount,
                    effect: PowerEffectKind::Debuff,
                    visible_strength: debuff.strength,
                })
            ],
            Self::AttackDefend(attack, defend) => smallvec![
                MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack: attack.clone(),
                }),
                MoveStep::GainBlock(BlockStep {
                    target: MoveTarget::SelfTarget,
                    amount: defend.block,
                })
            ],
            Self::Heal(heal) => smallvec![MoveStep::Heal(HealStep {
                target: heal.target,
                amount: heal.amount,
            })],
            Self::Buff(buff) => smallvec![MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: buff.power_id,
                amount: buff.amount,
                effect: PowerEffectKind::Buff,
                visible_strength: EffectStrength::Normal,
            })],
            Self::Debuff(debuff) | Self::StrongDebuff(debuff) => {
                smallvec![MoveStep::ApplyPower(ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: debuff.power_id,
                    amount: debuff.amount,
                    effect: PowerEffectKind::Debuff,
                    visible_strength: debuff.strength,
                })]
            }
            Self::Defend(defend) => smallvec![MoveStep::GainBlock(BlockStep {
                target: MoveTarget::SelfTarget,
                amount: defend.block,
            })],
            Self::AddCard(add_card) => smallvec![MoveStep::AddCard(add_card.clone())],
            Self::DefendDebuff(defend, debuff) => smallvec![
                MoveStep::GainBlock(BlockStep {
                    target: MoveTarget::SelfTarget,
                    amount: defend.block,
                }),
                MoveStep::ApplyPower(ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: debuff.power_id,
                    amount: debuff.amount,
                    effect: PowerEffectKind::Debuff,
                    visible_strength: debuff.strength,
                })
            ],
            Self::DefendBuff(defend, buff) => smallvec![
                MoveStep::GainBlock(BlockStep {
                    target: MoveTarget::SelfTarget,
                    amount: defend.block,
                }),
                MoveStep::ApplyPower(ApplyPowerStep {
                    target: MoveTarget::SelfTarget,
                    power_id: buff.power_id,
                    amount: buff.amount,
                    effect: PowerEffectKind::Buff,
                    visible_strength: EffectStrength::Normal,
                })
            ],
            Self::BuffIntent => smallvec![],
            Self::Escape => smallvec![MoveStep::Escape],
            Self::Magic => smallvec![MoveStep::Magic],
            Self::Sleep => smallvec![MoveStep::Sleep],
            Self::Stun => smallvec![MoveStep::Stun],
            Self::Debug => smallvec![MoveStep::Debug],
            Self::None => smallvec![MoveStep::None],
            Self::Unknown | Self::StrongDebuffIntent => smallvec![],
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct MonsterTurnPlan {
    pub move_id: u8,
    pub steps: MonsterTurnSteps,
    pub visible_spec: Option<MonsterMoveSpec>,
}

impl MonsterTurnPlan {
    pub fn new(move_id: u8, steps: MonsterTurnSteps) -> Self {
        assert!(
            steps.len() <= 1,
            "MonsterTurnPlan::new no longer permits multi-step plans; use with_visible_spec instead"
        );
        let visible_spec = steps.first().map(summary_spec_from_single_step);
        Self {
            move_id,
            steps,
            visible_spec,
        }
    }

    pub fn with_visible_spec(
        move_id: u8,
        steps: MonsterTurnSteps,
        visible_spec: MonsterMoveSpec,
    ) -> Self {
        Self {
            move_id,
            steps,
            visible_spec: Some(visible_spec),
        }
    }

    pub fn from_spec(move_id: u8, spec: MonsterMoveSpec) -> Self {
        Self {
            move_id,
            steps: spec.to_steps(),
            visible_spec: Some(spec),
        }
    }

    pub fn unknown(move_id: u8) -> Self {
        Self {
            move_id,
            steps: smallvec![],
            visible_spec: Some(MonsterMoveSpec::Unknown),
        }
    }

    pub fn single(move_id: u8, step: MoveStep) -> Self {
        let visible_spec = summary_spec_from_single_step(&step);
        Self {
            move_id,
            steps: smallvec![step],
            visible_spec: Some(visible_spec),
        }
    }

    pub fn attack(&self) -> Option<&AttackSpec> {
        self.steps.iter().find_map(|step| match step {
            MoveStep::Attack(attack) => Some(&attack.attack),
            _ => None,
        })
    }

    pub fn summary_spec(&self) -> MonsterMoveSpec {
        if let Some(spec) = &self.visible_spec {
            return spec.clone();
        }
        match self.steps.as_slice() {
            [step] => summary_spec_from_single_step(step),
            [] => panic!("summary_spec missing visible_spec for empty plan"),
            _ => panic!(
                "summary_spec missing visible_spec for multi-step plan: move_id={} steps={:?}",
                self.move_id, self.steps
            ),
        }
    }
}

fn summary_spec_from_single_step(step: &MoveStep) -> MonsterMoveSpec {
    match step {
        MoveStep::Attack(attack) => MonsterMoveSpec::Attack(attack.attack.clone()),
        MoveStep::AddCard(add_card) => MonsterMoveSpec::AddCard(add_card.clone()),
        MoveStep::ApplyPower(power) => {
            if let Some(buff) = buff_from_step(power) {
                MonsterMoveSpec::Buff(buff)
            } else if is_debuff_step(power) {
                let debuff = DebuffSpec {
                    power_id: power.power_id,
                    amount: power.amount,
                    strength: power.visible_strength,
                };
                match debuff.strength {
                    EffectStrength::Strong => MonsterMoveSpec::StrongDebuff(debuff),
                    EffectStrength::Normal => MonsterMoveSpec::Debuff(debuff),
                }
            } else {
                MonsterMoveSpec::Unknown
            }
        }
        MoveStep::Heal(heal) => MonsterMoveSpec::Heal(HealSpec {
            target: heal.target,
            amount: heal.amount,
        }),
        MoveStep::GainBlock(block) if block.target == MoveTarget::SelfTarget => {
            MonsterMoveSpec::Defend(DefendSpec {
                block: block.amount,
            })
        }
        MoveStep::GainBlockRandomMonster(block) => MonsterMoveSpec::Defend(DefendSpec {
            block: block.amount,
        }),
        MoveStep::Utility(_) => MonsterMoveSpec::Unknown,
        MoveStep::Escape => MonsterMoveSpec::Escape,
        MoveStep::Magic => MonsterMoveSpec::Magic,
        MoveStep::Sleep => MonsterMoveSpec::Sleep,
        MoveStep::Stun => MonsterMoveSpec::Stun,
        MoveStep::Debug => MonsterMoveSpec::Debug,
        MoveStep::None => MonsterMoveSpec::None,
        MoveStep::Charge => MonsterMoveSpec::Unknown,
        _ => MonsterMoveSpec::Unknown,
    }
}

fn buff_from_step(power: &ApplyPowerStep) -> Option<BuffSpec> {
    (power.effect == PowerEffectKind::Buff).then_some(BuffSpec {
        power_id: power.power_id,
        amount: power.amount,
    })
}

fn is_debuff_step(power: &ApplyPowerStep) -> bool {
    power.effect == PowerEffectKind::Debuff
}
