use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateAbstractionBoundaryId {
    StarterBasicDuplicatePlayCardByTarget,
    PendingChoiceIdenticalRuntimeCard,
    TurnSequenceOrderSensitive,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateAbstractionBoundaryScope {
    LocalActionList,
    CombatSearchAnalysis,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateAbstractionSoundnessLevel {
    ExactStructural,
    LocalActionEquivalent,
    HorizonExact,
    PublicObservationEquivalent,
    EstimateOnly,
    CandidateOnly,
    ReportOnly,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateAbstractionConsumer {
    ProofPrune,
    LocalActionDedup,
    EstimateShare,
    CandidateOrdering,
    ReportOnly,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateAbstractionRevealGate {
    NextDraw,
    NextShuffle,
    NextRandomCall,
    NextCardSelection,
    NextRelicCounterRead,
    NextLegalActionGeneration,
    CombatEnd,
    CurrentActionResolution,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateDivergenceKind {
    ImmediatePublicDelta,
    LegalActionDelta,
    TerminalDelta,
    DrawPileOrderDelta,
    DiscardOrderDelta,
    HandOrderDelta,
    ExhaustOrderDelta,
    RngStateDelta,
    RelicCounterDelta,
    TurnRuntimeDelta,
    TurnDrawModifierDelta,
    TurnActionCounterDelta,
    TurnPlayedCardHistoryDelta,
    TurnDiscardCounterDelta,
    TurnOrbHistoryDelta,
    TurnCombatFlagDelta,
    MonsterRuntimeDelta,
    CombatRuntimeHintDelta,
    PotionStateDelta,
    PlayerFutureDelta,
    ZoneRuntimeDelta,
    EngineRuntimeDelta,
    CombatMetaDelta,
    CardLineageDelta,
    PendingQueueDelta,
    IdentityOnlyCandidate,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateAbstractionLatentDebtKind {
    DiscardOrder,
    CardIdentity,
    TurnPlayedCardHistory,
    ImmediatePublicState,
    TerminalClass,
    LegalActionSet,
    OtherRuntime,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateAbstractionCandidateLevel {
    HorizonLimitedCandidate,
    IdentityAuditCandidate,
    ReportOnlyBlocked,
    ReportOnlyUnknown,
}
