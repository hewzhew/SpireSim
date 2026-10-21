use super::{CandidateAction, PressureKind, StrategicDebt};
use crate::ai::card_component_signal_v1::{CardComponentSignalKindV1, CardComponentSignalReportV1};
use crate::ai::deck_startup_profile_v1::{
    startup_snecko_cost_conversion_candidate_v1, DeckStartupProfileV1,
};
use crate::content::cards::CardId;
use serde::{Deserialize, Serialize};

pub const COMPONENT_SIGNAL_LEDGER_AMOUNT: f32 = 0.35;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateRole {
    Enabler,
    Payoff,
    Lubricant,
    Transition,
    BossAnswer,
    DeckCleaning,
    ResourceConversion,
    DefensivePatch,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictHint {
    Reject,
    SkipPreferred,
    Speculative,
    ContextTake,
    StrongTake,
    MustTake,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LedgerDelta {
    pub kind: PressureKind,
    pub amount: f32,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OpportunityCost {
    pub label: String,
    pub severity: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StrategicContraindication {
    pub label: String,
    pub severity: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CandidateDelta {
    pub action: CandidateAction,
    pub positive: Vec<LedgerDelta>,
    pub negative: Vec<LedgerDelta>,
    pub role: CandidateRole,
    pub verdict_hint: VerdictHint,
    pub opportunity_costs: Vec<OpportunityCost>,
    pub contraindications: Vec<StrategicContraindication>,
    pub notes: Vec<String>,
    pub evidence: Vec<String>,
}

impl CandidateDelta {
    pub fn empty(action: CandidateAction) -> Self {
        Self {
            action,
            positive: Vec::new(),
            negative: Vec::new(),
            role: CandidateRole::Unknown,
            verdict_hint: VerdictHint::Speculative,
            opportunity_costs: Vec::new(),
            contraindications: Vec::new(),
            notes: Vec::new(),
            evidence: Vec::new(),
        }
    }

    pub fn apply_component_signals_v1(&mut self, report: &CardComponentSignalReportV1) {
        if self.role == CandidateRole::Unknown {
            self.role = component_role(report);
        }
        for signal in &report.positive_signals {
            let kind = positive_component_signal_pressure(*signal);
            if kind == PressureKind::BranchDiversityNeed {
                self.notes
                    .push(format!("component_report_only:{}", signal.label()));
                continue;
            }
            self.positive.push(LedgerDelta {
                kind,
                amount: COMPONENT_SIGNAL_LEDGER_AMOUNT,
                reason: signal.label().to_string(),
            });
        }
        self.negative
            .extend(report.debt_signals.iter().map(|signal| LedgerDelta {
                kind: negative_component_signal_pressure(*signal),
                amount: COMPONENT_SIGNAL_LEDGER_AMOUNT,
                reason: signal.label().to_string(),
            }));
        self.notes.extend(
            report
                .note_signals
                .iter()
                .map(|note| note.label().to_string()),
        );
        self.evidence
            .push("card_component_signal contributor".to_string());
    }

    pub fn positive_amount(&self) -> f32 {
        self.positive.iter().map(|delta| delta.amount).sum()
    }

    pub fn negative_amount(&self) -> f32 {
        self.negative.iter().map(|delta| delta.amount).sum::<f32>()
            + self
                .opportunity_costs
                .iter()
                .map(|cost| cost.severity)
                .sum::<f32>()
            + self
                .contraindications
                .iter()
                .map(|contraindication| contraindication.severity * 2.0)
                .sum::<f32>()
    }
}

pub fn add_snecko_cost_conversion_delta_v1(
    delta: &mut CandidateDelta,
    startup: &DeckStartupProfileV1,
    candidate: CardId,
) {
    let Some(reason) = startup_snecko_cost_conversion_candidate_v1(startup, candidate) else {
        return;
    };

    if !delta.positive.iter().any(|entry| entry.reason != reason) {
        delta.notes.push(format!(
            "{reason}:candidate_has_no_positive_function_signal"
        ));
        return;
    }
    if delta.positive.iter().any(|entry| entry.reason == reason) {
        return;
    }

    delta.positive.push(LedgerDelta {
        kind: PressureKind::DeckDebt(StrategicDebt::SetupDebt),
        amount: 0.35,
        reason: reason.to_string(),
    });
    delta.evidence.push(reason.to_string());
}

fn component_role(report: &CardComponentSignalReportV1) -> CandidateRole {
    use crate::ai::card_component_signal_v1::CardComponentRoleV1;
    if report.roles.contains(&CardComponentRoleV1::Mitigation) {
        CandidateRole::BossAnswer
    } else if report.roles.contains(&CardComponentRoleV1::Enabler) {
        CandidateRole::Enabler
    } else if report.roles.contains(&CardComponentRoleV1::Lubricant) {
        CandidateRole::Lubricant
    } else if report.roles.contains(&CardComponentRoleV1::Payoff) {
        CandidateRole::Payoff
    } else if report.roles.contains(&CardComponentRoleV1::Transition) {
        CandidateRole::Transition
    } else {
        CandidateRole::Unknown
    }
}

fn positive_component_signal_pressure(signal: CardComponentSignalKindV1) -> PressureKind {
    use super::StrategicJob;
    match signal {
        CardComponentSignalKindV1::DamageMitigation => {
            PressureKind::MissingJob(StrategicJob::Block)
        }
        CardComponentSignalKindV1::DrawEnergyAccess => {
            PressureKind::MissingJob(StrategicJob::DrawEnergy)
        }
        CardComponentSignalKindV1::ExhaustAccess
        | CardComponentSignalKindV1::ExhaustEngineEnabler
        | CardComponentSignalKindV1::FnpEngineUnlock
        | CardComponentSignalKindV1::ExhaustPayoffSupported => {
            PressureKind::MissingJob(StrategicJob::ExhaustAccess)
        }
        CardComponentSignalKindV1::BlockPayoffSupported => {
            PressureKind::MissingJob(StrategicJob::Scaling)
        }
        CardComponentSignalKindV1::SelfDamagePayoffSupported
        | CardComponentSignalKindV1::StrengthPayoffConvertibleBurstSupported
        | CardComponentSignalKindV1::StrengthPayoffSupported => {
            PressureKind::MissingJob(StrategicJob::Scaling)
        }
        CardComponentSignalKindV1::FormationNeedProgress => PressureKind::BranchDiversityNeed,
        _ => PressureKind::BranchDiversityNeed,
    }
}

fn negative_component_signal_pressure(signal: CardComponentSignalKindV1) -> PressureKind {
    use super::StrategicDebt;
    match signal {
        CardComponentSignalKindV1::PayoffWithoutVisibleGapFill
        | CardComponentSignalKindV1::ExhaustPayoffUnsupported
        | CardComponentSignalKindV1::BlockPayoffUnsupported
        | CardComponentSignalKindV1::SelfDamagePayoffUnsupported
        | CardComponentSignalKindV1::StrengthPayoffWithoutStableGenerator
        | CardComponentSignalKindV1::StrengthPayoffUnsupported => {
            PressureKind::DeckDebt(StrategicDebt::PayoffWithoutEnabler)
        }
        CardComponentSignalKindV1::SneckoEnergyDiscountDebt => {
            PressureKind::DeckDebt(StrategicDebt::SetupDebt)
        }
        _ => PressureKind::DeckDebt(StrategicDebt::CombatShapeRisk),
    }
}
