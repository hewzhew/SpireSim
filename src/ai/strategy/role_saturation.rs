use crate::ai::analysis::card_semantics::Mechanic;
use crate::ai::strategy::boss_scaling_evidence::admission_is_strength_payoff;
use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
use crate::ai::strategy::reward_admission::{
    RewardAdmission, RewardAdmissionClass, RewardAdmissionReason,
};
use crate::ai::strategy::reward_quality::RewardDuplicateConcern;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoleSaturationCandidate {
    pub upgrades: u8,
    pub is_shop_card: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaneCap {
    ProbeOnly,
    Reject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarginalUtilityReason {
    CycleBlockSaturated,
    DuplicateAccessCopy,
    DuplicateBlockPayoff,
    DuplicateStrengthPayoff,
    DuplicateUnupgradedPayoff,
    ThickDeckImmediateWork,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoleSaturationPenalty {
    pub reason: MarginalUtilityReason,
    pub score_delta: i32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RoleSaturationAssessment {
    pub penalties: Vec<RoleSaturationPenalty>,
    pub lane_cap: Option<LaneCap>,
}

pub fn assess_role_saturation(
    deck: DeckPlanSnapshot,
    candidate: Option<RoleSaturationCandidate>,
    admission: Option<&RewardAdmission>,
) -> RoleSaturationAssessment {
    let Some(candidate) = candidate else {
        return RoleSaturationAssessment::default();
    };
    let Some(admission) = admission else {
        return RoleSaturationAssessment::default();
    };

    let mut assessment = RoleSaturationAssessment::default();
    if is_cycle_block(admission) {
        if deck.roles.cycle_block_units >= 4 {
            assessment.add(
                MarginalUtilityReason::CycleBlockSaturated,
                -90,
                LaneCap::ProbeOnly,
            );
        } else if deck.roles.cycle_block_units >= 3 && deck.deck_size >= 22 {
            assessment.add(
                MarginalUtilityReason::CycleBlockSaturated,
                -70,
                LaneCap::ProbeOnly,
            );
        }
    }

    if admission_damage_uses(admission, Mechanic::Block) && deck.roles.block_payoff_units >= 1 {
        if candidate.upgrades == 0 {
            assessment.add(
                MarginalUtilityReason::DuplicateUnupgradedPayoff,
                -100,
                LaneCap::ProbeOnly,
            );
        } else {
            assessment.add(
                MarginalUtilityReason::DuplicateBlockPayoff,
                -80,
                LaneCap::ProbeOnly,
            );
        }
    }

    if admission_is_strength_payoff(admission)
        && !admission_recovers_current_hp(admission)
        && !deck.has_supported_hand_exhaust_conversion(admission)
        && deck.roles.strength_payoff_units >= strength_payoff_capacity(deck)
    {
        assessment.add(
            MarginalUtilityReason::DuplicateStrengthPayoff,
            if candidate.upgrades == 0 { -95 } else { -70 },
            LaneCap::ProbeOnly,
        );
    }

    if has_duplicate_access_copy(admission) {
        assessment.add(
            MarginalUtilityReason::DuplicateAccessCopy,
            -70,
            LaneCap::ProbeOnly,
        );
    }

    if deck.context.act >= 3
        && deck.deck_size >= 24
        && generic_immediate_work(admission)
        && !candidate.is_shop_card
    {
        assessment.add(
            MarginalUtilityReason::ThickDeckImmediateWork,
            -55,
            LaneCap::ProbeOnly,
        );
    }

    if candidate.is_shop_card && assessment.lane_cap == Some(LaneCap::ProbeOnly) {
        assessment.lane_cap = Some(LaneCap::Reject);
    }
    assessment
}

impl RoleSaturationAssessment {
    fn add(&mut self, reason: MarginalUtilityReason, score_delta: i32, lane_cap: LaneCap) {
        self.penalties.push(RoleSaturationPenalty {
            reason,
            score_delta,
        });
        self.lane_cap = Some(stricter_cap(self.lane_cap, lane_cap));
    }
}

pub fn marginal_reason_label(reason: MarginalUtilityReason) -> &'static str {
    match reason {
        MarginalUtilityReason::CycleBlockSaturated => "cycle-block-saturated",
        MarginalUtilityReason::DuplicateAccessCopy => "duplicate-access-copy",
        MarginalUtilityReason::DuplicateBlockPayoff => "duplicate-block-payoff",
        MarginalUtilityReason::DuplicateStrengthPayoff => "duplicate-strength-payoff",
        MarginalUtilityReason::DuplicateUnupgradedPayoff => "duplicate-unupgraded-payoff",
        MarginalUtilityReason::ThickDeckImmediateWork => "thick-deck-immediate",
    }
}

fn strength_payoff_capacity(deck: DeckPlanSnapshot) -> u8 {
    if deck.roles.strength_source_units >= 2 {
        2
    } else {
        1
    }
}

fn stricter_cap(current: Option<LaneCap>, next: LaneCap) -> LaneCap {
    match (current, next) {
        (Some(LaneCap::Reject), _) | (_, LaneCap::Reject) => LaneCap::Reject,
        _ => LaneCap::ProbeOnly,
    }
}

fn is_cycle_block(admission: &RewardAdmission) -> bool {
    admission_provides(admission, Mechanic::Block)
        && admission_provides(admission, Mechanic::CardDraw)
}

fn generic_immediate_work(admission: &RewardAdmission) -> bool {
    matches!(
        admission.class,
        RewardAdmissionClass::ImmediateWork | RewardAdmissionClass::BurdenedImmediateWork
    ) && !admission_provides(admission, Mechanic::CardDraw)
        && !admission_provides(admission, Mechanic::Energy)
        && !admission_provides(admission, Mechanic::Weak)
        && !admission_provides(admission, Mechanic::Vulnerable)
        && !admission_provides(admission, Mechanic::EnemyStrengthDown)
        && !admission_provides(admission, Mechanic::TemporaryEnemyStrengthDown)
        && !admission_provides(admission, Mechanic::Strength)
        && !admission.reasons.iter().any(|reason| {
            matches!(
                reason,
                RewardAdmissionReason::AreaDamage
                    | RewardAdmissionReason::CombatUpgrade
                    | RewardAdmissionReason::RunReward(_)
                    | RewardAdmissionReason::Installs(_)
                    | RewardAdmissionReason::Supports(_)
                    | RewardAdmissionReason::Closes(_)
                    | RewardAdmissionReason::DamageUses(_)
            )
        })
}

fn admission_provides(admission: &RewardAdmission, mechanic: Mechanic) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::Provides(mechanic))
}

fn admission_damage_uses(admission: &RewardAdmission, mechanic: Mechanic) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::DamageUses(mechanic))
}

fn has_duplicate_access_copy(admission: &RewardAdmission) -> bool {
    admission.reasons.iter().any(|reason| {
        matches!(
            reason,
            RewardAdmissionReason::DuplicateConcern(RewardDuplicateConcern::DiminishingAccessCopy)
        )
    })
}

fn admission_recovers_current_hp(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::RecoverCurrentHp)
}
