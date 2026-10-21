//! Public pre-execution combat-action semantics.
//!
//! This projection deliberately does not execute an action or contain an exact
//! successor. It combines only the current public observation, the public
//! candidate, printed/effective card mechanics, and public potion mechanics.
//! Coverage remains explicit so a proposal owner cannot treat the bounded
//! auxiliary-role vocabulary as a complete model of the card.

use serde::{Deserialize, Serialize};

use crate::agent::information::action::{
    PublicCombatAtomicActionV1, PublicCombatIndexedChoiceCandidateV1,
    PublicCombatIndexedChoiceReasonV1, PublicCombatSelectionFamilyV1,
};
use crate::agent::information::combat::{InformationAccessV1, ObservationEvidenceKindV1};
use crate::agent::information::state::{CombatLearningCardV1, PublicCombatStateV1};
use crate::content::cards::mechanics::{
    card_end_of_turn_block_each_turn_v1, card_mechanic_profile, card_self_hp_loss_on_play_v1,
    x_cost_card_effect_bonus_v1, CardEndOfTurnBlockEachTurnV1, CardMechanicCoverage,
    CardMechanicRole,
};
use crate::content::cards::{get_card_definition, CardId, CardTarget, CardType};
use crate::content::potions::{
    get_potion_definition, potion_mechanic_roles, PotionId, PotionMechanicRole,
};
use crate::content::powers::PowerId;
use crate::content::relics::RelicId;
use crate::sim::combat_action_surface::CombatIndexedChoiceInputEncodingV2;

pub const PUBLIC_COMBAT_ACTION_EFFECT_SCHEMA_NAME: &str = "PublicCombatActionEffect";
pub const PUBLIC_COMBAT_ACTION_EFFECT_SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatActionEffectEnvelopeV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub effect: PublicCombatCandidateEffectV1,
}

