use super::types::*;

pub(super) fn latent_debt_kind(
    divergence_kind: StateDivergenceKind,
) -> StateAbstractionLatentDebtKind {
    match divergence_kind {
        StateDivergenceKind::DiscardOrderDelta => StateAbstractionLatentDebtKind::DiscardOrder,
        StateDivergenceKind::CardLineageDelta => StateAbstractionLatentDebtKind::CardIdentity,
        StateDivergenceKind::TurnPlayedCardHistoryDelta => {
            StateAbstractionLatentDebtKind::TurnPlayedCardHistory
        }
        StateDivergenceKind::ImmediatePublicDelta => {
            StateAbstractionLatentDebtKind::ImmediatePublicState
        }
        StateDivergenceKind::TerminalDelta => StateAbstractionLatentDebtKind::TerminalClass,
        StateDivergenceKind::LegalActionDelta => StateAbstractionLatentDebtKind::LegalActionSet,
        StateDivergenceKind::Unknown => StateAbstractionLatentDebtKind::Unknown,
        _ => StateAbstractionLatentDebtKind::OtherRuntime,
    }
}

pub(super) fn candidate_level(
    divergence_kind: StateDivergenceKind,
    latent_debt_kind: StateAbstractionLatentDebtKind,
    first_divergence_path: Option<&'static str>,
    guessed_reveal_gate: StateAbstractionRevealGate,
) -> StateAbstractionCandidateLevel {
    match (
        divergence_kind,
        latent_debt_kind,
        first_divergence_path,
        guessed_reveal_gate,
    ) {
        (
            StateDivergenceKind::DiscardOrderDelta,
            StateAbstractionLatentDebtKind::DiscardOrder,
            Some("combat.zones.discard_pile"),
            StateAbstractionRevealGate::NextShuffle,
        ) => StateAbstractionCandidateLevel::HorizonLimitedCandidate,
        (
            StateDivergenceKind::CardLineageDelta,
            StateAbstractionLatentDebtKind::CardIdentity,
            Some("combat.zones.discard_pile.lineage_order"),
            StateAbstractionRevealGate::NextShuffle,
        ) => StateAbstractionCandidateLevel::IdentityAuditCandidate,
        (_, StateAbstractionLatentDebtKind::Unknown, _, _) => {
            StateAbstractionCandidateLevel::ReportOnlyUnknown
        }
        _ => StateAbstractionCandidateLevel::ReportOnlyBlocked,
    }
}

pub(super) fn primary_divergence(
    histogram: &[StateAbstractionDivergenceInput],
) -> StateAbstractionDivergenceInput {
    histogram
        .iter()
        .max_by(|left, right| {
            left.groups
                .cmp(&right.groups)
                .then_with(|| divergence_rank(right.kind).cmp(&divergence_rank(left.kind)))
                .then_with(|| right.kind.cmp(&left.kind))
        })
        .cloned()
        .unwrap_or(StateAbstractionDivergenceInput {
            kind: StateDivergenceKind::Unknown,
            first_divergence_path: None,
            guessed_reveal_gate: StateAbstractionRevealGate::Unknown,
            groups: 0,
        })
}

fn divergence_rank(kind: StateDivergenceKind) -> u8 {
    match kind {
        StateDivergenceKind::TerminalDelta => 0,
        StateDivergenceKind::LegalActionDelta => 1,
        StateDivergenceKind::ImmediatePublicDelta => 2,
        StateDivergenceKind::HandOrderDelta => 3,
        StateDivergenceKind::DrawPileOrderDelta => 4,
        StateDivergenceKind::DiscardOrderDelta => 5,
        StateDivergenceKind::ExhaustOrderDelta => 6,
        StateDivergenceKind::RngStateDelta => 7,
        StateDivergenceKind::CardLineageDelta => 8,
        StateDivergenceKind::TurnRuntimeDelta => 9,
        StateDivergenceKind::TurnDrawModifierDelta => 10,
        StateDivergenceKind::TurnActionCounterDelta => 11,
        StateDivergenceKind::TurnPlayedCardHistoryDelta => 12,
        StateDivergenceKind::TurnDiscardCounterDelta => 13,
        StateDivergenceKind::TurnOrbHistoryDelta => 14,
        StateDivergenceKind::TurnCombatFlagDelta => 15,
        StateDivergenceKind::MonsterRuntimeDelta => 16,
        StateDivergenceKind::CombatRuntimeHintDelta => 17,
        StateDivergenceKind::PotionStateDelta => 18,
        StateDivergenceKind::RelicCounterDelta => 19,
        StateDivergenceKind::PlayerFutureDelta => 20,
        StateDivergenceKind::ZoneRuntimeDelta => 21,
        StateDivergenceKind::EngineRuntimeDelta => 22,
        StateDivergenceKind::CombatMetaDelta => 23,
        StateDivergenceKind::PendingQueueDelta => 24,
        StateDivergenceKind::IdentityOnlyCandidate => 25,
        StateDivergenceKind::Unknown => 26,
    }
}
