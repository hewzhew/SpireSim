use serde::{Deserialize, Serialize};

use crate::ai::planner_core::{PlannerDecisionSite, PublicDecisionDomainV1};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskDecisionSiteV1 {
    Combat,
    Map,
    CardReward,
    Neow,
    Event,
    Shop,
    Campfire,
    BossRelic,
    Reward,
    RunChoice,
    Treasure,
    RunTransition,
}

impl TaskDecisionSiteV1 {
    pub fn public_domain(self) -> PublicDecisionDomainV1 {
        match self {
            Self::Combat => PublicDecisionDomainV1::Combat,
            _ => PublicDecisionDomainV1::Strategic,
        }
    }
}

impl From<PlannerDecisionSite> for TaskDecisionSiteV1 {
    fn from(value: PlannerDecisionSite) -> Self {
        match value {
            PlannerDecisionSite::Map => Self::Map,
            PlannerDecisionSite::CardReward => Self::CardReward,
            PlannerDecisionSite::Neow => Self::Neow,
            PlannerDecisionSite::Event => Self::Event,
            PlannerDecisionSite::Shop => Self::Shop,
            PlannerDecisionSite::Campfire => Self::Campfire,
            PlannerDecisionSite::BossRelic => Self::BossRelic,
            PlannerDecisionSite::Reward => Self::Reward,
            PlannerDecisionSite::RunChoice => Self::RunChoice,
            PlannerDecisionSite::Treasure => Self::Treasure,
            PlannerDecisionSite::RunTransition => Self::RunTransition,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoReloadReasonV1 {
    UnverifiedOriginalGameBoundary,
    UnverifiedSimulatorParity,
    DisabledByTask,
}

/// The original autosave generation from which the run is reconstructed.
/// This is protocol identity, never a simulator checkpoint type.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeReloadAnchorV1 {
    EnterRoomAutosave,
    PostNeowAutosave,
    PostCombatAutosave,
    EndlessNeowAutosave,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IrreversibleCommitBoundaryV1 {
    AutosaveGenerationReplacedOrDeleted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum NativeSaveModelV1 {
    Unverified,
    SourceAuditedAutosaveGenerations {
        game_build: String,
        source_reference: String,
    },
}

impl NativeSaveModelV1 {
    fn validate(&self) -> Result<(), String> {
        if let Self::SourceAuditedAutosaveGenerations {
            game_build,
            source_reference,
        } = self
        {
            if game_build.trim().is_empty() {
                return Err("native save model lacks a game build".to_string());
            }
            if source_reference.trim().is_empty() {
                return Err("native save model lacks a source reference".to_string());
            }
        }
        Ok(())
    }

    fn is_source_audited(&self) -> bool {
        matches!(self, Self::SourceAuditedAutosaveGenerations { .. })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "authorization", rename_all = "snake_case")]
pub enum ReloadAuthorizationV1 {
    NoReload {
        reason: NoReloadReasonV1,
    },
    NativeAutosaveScopes {
        native_save_model: NativeSaveModelV1,
        simulator_contract_reference: String,
    },
    /// Research/training authority to restore an exact simulator scope root
    /// after observing either its next public scope or a game terminal. This
    /// is intentionally not described as an original-game save permission.
    ExactBranchingScopes {
        simulator_contract_reference: String,
    },
}

impl ReloadAuthorizationV1 {
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::NativeAutosaveScopes {
                native_save_model,
                simulator_contract_reference,
            } => {
                native_save_model.validate()?;
                if !native_save_model.is_source_audited() {
                    return Err(
                        "native autosave reload requires a source-audited save model".to_string(),
                    );
                }
                if simulator_contract_reference.trim().is_empty() {
                    return Err(
                        "native autosave reload lacks a simulator parity reference".to_string()
                    );
                }
            }
            Self::ExactBranchingScopes {
                simulator_contract_reference,
            } => {
                if simulator_contract_reference.trim().is_empty() {
                    return Err(
                        "exact branching reload lacks a simulator contract reference".to_string(),
                    );
                }
            }
            Self::NoReload { .. } => {}
        }
        Ok(())
    }

    fn permits_native_autosave_scopes(&self) -> bool {
        matches!(self, Self::NativeAutosaveScopes { .. })
    }

    fn permits_exact_branching_scopes(&self) -> bool {
        matches!(self, Self::ExactBranchingScopes { .. })
    }
}

/// Permission belongs to one native autosave scope, not a decision-site name.
/// A scope may contain reward, card-reward, and map decisions before the next
/// autosave replaces its anchor.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "permission", rename_all = "snake_case")]
pub enum ReloadPermissionV1 {
    NoReload {
        reason: NoReloadReasonV1,
    },
    ReloadToNativeAnchor {
        native_anchor: NativeReloadAnchorV1,
        irreversible_commit: IrreversibleCommitBoundaryV1,
    },
    /// Restore an in-memory exact scope root after observing a branch exit.
    /// This permission is for research/planning and carries no claim about
    /// the original game's disk save behavior.
    ReloadToExactScopeRoot,
}

impl ReloadPermissionV1 {
    pub fn permits_reload(&self) -> bool {
        !matches!(self, Self::NoReload { .. })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReloadProtocolV1 {
    pub protocol_name: String,
    pub authorization: ReloadAuthorizationV1,
}

impl ReloadProtocolV1 {
    /// Original PC save semantics are known, but deployment remains fail-closed
    /// until a simulator adapter proves parity with native autosave scopes.
    pub fn fail_closed() -> Self {
        Self {
            protocol_name: "original_pc_v2_3_4_autosave_fail_closed_v1".to_string(),
            authorization: ReloadAuthorizationV1::NoReload {
                reason: NoReloadReasonV1::UnverifiedSimulatorParity,
            },
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.protocol_name.trim().is_empty() {
            return Err("reload protocol name must not be empty".to_string());
        }
        self.authorization.validate()
    }

    pub fn validate_scope_permission(&self, permission: &ReloadPermissionV1) -> Result<(), String> {
        match permission {
            ReloadPermissionV1::NoReload { .. } => Ok(()),
            ReloadPermissionV1::ReloadToNativeAnchor { .. }
                if self.authorization.permits_native_autosave_scopes() =>
            {
                Ok(())
            }
            ReloadPermissionV1::ReloadToExactScopeRoot
                if self.authorization.permits_exact_branching_scopes() =>
            {
                Ok(())
            }
            ReloadPermissionV1::ReloadToNativeAnchor { .. } => {
                Err("task protocol does not authorize native autosave reload".to_string())
            }
            ReloadPermissionV1::ReloadToExactScopeRoot => {
                Err("task protocol does not authorize exact branching reload".to_string())
            }
        }
    }
}