impl PublicCombatActionEffectEnvelopeV1 {
    fn new(effect: PublicCombatCandidateEffectV1) -> Self {
        Self {
            schema_name: PUBLIC_COMBAT_ACTION_EFFECT_SCHEMA_NAME.to_string(),
            schema_version: PUBLIC_COMBAT_ACTION_EFFECT_SCHEMA_VERSION,
            effect,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicCombatCandidateEffectV1 {
    PlayCard {
        card: PublicCombatCardEffectV1,
    },
    UsePotion {
        potion: PublicCombatPotionEffectV1,
    },
    DiscardPotion {
        potion_index: usize,
        potion_id: PotionId,
    },
    EndTurn,
    SubmitIndexedChoice {
        input_encoding: CombatIndexedChoiceInputEncodingV2,
        reason: PublicCombatIndexedChoiceReasonV1,
        candidate: PublicCombatIndexedChoiceCandidateV1,
    },
    SelectionFamily {
        family: PublicCombatSelectionFamilyV1,
    },
    Proceed,
    Cancel,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatMonsterReferenceV1 {
    pub monster_index: usize,
    pub monster_slot: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCombatDamageRecipientCertaintyV1 {
    SelectedTarget,
    AllLivingTargets,
    PossibleRandomTarget,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatDisplayedDamageV1 {
    pub recipient: PublicCombatMonsterReferenceV1,
    /// The current public per-hit card projection. This is not total damage
    /// for multi-hit cards and is not an exact successor delta.
    pub amount_per_hit: i32,
    pub recipient_certainty: PublicCombatDamageRecipientCertaintyV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicCombatXCostPaymentV1 {
    SpendAllCurrentEnergy { amount: u8 },
    FreeToPlayOnce,
}

impl PublicCombatXCostPaymentV1 {
    fn amount(self) -> u8 {
        match self {
            Self::SpendAllCurrentEnergy { amount } => amount,
            Self::FreeToPlayOnce => 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatProjectedDamageTotalV1 {
    pub recipient: PublicCombatMonsterReferenceV1,
    /// Sum of the public per-hit values before block and other hit-by-hit
    /// successor mechanics. This remains a projection, not realized HP loss.
    pub amount_before_hit_by_hit_resolution: i32,
    pub recipient_certainty: PublicCombatDamageRecipientCertaintyV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatXCostRepeatedDamageV1 {
    pub hit_count: u32,
    pub projected_totals: Vec<PublicCombatProjectedDamageTotalV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatXCostProjectionV1 {
    /// A direct hand play captures the energy visible at this decision as
    /// `energy_on_use`, even though the card's cached pre-play field may be -1.
    pub captured_energy_on_use: u8,
    pub payment: PublicCombatXCostPaymentV1,
    pub chemical_x_bonus: u8,
    pub card_effect_bonus: u8,
    /// The energy-derived effect after Chemical X and the card-owned upgrade
    /// addition. The concrete card determines what one unit of effect means.
    pub effect_x: u32,
    /// Present only when reviewed card mechanics say that each unit of X
    /// repeats the displayed damage effect.
    pub repeated_damage: Option<PublicCombatXCostRepeatedDamageV1>,
    /// Energy queued by card-play triggers after the X payment boundary (for
    /// example Nunchaku) is not retroactively included in that payment.
    pub post_play_energy_gains_are_not_retroactively_spent: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatCardCostProjectionV1 {
    /// Java-style current card cost. Negative values retain their existing
    /// X-cost or unplayable meaning rather than being guessed into a payment.
    pub effective_cost: i32,
    pub available_energy: u8,
    pub free_to_play_once: bool,
    pub energy_on_use: i32,
    pub x_cost: Option<PublicCombatXCostProjectionV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatCardEffectV1 {
    pub hand_index: usize,
    pub card_id: CardId,
    pub upgrades: u8,
    pub card_type: CardType,
    pub effective_target: CardTarget,
    pub selected_target: Option<PublicCombatMonsterReferenceV1>,
    pub cost: PublicCombatCardCostProjectionV1,
    /// Current public UI/mechanics projection. It is intentionally not
    /// labelled as realized successor damage.
    pub displayed_damage: Vec<PublicCombatDisplayedDamageV1>,
    pub displayed_block: i32,
    /// Raw current card magic number. Its meaning is card-specific and the
    /// projection deliberately does not reinterpret it as draw, stacks, hits,
    /// or another effect without an explicit mechanic role.
    pub displayed_uninterpreted_magic_number: i32,
    /// Coverage applies only to the bounded `CardMechanicRole` vocabulary. It
    /// is not coverage of the card's complete production action sequence.
    pub auxiliary_role_coverage: CardMechanicCoverage,
    /// Positive public facts in the bounded auxiliary-role vocabulary. Their
    /// absence is a negative fact for that vocabulary solely when
    /// `auxiliary_role_coverage` is `Complete`.
    pub auxiliary_roles: Vec<CardMechanicRole>,
    pub exhaust_disposition: PublicCombatCardExhaustDispositionV1,
    pub ethereal: bool,
    pub draw_pile_evidence: ObservationEvidenceKindV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCombatCardExhaustDispositionV1 {
    DoesNotExhaust,
    Exhausts,
    RandomizedByStrangeSpoon,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCombatMechanicAxisV1 {
    Target,
    ImmediateNumerics,
    Status,
    InformationChange,
    PileMovement,
    SelectionProtocol,
    Lifecycle,
    PublicCondition,
    Uncertainty,
    ResourceChange,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCombatCoverageGapReasonV1 {
    CardProductionMechanicsNotReviewed,
    MechanicAxisNotProjected,
    DynamicPaymentNotProjected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatCoverageGapV1 {
    pub axis: PublicCombatMechanicAxisV1,
    pub card_id: CardId,
    pub hand_index: usize,
    pub upgrades: u8,
    pub reason: PublicCombatCoverageGapReasonV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PublicCombatMechanicAxisCoverageStateV1 {
    Known,
    Unknown { gap: PublicCombatCoverageGapV1 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatMechanicCoverageAxesV1 {
    pub target: PublicCombatMechanicAxisCoverageStateV1,
    pub immediate_numerics: PublicCombatMechanicAxisCoverageStateV1,
    pub status: PublicCombatMechanicAxisCoverageStateV1,
    pub information_change: PublicCombatMechanicAxisCoverageStateV1,
    pub pile_movement: PublicCombatMechanicAxisCoverageStateV1,
    pub selection_protocol: PublicCombatMechanicAxisCoverageStateV1,
    pub lifecycle: PublicCombatMechanicAxisCoverageStateV1,
    pub public_condition: PublicCombatMechanicAxisCoverageStateV1,
    pub uncertainty: PublicCombatMechanicAxisCoverageStateV1,
    pub resource_change: PublicCombatMechanicAxisCoverageStateV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "knowledge", rename_all = "snake_case")]
pub enum PublicCombatMechanicalFactV1<T> {
    Known {
        value: T,
    },
    Conditional {
        value: T,
        condition: PublicCombatPublicConditionV1,
    },
    UnknownCoverage {
        gap: PublicCombatCoverageGapV1,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "condition", rename_all = "snake_case")]
pub enum PublicCombatPublicConditionV1 {
    TargetLacksPower {
        target: PublicCombatMonsterReferenceV1,
        power: PowerId,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatStatusApplicationV1 {
    pub target: PublicCombatMonsterReferenceV1,
    pub power: PowerId,
    pub amount: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "boundary", rename_all = "snake_case")]
pub enum PublicCombatInformationBoundaryV1 {
    None,
    DrawCards { count: u32 },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicCombatTargetUncertaintyV1 {
    NoTarget,
    Deterministic,
    RandomTargetPerHit,
    OtherRandomOutcome,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatEnergyPaymentV1 {
    pub amount: u8,
}

/// Queryable facts needed by structured public proposals. This is not a
/// second card executor: only a deliberately reviewed transfer surface returns
/// negative facts as `Known`; every other absence remains `UnknownCoverage`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatCardMechanicalFactsV1 {
    pub coverage: PublicCombatMechanicCoverageAxesV1,
    pub damage_per_hit: PublicCombatMechanicalFactV1<Vec<PublicCombatDisplayedDamageV1>>,
    pub hit_count: PublicCombatMechanicalFactV1<u32>,
    pub block_gain: PublicCombatMechanicalFactV1<i32>,
    pub end_of_turn_block_each_turn: PublicCombatMechanicalFactV1<CardEndOfTurnBlockEachTurnV1>,
    pub status_applications: PublicCombatMechanicalFactV1<Vec<PublicCombatStatusApplicationV1>>,
    pub information_boundary: PublicCombatMechanicalFactV1<PublicCombatInformationBoundaryV1>,
    pub energy_payment: PublicCombatMechanicalFactV1<PublicCombatEnergyPaymentV1>,
    pub self_hp_loss: PublicCombatMechanicalFactV1<i32>,
    pub target_uncertainty: PublicCombatMechanicalFactV1<PublicCombatTargetUncertaintyV1>,
}

impl PublicCombatCardEffectV1 {
    pub fn mechanical_facts_v1(&self) -> PublicCombatCardMechanicalFactsV1 {
        let reviewed = reviewed_public_mechanics(self.card_id);
        let self_hp_loss_on_play = card_self_hp_loss_on_play_v1(self.card_id, self.upgrades);
        let end_of_turn_block_each_turn =
            card_end_of_turn_block_each_turn_v1(self.card_id, self.upgrades);
        let unreviewed_gap = |axis| PublicCombatCoverageGapV1 {
            axis,
            card_id: self.card_id,
            hand_index: self.hand_index,
            upgrades: self.upgrades,
            reason: PublicCombatCoverageGapReasonV1::CardProductionMechanicsNotReviewed,
        };
        let unprojected_gap = |axis| PublicCombatCoverageGapV1 {
            axis,
            card_id: self.card_id,
            hand_index: self.hand_index,
            upgrades: self.upgrades,
            reason: PublicCombatCoverageGapReasonV1::MechanicAxisNotProjected,
        };
        let dynamic_payment_gap = || PublicCombatCoverageGapV1 {
            axis: PublicCombatMechanicAxisV1::ResourceChange,
            card_id: self.card_id,
            hand_index: self.hand_index,
            upgrades: self.upgrades,
            reason: PublicCombatCoverageGapReasonV1::DynamicPaymentNotProjected,
        };
        let axis_coverage = |axis| {
            if axis == PublicCombatMechanicAxisV1::ResourceChange
                && self.cost.effective_cost < 0
                && self.cost.x_cost.is_none()
                && !self.cost.free_to_play_once
            {
                PublicCombatMechanicAxisCoverageStateV1::Unknown {
                    gap: dynamic_payment_gap(),
                }
            } else if (axis == PublicCombatMechanicAxisV1::ResourceChange
                && self_hp_loss_on_play.is_some())
                || (axis == PublicCombatMechanicAxisV1::Status
                    && end_of_turn_block_each_turn.is_some())
                || axis == PublicCombatMechanicAxisV1::Target
                || axis == PublicCombatMechanicAxisV1::Lifecycle
                || (reviewed && axis != PublicCombatMechanicAxisV1::PileMovement)
            {
                PublicCombatMechanicAxisCoverageStateV1::Known
            } else if reviewed {
                PublicCombatMechanicAxisCoverageStateV1::Unknown {
                    gap: unprojected_gap(axis),
                }
            } else {
                PublicCombatMechanicAxisCoverageStateV1::Unknown {
                    gap: unreviewed_gap(axis),
                }
            }
        };
        let coverage = PublicCombatMechanicCoverageAxesV1 {
            target: axis_coverage(PublicCombatMechanicAxisV1::Target),
            immediate_numerics: axis_coverage(PublicCombatMechanicAxisV1::ImmediateNumerics),
            status: axis_coverage(PublicCombatMechanicAxisV1::Status),
            information_change: axis_coverage(PublicCombatMechanicAxisV1::InformationChange),
            pile_movement: axis_coverage(PublicCombatMechanicAxisV1::PileMovement),
            selection_protocol: axis_coverage(PublicCombatMechanicAxisV1::SelectionProtocol),
            lifecycle: axis_coverage(PublicCombatMechanicAxisV1::Lifecycle),
            public_condition: axis_coverage(PublicCombatMechanicAxisV1::PublicCondition),
            uncertainty: axis_coverage(PublicCombatMechanicAxisV1::Uncertainty),
            resource_change: axis_coverage(PublicCombatMechanicAxisV1::ResourceChange),
        };

        let hit_count = if reviewed {
            PublicCombatMechanicalFactV1::Known {
                value: reviewed_hit_count(self),
            }
        } else {
            PublicCombatMechanicalFactV1::UnknownCoverage {
                gap: unreviewed_gap(PublicCombatMechanicAxisV1::ImmediateNumerics),
            }
        };
        let status_applications = if self.card_id == CardId::Bash {
            let target = self
                .selected_target
                .clone()
                .expect("reviewed Bash effect always has its required target");
            PublicCombatMechanicalFactV1::Conditional {
                value: vec![PublicCombatStatusApplicationV1 {
                    target: target.clone(),
                    power: PowerId::Vulnerable,
                    amount: self.displayed_uninterpreted_magic_number,
                }],
                condition: PublicCombatPublicConditionV1::TargetLacksPower {
                    target,
                    power: PowerId::Artifact,
                },
            }
        } else {
            known_or_gap(
                reviewed,
                self.card_id,
                self.hand_index,
                self.upgrades,
                PublicCombatMechanicAxisV1::Status,
                Vec::new(),
            )
        };
        let information_boundary = known_or_gap(
            reviewed,
            self.card_id,
            self.hand_index,
            self.upgrades,
            PublicCombatMechanicAxisV1::InformationChange,
            match self.card_id {
                CardId::PommelStrike => PublicCombatInformationBoundaryV1::DrawCards {
                    count: self.displayed_uninterpreted_magic_number.max(0) as u32,
                },
                CardId::ShrugItOff => PublicCombatInformationBoundaryV1::DrawCards { count: 1 },
                _ => PublicCombatInformationBoundaryV1::None,
            },
        );
        let energy_payment = if let Some(x_cost) = &self.cost.x_cost {
            PublicCombatMechanicalFactV1::Known {
                value: PublicCombatEnergyPaymentV1 {
                    amount: x_cost.payment.amount(),
                },
            }
        } else if self.cost.free_to_play_once {
            PublicCombatMechanicalFactV1::Known {
                value: PublicCombatEnergyPaymentV1 { amount: 0 },
            }
        } else if self.cost.effective_cost >= 0 {
            PublicCombatMechanicalFactV1::Known {
                value: PublicCombatEnergyPaymentV1 {
                    amount: self.cost.effective_cost.min(u8::MAX as i32) as u8,
                },
            }
        } else {
            PublicCombatMechanicalFactV1::UnknownCoverage {
                gap: dynamic_payment_gap(),
            }
        };
        let target_uncertainty = if self
            .auxiliary_roles
            .contains(&CardMechanicRole::RandomTarget)
        {
            PublicCombatMechanicalFactV1::Known {
                value: PublicCombatTargetUncertaintyV1::RandomTargetPerHit,
            }
        } else if self
            .auxiliary_roles
            .contains(&CardMechanicRole::RandomOutcome)
        {
            PublicCombatMechanicalFactV1::Known {
                value: PublicCombatTargetUncertaintyV1::OtherRandomOutcome,
            }
        } else if self.displayed_damage.is_empty() {
            known_or_gap(
                reviewed,
                self.card_id,
                self.hand_index,
                self.upgrades,
                PublicCombatMechanicAxisV1::Uncertainty,
                PublicCombatTargetUncertaintyV1::NoTarget,
            )
        } else {
            known_or_gap(
                reviewed,
                self.card_id,
                self.hand_index,
                self.upgrades,
                PublicCombatMechanicAxisV1::Uncertainty,
                PublicCombatTargetUncertaintyV1::Deterministic,
            )
        };

        PublicCombatCardMechanicalFactsV1 {
            coverage,
            damage_per_hit: if reviewed {
                PublicCombatMechanicalFactV1::Known {
                    value: self.displayed_damage.clone(),
                }
            } else if self.displayed_damage.is_empty() {
                PublicCombatMechanicalFactV1::UnknownCoverage {
                    gap: unreviewed_gap(PublicCombatMechanicAxisV1::ImmediateNumerics),
                }
            } else {
                PublicCombatMechanicalFactV1::Known {
                    value: self.displayed_damage.clone(),
                }
            },
            hit_count,
            block_gain: known_or_gap(
                reviewed,
                self.card_id,
                self.hand_index,
                self.upgrades,
                PublicCombatMechanicAxisV1::ImmediateNumerics,
                self.displayed_block,
            ),
            end_of_turn_block_each_turn: if let Some(fact) = end_of_turn_block_each_turn {
                PublicCombatMechanicalFactV1::Known { value: fact }
            } else {
                known_or_gap(
                    reviewed,
                    self.card_id,
                    self.hand_index,
                    self.upgrades,
                    PublicCombatMechanicAxisV1::Status,
                    CardEndOfTurnBlockEachTurnV1 {
                        amount: 0,
                        persists_for_combat: false,
                        stacks_additively: false,
                        affected_by_dexterity: false,
                        affected_by_frail: false,
                    },
                )
            },
            status_applications,
            information_boundary,
            energy_payment,
            self_hp_loss: if let Some(fact) = self_hp_loss_on_play {
                PublicCombatMechanicalFactV1::Known {
                    value: fact.base_hp_loss,
                }
            } else {
                known_or_gap(
                    reviewed,
                    self.card_id,
                    self.hand_index,
                    self.upgrades,
                    PublicCombatMechanicAxisV1::ResourceChange,
                    0,
                )
            },
            target_uncertainty,
        }
    }
}

fn known_or_gap<T>(
    reviewed: bool,
    card_id: CardId,
    hand_index: usize,
    upgrades: u8,
    axis: PublicCombatMechanicAxisV1,
    value: T,
) -> PublicCombatMechanicalFactV1<T> {
    if reviewed {
        PublicCombatMechanicalFactV1::Known { value }
    } else {
        PublicCombatMechanicalFactV1::UnknownCoverage {
            gap: PublicCombatCoverageGapV1 {
                axis,
                card_id,
                hand_index,
                upgrades,
                reason: PublicCombatCoverageGapReasonV1::CardProductionMechanicsNotReviewed,
            },
        }
    }
}

fn reviewed_public_mechanics(card_id: CardId) -> bool {
    matches!(
        card_id,
        CardId::Strike
            | CardId::Defend
            | CardId::Bash
            | CardId::PommelStrike
            | CardId::ShrugItOff
            | CardId::SwordBoomerang
            | CardId::Whirlwind
            | CardId::Skewer
    )
}

fn reviewed_hit_count(card: &PublicCombatCardEffectV1) -> u32 {
    if let Some(repeated_damage) = card
        .cost
        .x_cost
        .as_ref()
        .and_then(|x_cost| x_cost.repeated_damage.as_ref())
    {
        return repeated_damage.hit_count;
    }
    match card.card_id {
        CardId::SwordBoomerang => card.displayed_uninterpreted_magic_number.max(0) as u32,
        _ if card.displayed_damage.is_empty() => 0,
        _ => 1,
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatPotionEffectV1 {
    pub potion_index: usize,
    pub potion_id: PotionId,
    pub selected_target: Option<PublicCombatMonsterReferenceV1>,
    pub base_potency: i32,
    pub effective_potency: i32,
    pub mechanic_roles: Vec<PotionMechanicRole>,
    pub consumes_potion: bool,
    pub draw_pile_evidence: ObservationEvidenceKindV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicCombatEffectProjectionErrorV1 {
    NonCanonicalTaskDecision,
    NotCombatDecision,
    NotCombatCandidate,
    CandidateOrdinalOutOfRange(usize),
    NonPublicObservation,
    HandIndexOutOfRange(usize),
    PotionIndexOutOfRange(usize),
    EmptyPotionSlot(usize),
    MonsterIndexOutOfRange(usize),
    MissingRequiredMonsterTarget,
    UnexpectedMonsterTarget,
    CardDamageProjectionMisaligned,
    IndexedChoiceSemanticsRequired,
}

impl std::fmt::Display for PublicCombatEffectProjectionErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for PublicCombatEffectProjectionErrorV1 {}

/// Projects one atomic candidate without executing it. Indexed-choice actions
/// require [`public_combat_indexed_choice_effect_v1`] because their public
/// semantics live beside, rather than inside, the atomic action.
pub(crate) fn public_combat_atomic_action_effect_v1(
    observation: &PublicCombatStateV1,
    action: &PublicCombatAtomicActionV1,
) -> Result<PublicCombatActionEffectEnvelopeV1, PublicCombatEffectProjectionErrorV1> {
    require_public(observation)?;
    let effect = match action {
        PublicCombatAtomicActionV1::PlayCard {
            hand_index,
            target_monster_index,
        } => PublicCombatCandidateEffectV1::PlayCard {
            card: project_card_effect(observation, *hand_index, *target_monster_index)?,
        },
        PublicCombatAtomicActionV1::UsePotion {
            potion_index,
            target_monster_index,
        } => PublicCombatCandidateEffectV1::UsePotion {
            potion: project_potion_effect(observation, *potion_index, *target_monster_index)?,
        },
        PublicCombatAtomicActionV1::DiscardPotion { potion_index } => {
            let potion_id = observation
                .potions
                .get(*potion_index)
                .ok_or(PublicCombatEffectProjectionErrorV1::PotionIndexOutOfRange(
                    *potion_index,
                ))?
                .as_ref()
                .ok_or(PublicCombatEffectProjectionErrorV1::EmptyPotionSlot(
                    *potion_index,
                ))?
                .potion_id;
            PublicCombatCandidateEffectV1::DiscardPotion {
                potion_index: *potion_index,
                potion_id,
            }
        }
        PublicCombatAtomicActionV1::EndTurn => PublicCombatCandidateEffectV1::EndTurn,
        PublicCombatAtomicActionV1::SubmitIndexedChoice { .. } => {
            return Err(PublicCombatEffectProjectionErrorV1::IndexedChoiceSemanticsRequired)
        }
        PublicCombatAtomicActionV1::Proceed => PublicCombatCandidateEffectV1::Proceed,
        PublicCombatAtomicActionV1::Cancel => PublicCombatCandidateEffectV1::Cancel,
    };
    Ok(PublicCombatActionEffectEnvelopeV1::new(effect))
}

pub(crate) fn public_combat_indexed_choice_effect_v1(
    observation: &PublicCombatStateV1,
    input_encoding: CombatIndexedChoiceInputEncodingV2,
    reason: &PublicCombatIndexedChoiceReasonV1,
    candidate: PublicCombatIndexedChoiceCandidateV1,
) -> Result<PublicCombatActionEffectEnvelopeV1, PublicCombatEffectProjectionErrorV1> {
    require_public(observation)?;
    Ok(PublicCombatActionEffectEnvelopeV1::new(
        PublicCombatCandidateEffectV1::SubmitIndexedChoice {
            input_encoding,
            reason: reason.clone(),
            candidate,
        },
    ))
}

pub(crate) fn public_combat_selection_effect_v1(
    observation: &PublicCombatStateV1,
    family: &PublicCombatSelectionFamilyV1,
) -> Result<PublicCombatActionEffectEnvelopeV1, PublicCombatEffectProjectionErrorV1> {
    require_public(observation)?;
    Ok(PublicCombatActionEffectEnvelopeV1::new(
        PublicCombatCandidateEffectV1::SelectionFamily {
            family: family.clone(),
        },
    ))
}

fn require_public(
    observation: &PublicCombatStateV1,
) -> Result<(), PublicCombatEffectProjectionErrorV1> {
    if observation.information_access != InformationAccessV1::Public {
        return Err(PublicCombatEffectProjectionErrorV1::NonPublicObservation);
    }
    Ok(())
}

fn monster_reference(
    observation: &PublicCombatStateV1,
    monster_index: usize,
) -> Result<PublicCombatMonsterReferenceV1, PublicCombatEffectProjectionErrorV1> {
    let monster = observation.monsters.get(monster_index).ok_or(
        PublicCombatEffectProjectionErrorV1::MonsterIndexOutOfRange(monster_index),
    )?;
    Ok(PublicCombatMonsterReferenceV1 {
        monster_index,
        monster_slot: monster.slot,
    })
}

fn project_card_effect(
    observation: &PublicCombatStateV1,
    hand_index: usize,
    target_monster_index: Option<usize>,
) -> Result<PublicCombatCardEffectV1, PublicCombatEffectProjectionErrorV1> {
    let card = observation.cards.hand.cards.get(hand_index).ok_or(
        PublicCombatEffectProjectionErrorV1::HandIndexOutOfRange(hand_index),
    )?;
    let definition = get_card_definition(card.card_id);
    let profile = card_mechanic_profile(card.card_id, card.upgrades);
    validate_card_target(profile.effective_target, target_monster_index)?;
    let selected_target = target_monster_index
        .map(|index| monster_reference(observation, index))
        .transpose()?;
    let displayed_damage = displayed_card_damage(
        observation,
        card,
        definition.card_type,
        profile.effective_target,
        target_monster_index,
        profile.roles.contains(CardMechanicRole::RandomTarget),
    )?;
    let x_cost = project_x_cost(
        observation,
        card,
        definition.cost,
        profile.roles.contains(CardMechanicRole::MultiHit),
        &displayed_damage,
    );
    Ok(PublicCombatCardEffectV1 {
        hand_index,
        card_id: card.card_id,
        upgrades: card.upgrades,
        card_type: definition.card_type,
        effective_target: profile.effective_target,
        selected_target,
        cost: PublicCombatCardCostProjectionV1 {
            effective_cost: card.effective_cost,
            available_energy: observation.turn.energy,
            free_to_play_once: card.free_to_play_once,
            energy_on_use: card.energy_on_use,
            x_cost,
        },
        displayed_damage,
        displayed_block: card.current_block,
        displayed_uninterpreted_magic_number: card.current_magic_number,
        auxiliary_role_coverage: profile.coverage,
        auxiliary_roles: profile.roles.iter().collect(),
        exhaust_disposition: effective_card_exhaust_disposition(
            observation,
            card,
            definition.card_type,
            profile.exhausts_when_played,
        ),
        ethereal: profile.ethereal,
        draw_pile_evidence: observation.cards.draw.evidence,
    })
}

fn project_x_cost(
    observation: &PublicCombatStateV1,
    card: &CombatLearningCardV1,
    printed_cost: i8,
    repeats_displayed_damage: bool,
    displayed_damage: &[PublicCombatDisplayedDamageV1],
) -> Option<PublicCombatXCostProjectionV1> {
    if printed_cost != -1 {
        return None;
    }

    let captured_energy_on_use = observation.turn.energy;
    let payment = if card.free_to_play_once {
        PublicCombatXCostPaymentV1::FreeToPlayOnce
    } else {
        PublicCombatXCostPaymentV1::SpendAllCurrentEnergy {
            amount: captured_energy_on_use,
        }
    };
    let chemical_x_bonus = u8::from(
        observation
            .player
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::ChemicalX),
    ) * 2;
    let card_effect_bonus = x_cost_card_effect_bonus_v1(card.card_id, card.upgrades)
        .expect("printed X-cost card must have an X-cost effect profile");
    let effect_x = u32::from(captured_energy_on_use)
        .saturating_add(u32::from(chemical_x_bonus))
        .saturating_add(u32::from(card_effect_bonus));
    let repeated_damage = (repeats_displayed_damage && !displayed_damage.is_empty()).then(|| {
        let hit_count_i32 = i32::try_from(effect_x).unwrap_or(i32::MAX);
        PublicCombatXCostRepeatedDamageV1 {
            hit_count: effect_x,
            projected_totals: displayed_damage
                .iter()
                .map(|damage| PublicCombatProjectedDamageTotalV1 {
                    recipient: damage.recipient.clone(),
                    amount_before_hit_by_hit_resolution: damage
                        .amount_per_hit
                        .saturating_mul(hit_count_i32),
                    recipient_certainty: damage.recipient_certainty,
                })
                .collect(),
        }
    });

    Some(PublicCombatXCostProjectionV1 {
        captured_energy_on_use,
        payment,
        chemical_x_bonus,
        card_effect_bonus,
        effect_x,
        repeated_damage,
        post_play_energy_gains_are_not_retroactively_spent: true,
    })
}

fn effective_card_exhaust_disposition(
    observation: &PublicCombatStateV1,
    card: &CombatLearningCardV1,
    card_type: CardType,
    printed_exhaust: bool,
) -> PublicCombatCardExhaustDispositionV1 {
    let has_power = |power| {
        observation
            .player
            .powers
            .iter()
            .any(|state| state.power == power)
    };
    let has_relic = |relic| {
        observation
            .player
            .relics
            .iter()
            .any(|state| state.id == relic)
    };
    let exhausts = card.exhaust_override.unwrap_or(printed_exhaust)
        || (card_type == CardType::Skill && has_power(PowerId::Corruption))
        || (card_type == CardType::Status && has_relic(RelicId::MedicalKit))
        || (card_type == CardType::Curse && has_relic(RelicId::BlueCandle));

    if !exhausts {
        PublicCombatCardExhaustDispositionV1::DoesNotExhaust
    } else if has_relic(RelicId::StrangeSpoon) {
        PublicCombatCardExhaustDispositionV1::RandomizedByStrangeSpoon
    } else {
        PublicCombatCardExhaustDispositionV1::Exhausts
    }
}

fn validate_card_target(
    target: CardTarget,
    target_monster_index: Option<usize>,
) -> Result<(), PublicCombatEffectProjectionErrorV1> {
    let requires_monster = matches!(target, CardTarget::Enemy | CardTarget::SelfAndEnemy);
    if requires_monster && target_monster_index.is_none() {
        return Err(PublicCombatEffectProjectionErrorV1::MissingRequiredMonsterTarget);
    }
    if !requires_monster && target_monster_index.is_some() {
        return Err(PublicCombatEffectProjectionErrorV1::UnexpectedMonsterTarget);
    }
    Ok(())
}

fn displayed_card_damage(
    observation: &PublicCombatStateV1,
    card: &CombatLearningCardV1,
    card_type: CardType,
    target: CardTarget,
    target_monster_index: Option<usize>,
    random_target: bool,
) -> Result<Vec<PublicCombatDisplayedDamageV1>, PublicCombatEffectProjectionErrorV1> {
    if card_type != CardType::Attack {
        return Ok(Vec::new());
    }
    if card.damage_by_monster_order.len() != observation.monsters.len() {
        return Err(PublicCombatEffectProjectionErrorV1::CardDamageProjectionMisaligned);
    }
    let targets = if let Some(index) = target_monster_index {
        vec![(
            index,
            PublicCombatDamageRecipientCertaintyV1::SelectedTarget,
        )]
    } else if random_target {
        living_monster_indices(observation)
            .into_iter()
            .map(|index| {
                (
                    index,
                    PublicCombatDamageRecipientCertaintyV1::PossibleRandomTarget,
                )
            })
            .collect()
    } else if matches!(target, CardTarget::AllEnemy | CardTarget::All) {
        living_monster_indices(observation)
            .into_iter()
            .map(|index| {
                (
                    index,
                    PublicCombatDamageRecipientCertaintyV1::AllLivingTargets,
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    targets
        .into_iter()
        .map(|(index, certainty)| {
            Ok(PublicCombatDisplayedDamageV1 {
                recipient: monster_reference(observation, index)?,
                amount_per_hit: card.damage_by_monster_order[index],
                recipient_certainty: certainty,
            })
        })
        .collect()
}

fn living_monster_indices(observation: &PublicCombatStateV1) -> Vec<usize> {
    observation
        .monsters
        .iter()
        .enumerate()
        .filter_map(|(index, monster)| monster.alive.then_some(index))
        .collect()
}

fn project_potion_effect(
    observation: &PublicCombatStateV1,
    potion_index: usize,
    target_monster_index: Option<usize>,
) -> Result<PublicCombatPotionEffectV1, PublicCombatEffectProjectionErrorV1> {
    let potion = observation
        .potions
        .get(potion_index)
        .ok_or(PublicCombatEffectProjectionErrorV1::PotionIndexOutOfRange(
            potion_index,
        ))?
        .as_ref()
        .ok_or(PublicCombatEffectProjectionErrorV1::EmptyPotionSlot(
            potion_index,
        ))?;
    if potion.requires_target && target_monster_index.is_none() {
        return Err(PublicCombatEffectProjectionErrorV1::MissingRequiredMonsterTarget);
    }
    if !potion.requires_target && target_monster_index.is_some() {
        return Err(PublicCombatEffectProjectionErrorV1::UnexpectedMonsterTarget);
    }
    let selected_target = target_monster_index
        .map(|index| monster_reference(observation, index))
        .transpose()?;
    let definition = get_potion_definition(potion.potion_id);
    let sacred_bark = observation
        .player
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::SacredBark);
    Ok(PublicCombatPotionEffectV1 {
        potion_index,
        potion_id: potion.potion_id,
        selected_target,
        base_potency: definition.base_potency,
        effective_potency: if sacred_bark {
            definition.base_potency.saturating_mul(2)
        } else {
            definition.base_potency
        },
        mechanic_roles: potion_mechanic_roles(potion.potion_id).to_vec(),
        consumes_potion: true,
        draw_pile_evidence: observation.cards.draw.evidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::information::action::project_public_combat_actions_v1;
    use crate::agent::information::state::public_combat_state_v1;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::content::potions::{Potion, PotionId};
    use crate::runtime::combat::CombatCard;
    use crate::state::EngineState;

    #[test]
    fn card_effects_expose_public_display_and_reviewed_roles_without_a_successor() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Bash, 11),
            CombatCard::for_scenario(CardId::Defend, 22),
        ];
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 77;
        monster.slot = 3;
        combat.entities.monsters = vec![monster];
        let observation = public_combat_state_v1(&combat);
        let actions = project_public_combat_actions_v1(&EngineState::CombatPlayerTurn, &combat)
            .expect("public actions");

        let bash_action = actions
            .public
            .atomic_actions
            .iter()
            .find(|action| {
                matches!(
                    action,
                    PublicCombatAtomicActionV1::PlayCard {
                        hand_index: 0,
                        target_monster_index: Some(0)
                    }
                )
            })
            .expect("Bash action");
        let bash =
            public_combat_atomic_action_effect_v1(&observation, bash_action).expect("Bash effect");
        let PublicCombatCandidateEffectV1::PlayCard { card: bash } = bash.effect else {
            panic!("expected card effect")
        };
        assert_eq!(bash.card_id, CardId::Bash);
        assert_eq!(bash.auxiliary_role_coverage, CardMechanicCoverage::Complete);
        assert!(bash
            .auxiliary_roles
            .contains(&CardMechanicRole::ApplyVulnerable));
        assert_eq!(bash.selected_target.expect("target").monster_slot, 3);
        assert_eq!(bash.displayed_damage[0].amount_per_hit, 8);

        let defend_action = actions
            .public
            .atomic_actions
            .iter()
            .find(|action| {
                matches!(
                    action,
                    PublicCombatAtomicActionV1::PlayCard {
                        hand_index: 1,
                        target_monster_index: None
                    }
                )
            })
            .expect("Defend action");
        let defend = public_combat_atomic_action_effect_v1(&observation, defend_action)
            .expect("Defend effect");
        let PublicCombatCandidateEffectV1::PlayCard { card: defend } = defend.effect else {
            panic!("expected card effect")
        };
        assert_eq!(defend.displayed_block, 5);
        assert!(defend.displayed_damage.is_empty());
    }

    #[test]
    fn potion_effect_uses_public_identity_target_and_sacred_bark_potency() {
        let mut combat = crate::test_support::blank_test_combat();
        combat
            .entities
            .player
            .add_relic(crate::content::relics::RelicState::new(RelicId::SacredBark));
        combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 41))];
        let mut monster = crate::test_support::test_monster(EnemyId::Cultist);
        monster.id = 91;
        monster.slot = 2;
        combat.entities.monsters = vec![monster];
        let observation = public_combat_state_v1(&combat);
        let action = PublicCombatAtomicActionV1::UsePotion {
            potion_index: 0,
            target_monster_index: Some(0),
        };

        let effect =
            public_combat_atomic_action_effect_v1(&observation, &action).expect("potion effect");
        let PublicCombatCandidateEffectV1::UsePotion { potion } = effect.effect else {
            panic!("expected potion effect")
        };
        assert_eq!(potion.potion_id, PotionId::FirePotion);
        assert_eq!(potion.effective_potency, 40);
        assert!(potion
            .mechanic_roles
            .contains(&PotionMechanicRole::DirectDamage));
        assert_eq!(potion.selected_target.expect("target").monster_slot, 2);
        assert!(potion.consumes_potion);
    }

    #[test]
    fn random_target_multi_hit_stays_possible_per_hit_instead_of_fake_total_damage() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::SwordBoomerang, 11)];
        let mut first = crate::test_support::test_monster(EnemyId::Cultist);
        first.id = 71;
        first.slot = 1;
        let mut second = crate::test_support::test_monster(EnemyId::JawWorm);
        second.id = 72;
        second.slot = 5;
        combat.entities.monsters = vec![first, second];
        let observation = public_combat_state_v1(&combat);
        let action = PublicCombatAtomicActionV1::PlayCard {
            hand_index: 0,
            target_monster_index: None,
        };

        let effect = public_combat_atomic_action_effect_v1(&observation, &action)
            .expect("random-target effect");
        let PublicCombatCandidateEffectV1::PlayCard { card } = effect.effect else {
            panic!("expected card effect")
        };
        assert!(card.auxiliary_roles.contains(&CardMechanicRole::MultiHit));
        assert!(card
            .auxiliary_roles
            .contains(&CardMechanicRole::RandomTarget));
        assert_eq!(card.displayed_damage.len(), 2);
        assert!(card.displayed_damage.iter().all(|damage| {
            damage.recipient_certainty
                == PublicCombatDamageRecipientCertaintyV1::PossibleRandomTarget
                && damage.amount_per_hit == 3
        }));

        let facts = card.mechanical_facts_v1();
        assert_eq!(
            facts.hit_count,
            PublicCombatMechanicalFactV1::Known { value: 3 }
        );
        assert_eq!(
            facts.target_uncertainty,
            PublicCombatMechanicalFactV1::Known {
                value: PublicCombatTargetUncertaintyV1::RandomTargetPerHit
            }
        );
    }

    #[test]
    fn x_cost_effect_exposes_full_payment_effect_count_and_free_chemical_x_boundary() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.turn.energy = 3;
        let mut whirlwind = CombatCard::for_scenario(CardId::Whirlwind, 11);
        whirlwind.upgrades = 1;
        combat.zones.hand = vec![whirlwind];
        combat.entities.monsters = vec![crate::test_support::test_monster(EnemyId::Cultist)];
        let action = PublicCombatAtomicActionV1::PlayCard {
            hand_index: 0,
            target_monster_index: None,
        };

        let observation = public_combat_state_v1(&combat);
        let effect =
            public_combat_atomic_action_effect_v1(&observation, &action).expect("Whirlwind effect");
        let PublicCombatCandidateEffectV1::PlayCard { card } = effect.effect else {
            panic!("expected card effect")
        };
        let x_cost = card.cost.x_cost.as_ref().expect("typed X-cost projection");
        assert_eq!(
            (
                effect.schema_version,
                x_cost.captured_energy_on_use,
                x_cost.payment,
                x_cost.chemical_x_bonus,
                x_cost.card_effect_bonus,
                x_cost.effect_x,
                x_cost
                    .repeated_damage
                    .as_ref()
                    .expect("repeated damage")
                    .hit_count,
                x_cost
                    .repeated_damage
                    .as_ref()
                    .expect("repeated damage")
                    .projected_totals[0]
                    .amount_before_hit_by_hit_resolution,
                x_cost.post_play_energy_gains_are_not_retroactively_spent,
            ),
            (
                3,
                3,
                PublicCombatXCostPaymentV1::SpendAllCurrentEnergy { amount: 3 },
                0,
                0,
                3,
                3,
                24,
                true,
            )
        );
        let facts = card.mechanical_facts_v1();
        assert_eq!(
            (facts.energy_payment, facts.hit_count),
            (
                PublicCombatMechanicalFactV1::Known {
                    value: PublicCombatEnergyPaymentV1 { amount: 3 }
                },
                PublicCombatMechanicalFactV1::Known { value: 3 },
            )
        );

        combat
            .entities
            .player
            .add_relic(crate::content::relics::RelicState::new(RelicId::ChemicalX));
        combat.zones.hand[0].free_to_play_once = true;
        let observation = public_combat_state_v1(&combat);
        let effect = public_combat_atomic_action_effect_v1(&observation, &action)
            .expect("free Chemical X Whirlwind effect");
        let PublicCombatCandidateEffectV1::PlayCard { card } = effect.effect else {
            panic!("expected card effect")
        };
        let x_cost = card.cost.x_cost.expect("typed X-cost projection");
        assert_eq!(
            (
                x_cost.payment,
                x_cost.captured_energy_on_use,
                x_cost.chemical_x_bonus,
                x_cost.effect_x,
                x_cost
                    .repeated_damage
                    .expect("repeated damage")
                    .projected_totals[0]
                    .amount_before_hit_by_hit_resolution,
            ),
            (PublicCombatXCostPaymentV1::FreeToPlayOnce, 3, 2, 5, 40)
        );
    }

    #[test]
    fn reviewed_parameterized_facts_keep_conditions_and_information_boundaries_typed() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Bash, 11),
            CombatCard::for_scenario(CardId::PommelStrike, 12),
            CombatCard::for_scenario(CardId::ShrugItOff, 13),
        ];
        combat.entities.monsters = vec![crate::test_support::test_monster(EnemyId::Cultist)];
        let observation = public_combat_state_v1(&combat);
        let actions = project_public_combat_actions_v1(&EngineState::CombatPlayerTurn, &combat)
            .expect("public actions");
        let card = |hand_index: usize, target_monster_index: Option<usize>| {
            let action = actions
                .public
                .atomic_actions
                .iter()
                .find(|action| {
                    matches!(
                        action,
                        PublicCombatAtomicActionV1::PlayCard {
                            hand_index: candidate_hand_index,
                            target_monster_index: candidate_target,
                        } if *candidate_hand_index == hand_index && *candidate_target == target_monster_index
                    )
                })
                .expect("card action");
            let effect =
                public_combat_atomic_action_effect_v1(&observation, action).expect("card effect");
            let PublicCombatCandidateEffectV1::PlayCard { card } = effect.effect else {
                panic!("expected card effect")
            };
            card
        };

        let bash = card(0, Some(0)).mechanical_facts_v1();
        let PublicCombatMechanicalFactV1::Conditional { value, condition } =
            bash.status_applications
        else {
            panic!("Bash vulnerable must retain its public Artifact condition")
        };
        assert_eq!(value[0].power, PowerId::Vulnerable);
        assert_eq!(value[0].amount, 2);
        assert!(matches!(
            condition,
            PublicCombatPublicConditionV1::TargetLacksPower {
                power: PowerId::Artifact,
                ..
            }
        ));

        let pommel = card(1, Some(0)).mechanical_facts_v1();
        assert_eq!(
            pommel.information_boundary,
            PublicCombatMechanicalFactV1::Known {
                value: PublicCombatInformationBoundaryV1::DrawCards { count: 1 }
            }
        );
        let shrug = card(2, None).mechanical_facts_v1();
        assert_eq!(
            shrug.information_boundary,
            PublicCombatMechanicalFactV1::Known {
                value: PublicCombatInformationBoundaryV1::DrawCards { count: 1 }
            }
        );
        assert!(matches!(
            shrug.coverage.pile_movement,
            PublicCombatMechanicAxisCoverageStateV1::Unknown { .. }
        ));
    }

    #[test]
    fn direct_self_hp_loss_is_known_even_when_other_card_axes_are_unreviewed() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Hemokinesis, 11),
            CombatCard::for_scenario(CardId::Bloodletting, 12),
            CombatCard::for_scenario(CardId::Offering, 13),
            CombatCard::for_scenario(CardId::JAX, 14),
        ];
        combat.entities.monsters = vec![crate::test_support::test_monster(EnemyId::Cultist)];
        let observation = public_combat_state_v1(&combat);
        let actions = project_public_combat_actions_v1(&EngineState::CombatPlayerTurn, &combat)
            .expect("public actions");

        for (hand_index, target_monster_index, expected_hp_loss) in
            [(0, Some(0), 2), (1, None, 3), (2, None, 6), (3, None, 3)]
        {
            let action = actions
                .public
                .atomic_actions
                .iter()
                .find(|action| {
                    matches!(
                        action,
                        PublicCombatAtomicActionV1::PlayCard {
                            hand_index: candidate_hand_index,
                            target_monster_index: candidate_target,
                        } if *candidate_hand_index == hand_index
                            && *candidate_target == target_monster_index
                    )
                })
                .expect("self-loss card action");
            let effect = public_combat_atomic_action_effect_v1(&observation, action)
                .expect("self-loss card effect");
            let PublicCombatCandidateEffectV1::PlayCard { card } = effect.effect else {
                panic!("expected card effect")
            };
            let facts = card.mechanical_facts_v1();
            assert_eq!(
                facts.self_hp_loss,
                PublicCombatMechanicalFactV1::Known {
                    value: expected_hp_loss
                }
            );
            assert_eq!(
                facts.coverage.resource_change,
                PublicCombatMechanicAxisCoverageStateV1::Known
            );
        }
    }

    #[test]
    fn recurring_end_of_turn_block_is_known_for_combat_card_actions() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Metallicize, 21)];
        let observation = public_combat_state_v1(&combat);
        let actions = project_public_combat_actions_v1(&EngineState::CombatPlayerTurn, &combat)
            .expect("public actions");
        let action = actions
            .public
            .atomic_actions
            .iter()
            .find(|action| {
                matches!(
                    action,
                    PublicCombatAtomicActionV1::PlayCard {
                        hand_index: 0,
                        target_monster_index: None,
                    }
                )
            })
            .expect("Metallicize action");
        let effect = public_combat_atomic_action_effect_v1(&observation, action)
            .expect("Metallicize effect");
        let PublicCombatCandidateEffectV1::PlayCard { card } = effect.effect else {
            panic!("expected card effect")
        };
        let facts = card.mechanical_facts_v1();
        let PublicCombatMechanicalFactV1::Known { value } = facts.end_of_turn_block_each_turn
        else {
            panic!("expected known recurring Block")
        };
        assert_eq!(value.amount, 3);
        assert!(value.persists_for_combat);
        assert!(value.stacks_additively);
        assert!(!value.affected_by_dexterity);
        assert!(!value.affected_by_frail);
        assert_eq!(
            facts.coverage.status,
            PublicCombatMechanicAxisCoverageStateV1::Known
        );
    }
}
