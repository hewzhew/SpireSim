use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum BossMatchupClaimStatus {
    Supported,
    WeakSupported,
    Unsupported,
    Uncertain,
    Unknown,
    NotPresent,
    SingleSlowSource,
}

impl BossMatchupClaimStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::WeakSupported => "weak_supported",
            Self::Unsupported => "unsupported",
            Self::Uncertain => "uncertain",
            Self::Unknown => "unknown",
            Self::NotPresent => "not_present",
            Self::SingleSlowSource => "single_slow_source",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum BossMatchupClaimScope {
    StaticOnly,
    ReviewOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum BossMatchupClaimConfidence {
    Provisional,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum BossMatchupPolicyConsumability {
    HumanOnly,
    ShadowPressure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BossMatchupShadowPressureKindV1 {
    AwakenedCultistCleanup,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BossMatchupShadowPressureV1 {
    pub kind: BossMatchupShadowPressureKindV1,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BossMatchupEvidenceClaim {
    pub id: &'static str,
    pub status: BossMatchupClaimStatus,
    pub support: Vec<String>,
    pub counterevidence: Vec<String>,
    pub unknown: Vec<String>,
    pub scope: BossMatchupClaimScope,
    pub confidence: BossMatchupClaimConfidence,
    pub policy_consumability: BossMatchupPolicyConsumability,
}

#[derive(Clone, Debug, Serialize)]
pub struct BossMatchupInputSummary {
    pub deck_size: usize,
    pub energy: u8,
    pub has_runic_dome: bool,
    pub deck: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BossMatchupEvidenceFrame {
    pub schema: &'static str,
    pub contract: &'static str,
    pub boss: &'static str,
    pub input: BossMatchupInputSummary,
    pub claims: Vec<BossMatchupEvidenceClaim>,
}
