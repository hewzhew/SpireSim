//! Durable access to the existing replay-proved research artifact. No search.
use super::*;
use crate::storage::write_new_synced;
use std::fs;
use sts_oracle_learning::ai::planner_core::stable_planner_id;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedPlans {
    /// The report's producer is independent of the scratch game's origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_identity: Option<RunPlayEvidenceIdentityV1>,
    pub max_search_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_generation_work: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_potions_used: Option<u32>,
    pub search_budget_exhausted: bool,
    pub artifact: RunPlayCombatResearchArtifactV1,
}

enum PlanBinding {
    Current,
    DifferentProducer,
    DifferentRoot,
}

impl SavedPlans {
    pub fn id(&self) -> Result<String, String> {
        stable_planner_id("run_play_saved_plans_v1", self)
    }

    pub fn save(&self, controller: &RunPlayControllerV1) -> Result<String, String> {
        self.artifact.validate()?;
        self.check_root(controller)?;
        let payload = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if payload.len() > DEFAULT_ARTIFACT_MAX_BYTES {
            return Err("saved combat plans exceed artifact size limit".into());
        }
        let path = controller.paths().root().join("combat-plans.json");
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let temp = path.with_extension(format!("{}-{nonce}.tmp", std::process::id()));
        write_new_synced(&temp, &payload, "saved combat plans")?;
        fs::rename(&temp, &path).map_err(|e| format!("cannot publish saved combat plans: {e}"))?;
        self.id()
    }

    fn binding(&self, controller: &RunPlayControllerV1) -> Result<PlanBinding, String> {
        if self.producer_identity.as_ref() != Some(&RunPlayEvidenceIdentityV1::current_v1()) {
            return Ok(PlanBinding::DifferentProducer);
        }
        let a = &self.artifact;
        let head = controller.head();
        let published = controller.paths().load_head()?;
        if published.session_id != head.session_id || published.head_revision != head.head_revision
        {
            return Err(
                "current position changed; read the current decision before using plans".into(),
            );
        }
        let source = crate::decision::decision_source(head)?;
        // Mark edits advance the metadata head without moving the game state.
        // Bind the live handle to the exact state revision as well as its root;
        // actual play and undo advance that revision and invalidate it.
        if a.session_id != source.session_id
            || a.root_state_revision != head.state_revision
            || a.root_archive_node_id != source.archive_node_id
            || a.root_public_snapshot_id != source.public_snapshot_id
            || a.source_evidence_identity != head.evidence_identity
        {
            return Ok(PlanBinding::DifferentRoot);
        }
        Ok(PlanBinding::Current)
    }

    fn check_root(&self, controller: &RunPlayControllerV1) -> Result<(), String> {
        match self.binding(controller)? {
            PlanBinding::Current => Ok(()),
            PlanBinding::DifferentProducer => Err("saved combat plans have an absent or different producer build; run research explicitly".into()),
            PlanBinding::DifferentRoot => Err("saved combat plans belong to a different head; run research explicitly".into()),
        }
    }

    fn read_file(controller: &RunPlayControllerV1) -> Result<Option<Self>, String> {
        let path = controller.paths().root().join("combat-plans.json");
        if !path.exists() {
            return Ok(None);
        }
        let bytes = read_bounded(&path, DEFAULT_ARTIFACT_MAX_BYTES, "saved combat plans")?;
        let saved: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("invalid saved combat plans: {e}"))?;
        saved.artifact.validate()?;
        Ok(Some(saved))
    }

    /// Optional presentation uses the same binding checks as review/commit.
    /// Missing and stale reports are ordinary absence, not a new search.
    pub fn current_if_present(controller: &RunPlayControllerV1) -> Result<Option<Self>, String> {
        let Some(saved) = Self::read_file(controller)? else {
            return Ok(None);
        };
        match saved.binding(controller)? {
            PlanBinding::Current => Ok(Some(saved)),
            _ => Ok(None),
        }
    }

    pub fn view(
        &self,
        controller: &RunPlayControllerV1,
        diagnostics: bool,
    ) -> Result<String, String> {
        let decision = controller
            .head()
            .current_decision
            .as_ref()
            .ok_or("combat plans require a current decision")?
            .public_decision_v1()?;
        let TaskPublicObservationV1::Combat {
            observation: root, ..
        } = &decision.observation
        else {
            return Err("combat plans require a combat decision".into());
        };
        Ok(crate::combat_reading::research(
            root,
            &self.artifact.candidates,
            &self.artifact.bounded_progress,
            Some(self.max_search_ms),
            &self.artifact.lanes,
            combat_research_options_for_effort_v1(
                &controller.head().combat_plan_options,
                self.artifact.effort,
                Some(self.max_search_ms),
            )
            .satisfaction,
            self.max_generation_work,
            diagnostics,
        ))
    }

    pub fn load(
        controller: &RunPlayControllerV1,
        expected_id: Option<&str>,
    ) -> Result<Self, String> {
        let saved =
            Self::read_file(controller)?.ok_or("no saved combat plans; run research explicitly")?;
        saved.check_root(controller)?;
        if expected_id.is_some_and(|id| saved.id().as_deref() != Ok(id)) {
            return Err("combat report changed; read the saved plans again".into());
        }
        Ok(saved)
    }

    pub fn candidate(&self, reference: &str) -> Result<&AgentCombatResearchCandidateV1, String> {
        let index = reference
            .strip_prefix('c')
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|i| reference == format!("c{i}"))
            .ok_or("invalid saved candidate reference")?;
        self.artifact
            .candidates
            .get(index)
            .ok_or_else(|| "candidate is absent from saved plans".into())
    }
}
