use super::enums::*;

impl StateAbstractionBoundaryId {
    pub fn label(self) -> &'static str {
        match self {
            StateAbstractionBoundaryId::StarterBasicDuplicatePlayCardByTarget => {
                "starter_basic_duplicate_play_card_by_target"
            }
            StateAbstractionBoundaryId::PendingChoiceIdenticalRuntimeCard => {
                "pending_choice_identical_runtime_card"
            }
            StateAbstractionBoundaryId::TurnSequenceOrderSensitive => {
                "turn_sequence_order_sensitive"
            }
        }
    }
}

impl StateAbstractionConsumer {
    pub fn label(self) -> &'static str {
        match self {
            StateAbstractionConsumer::ProofPrune => "proof_prune",
            StateAbstractionConsumer::LocalActionDedup => "local_action_dedup",
            StateAbstractionConsumer::EstimateShare => "estimate_share",
            StateAbstractionConsumer::CandidateOrdering => "candidate_ordering",
            StateAbstractionConsumer::ReportOnly => "report_only",
        }
    }
}

impl StateAbstractionLatentDebtKind {
    pub fn label(self) -> &'static str {
        match self {
            StateAbstractionLatentDebtKind::DiscardOrder => "discard_order",
            StateAbstractionLatentDebtKind::CardIdentity => "card_identity",
            StateAbstractionLatentDebtKind::TurnPlayedCardHistory => "turn_played_card_history",
            StateAbstractionLatentDebtKind::ImmediatePublicState => "immediate_public_state",
            StateAbstractionLatentDebtKind::TerminalClass => "terminal_class",
            StateAbstractionLatentDebtKind::LegalActionSet => "legal_action_set",
            StateAbstractionLatentDebtKind::OtherRuntime => "other_runtime",
            StateAbstractionLatentDebtKind::Unknown => "unknown",
        }
    }
}

impl StateAbstractionCandidateLevel {
    pub fn label(self) -> &'static str {
        match self {
            StateAbstractionCandidateLevel::HorizonLimitedCandidate => "horizon_limited_candidate",
            StateAbstractionCandidateLevel::IdentityAuditCandidate => "identity_audit_candidate",
            StateAbstractionCandidateLevel::ReportOnlyBlocked => "report_only_blocked",
            StateAbstractionCandidateLevel::ReportOnlyUnknown => "report_only_unknown",
        }
    }
}

impl StateAbstractionRevealGate {
    pub fn label(self) -> &'static str {
        match self {
            StateAbstractionRevealGate::NextDraw => "next_draw",
            StateAbstractionRevealGate::NextShuffle => "next_shuffle",
            StateAbstractionRevealGate::NextRandomCall => "next_random_call",
            StateAbstractionRevealGate::NextCardSelection => "next_card_selection",
            StateAbstractionRevealGate::NextRelicCounterRead => "next_relic_counter_read",
            StateAbstractionRevealGate::NextLegalActionGeneration => "next_legal_action_generation",
            StateAbstractionRevealGate::CombatEnd => "combat_end",
            StateAbstractionRevealGate::CurrentActionResolution => "current_action_resolution",
            StateAbstractionRevealGate::Unknown => "unknown",
        }
    }
}

impl StateDivergenceKind {
    pub fn label(self) -> &'static str {
        match self {
            StateDivergenceKind::ImmediatePublicDelta => "immediate_public_delta",
            StateDivergenceKind::LegalActionDelta => "legal_action_delta",
            StateDivergenceKind::TerminalDelta => "terminal_delta",
            StateDivergenceKind::DrawPileOrderDelta => "draw_pile_order_delta",
            StateDivergenceKind::DiscardOrderDelta => "discard_order_delta",
            StateDivergenceKind::HandOrderDelta => "hand_order_delta",
            StateDivergenceKind::ExhaustOrderDelta => "exhaust_order_delta",
            StateDivergenceKind::RngStateDelta => "rng_state_delta",
            StateDivergenceKind::RelicCounterDelta => "relic_counter_delta",
            StateDivergenceKind::TurnRuntimeDelta => "turn_runtime_delta",
            StateDivergenceKind::TurnDrawModifierDelta => "turn_draw_modifier_delta",
            StateDivergenceKind::TurnActionCounterDelta => "turn_action_counter_delta",
            StateDivergenceKind::TurnPlayedCardHistoryDelta => "turn_played_card_history_delta",
            StateDivergenceKind::TurnDiscardCounterDelta => "turn_discard_counter_delta",
            StateDivergenceKind::TurnOrbHistoryDelta => "turn_orb_history_delta",
            StateDivergenceKind::TurnCombatFlagDelta => "turn_combat_flag_delta",
            StateDivergenceKind::MonsterRuntimeDelta => "monster_runtime_delta",
            StateDivergenceKind::CombatRuntimeHintDelta => "combat_runtime_hint_delta",
            StateDivergenceKind::PotionStateDelta => "potion_state_delta",
            StateDivergenceKind::PlayerFutureDelta => "player_future_delta",
            StateDivergenceKind::ZoneRuntimeDelta => "zone_runtime_delta",
            StateDivergenceKind::EngineRuntimeDelta => "engine_runtime_delta",
            StateDivergenceKind::CombatMetaDelta => "combat_meta_delta",
            StateDivergenceKind::CardLineageDelta => "card_lineage_delta",
            StateDivergenceKind::PendingQueueDelta => "pending_queue_delta",
            StateDivergenceKind::IdentityOnlyCandidate => "identity_only_candidate",
            StateDivergenceKind::Unknown => "unknown",
        }
    }
}
