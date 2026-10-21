//! Bounded analyst-selected reward sequences, compiled privately and adopted
//! through the same per-action executor as complete combat drafts.
use crate::{
    draft_workspace::write_atomic_new,
    manual_draft::require_same,
    manual_replay::{adopted_scopes, replay_manual_scope_observed},
    LockstepProgressV1, ManualDraftSourceV1, ManualSessionV1,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use sts_parity_lockstep::{
    DecisionSurfaceV1, LockstepAdapterV1, LockstepRunnerV1, ParitySnapshotV1,
    SemanticActionSpaceV1, SemanticActionV1,
};
use sts_parity_lockstep_java::RecordedJavaEvidenceV1;
use sts_parity_lockstep_rust::RustLockstepAdapterV1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RewardPlanStepV1 {
    Potion,
    Gold,
    Cards,
}

impl RewardPlanStepV1 {
    fn id(self) -> &'static str {
        match self {
            Self::Potion => "potion",
            Self::Gold => "gold",
            Self::Cards => "card",
        }
    }
}

pub fn parse_reward_plan_steps_v1(words: &[&str]) -> Result<Vec<RewardPlanStepV1>, String> {
    let steps = words
        .iter()
        .map(|word| match *word {
            "potion" => Ok(RewardPlanStepV1::Potion),
            "gold" => Ok(RewardPlanStepV1::Gold),
            "cards" => Ok(RewardPlanStepV1::Cards),
            _ => Err("reward plan accepts potion, gold, cards; cards must be last".to_owned()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_steps(&steps)?;
    Ok(steps)
}

fn validate_steps(steps: &[RewardPlanStepV1]) -> Result<(), String> {
    if steps.is_empty()
        || steps.len() > 3
        || steps.last() != Some(&RewardPlanStepV1::Cards)
        || steps
            .iter()
            .enumerate()
            .any(|(i, s)| steps[..i].contains(s))
    {
        return Err("use each reward kind at most once and finish with cards".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualRewardPlanV1 {
    schema: String,
    source: ManualDraftSourceV1,
    steps: Vec<RewardPlanStepV1>,
    root: ParitySnapshotV1,
    actions: Vec<SemanticActionV1>,
    successors: Vec<ParitySnapshotV1>,
}

impl ManualRewardPlanV1 {
    pub fn actions(&self) -> &[SemanticActionV1] {
        &self.actions
    }

    pub(crate) fn from_selected(
        source: ManualDraftSourceV1,
        root: &RustLockstepAdapterV1,
        actions: &[SemanticActionV1],
    ) -> Result<Self, String> {
        let steps = actions
            .iter()
            .map(|action| match action {
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::CombatReward,
                    choice,
                } if choice.kind == "run_reward" => match choice.id.as_str() {
                    "potion" => Ok(RewardPlanStepV1::Potion),
                    "gold" => Ok(RewardPlanStepV1::Gold),
                    "card" => Ok(RewardPlanStepV1::Cards),
                    _ => Err("selected branch is outside the bounded reward plan".to_owned()),
                },
                _ => Err("selected branch is outside the bounded reward plan".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let plan = Self::compile(source, root, steps)?;
        if plan.actions != actions {
            return Err("selected rewards differ from the unique compiled choices".into());
        }
        Ok(plan)
    }

    /// No Java execution. All selectors must resolve uniquely before saving.
    pub fn compile(
        source: ManualDraftSourceV1,
        rust: &RustLockstepAdapterV1,
        steps: Vec<RewardPlanStepV1>,
    ) -> Result<Self, String> {
        validate_steps(&steps)?;
        let mut worker = rust.clone();
        let root = worker.observe().map_err(|e| e.message)?;
        let mut snapshot = root.clone();
        let mut actions = Vec::new();
        let mut successors = Vec::new();
        for step in &steps {
            if worker.session().active_combat.is_some()
                || snapshot.boundary_kind != "natural_combat_reward"
            {
                return Err("reward plan requires the post-combat rewards page".into());
            }
            let space = SemanticActionSpaceV1::from_snapshot(&snapshot)?;
            let matches = space
                .actions
                .into_iter()
                .filter(|a| {
                    matches!(&a.action,
                SemanticActionV1::Select { surface: DecisionSurfaceV1::CombatReward, choice }
                    if choice.kind == "run_reward" && choice.id == step.id())
                })
                .collect::<Vec<_>>();
            let action = match matches.as_slice() {
                [one] => one.action.clone(),
                [] => {
                    return Err(format!(
                    "{} reward missing or unavailable (check potion capacity); no actions adopted",
                    step.id()
                ))
                }
                _ => {
                    return Err(format!(
                        "{} reward is ambiguous; select a specific reward first",
                        step.id()
                    ))
                }
            };
            let potion_count = worker.session().run_state.potions.iter().flatten().count();
            let prepared = worker.prepare(&action, &snapshot).map_err(|e| e.message)?;
            snapshot = worker.execute(prepared).map_err(|e| e.message)?;
            if *step == RewardPlanStepV1::Potion
                && worker.session().run_state.potions.iter().flatten().count() != potion_count + 1
            {
                return Err(
                    "potion was not obtained in the private preview; no actions adopted".into(),
                );
            }
            actions.push(action);
            successors.push(snapshot.clone());
        }
        if snapshot.boundary_kind != "natural_card_reward"
            || worker.session().active_combat.is_some()
        {
            return Err("reward plan did not stop at the run card-choice page".into());
        }
        Ok(Self {
            schema: "sts-manual-reward-plan-v1".into(),
            source,
            steps,
            root,
            actions,
            successors,
        })
    }

    pub fn summary(&self) -> String {
        use sts_parity_lockstep::SemanticScalarV1;
        self.actions
            .iter()
            .map(|action| match action {
                SemanticActionV1::Select { choice, .. } => match choice.id.as_str() {
                    "potion" => match choice.attributes.get("potion") {
                        Some(SemanticScalarV1::Text(potion)) => format!("take {potion}"),
                        _ => "invalid potion choice".into(),
                    },
                    "gold" => match choice.attributes.get("amount") {
                        Some(SemanticScalarV1::Integer(amount)) => format!("take {amount} gold"),
                        _ => "invalid gold choice".into(),
                    },
                    "card" => {
                        let cards = choice
                            .attributes
                            .iter()
                            .filter_map(|(key, value)| match (key.starts_with("card_"), value) {
                                (true, SemanticScalarV1::Text(card)) => Some(card.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join(" / ");
                        format!("open cards [{cards}]; stop before choosing")
                    }
                    _ => "invalid reward choice".into(),
                },
                _ => "invalid reward action".into(),
            })
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    pub fn terminal(&self) -> Option<&ParitySnapshotV1> {
        self.successors.last()
    }

    fn verify(&self, origin: &RustLockstepAdapterV1) -> Result<Self, String> {
        if self.schema != "sts-manual-reward-plan-v1" {
            return Err("unsupported reward plan".into());
        }
        let replayed = Self::compile(self.source.clone(), origin, self.steps.clone())?;
        require_same("reward plan source", &self.root, &replayed.root)?;
        if self.actions != replayed.actions || self.successors.len() != replayed.successors.len() {
            return Err("reward plan actions changed in current replay".into());
        }
        for (expected, actual) in self.successors.iter().zip(&replayed.successors) {
            require_same("reward plan successor", expected, actual)?;
        }
        Ok(replayed)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SavedRewardPlanV1 {
    worker: crate::draft_workspace::DraftWorker,
    plan: ManualRewardPlanV1,
}

pub fn save_manual_reward_plan_v1(
    session: &ManualSessionV1,
    rust: &RustLockstepAdapterV1,
    steps: Vec<RewardPlanStepV1>,
) -> Result<(String, String), String> {
    let count = RecordedJavaEvidenceV1::load_complete(&session.evidence_directory())?
        .steps
        .len();
    let plan = ManualRewardPlanV1::compile(
        ManualDraftSourceV1 {
            session_id: session.session_id().into(),
            committed_actions: count,
        },
        rust,
        steps,
    )?;
    save_reward_plan(
        session,
        &plan,
        crate::draft_workspace::DraftWorker::capture(session.repository())?,
    )
}

pub(crate) fn save_reward_plan(
    session: &ManualSessionV1,
    plan: &ManualRewardPlanV1,
    worker: crate::draft_workspace::DraftWorker,
) -> Result<(String, String), String> {
    let (_, handle, summary) = save_reward_plan_reference(session, plan, worker)?;
    Ok((handle, summary))
}

pub(crate) fn save_reward_plan_reference(
    session: &ManualSessionV1,
    plan: &ManualRewardPlanV1,
    worker: crate::draft_workspace::DraftWorker,
) -> Result<(String, String, String), String> {
    if plan.source.session_id != session.session_id() {
        return Err("reward plan source disagrees with its owner".into());
    }
    let directory = session.directory().join("reward-plans");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    write_atomic_new(
        &directory.join(format!("{id}.json")),
        &serde_json::to_vec(&SavedRewardPlanV1 {
            worker,
            plan: plan.clone(),
        })
        .map_err(|e| e.to_string())?,
    )?;
    let files = plan_files(session)?;
    Ok((format!("P{id}"), plan_handle(&id, &files), plan.summary()))
}

fn plan_files(session: &ManualSessionV1) -> Result<Vec<(PathBuf, String, String)>, String> {
    let mut files = Vec::new();
    for (owner, _) in adopted_scopes(session.repository(), session, None)? {
        let directory = owner.directory().join("reward-plans");
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        };
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if id.len() == 32
                && id.bytes().all(|b| b.is_ascii_hexdigit())
                && path.extension().and_then(|s| s.to_str()) == Some("json")
                && entry.file_type().map_err(|e| e.to_string())?.is_file()
            {
                files.push((path.clone(), id.into(), owner.session_id().into()));
            }
        }
    }
    files.sort();
    Ok(files)
}

fn plan_handle(id: &str, files: &[(PathBuf, String, String)]) -> String {
    let length = (8..=32)
        .find(|n| {
            files
                .iter()
                .filter(|(_, other, _)| other.starts_with(&id[..*n]))
                .count()
                == 1
        })
        .unwrap_or(32);
    format!("P{}", &id[..length])
}

pub fn list_manual_reward_plans_v1(
    session: &ManualSessionV1,
) -> Result<Vec<(String, String)>, String> {
    let files = plan_files(session)?;
    let scopes = adopted_scopes(session.repository(), session, None)?;
    let mut proposals = Vec::new();
    for (path, id, owner) in &files {
        let plan = read_plan(path, owner)?;
        if plan.source.selected_scope_index(&scopes).is_ok()
            && plan
                .actions
                .starts_with(&plan.source.committed_suffix(session)?)
        {
            proposals.push((plan_handle(id, &files), plan.summary()));
        }
    }
    Ok(proposals)
}

fn read_plan(path: &PathBuf, owner: &str) -> Result<ManualRewardPlanV1, String> {
    let saved: SavedRewardPlanV1 =
        serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let plan = saved.plan;
    if plan.source.session_id != owner {
        return Err("reward plan source disagrees with its owner".into());
    }
    Ok(plan)
}

pub struct PreparedRewardPlanV1 {
    view: sts_parity_lockstep::ActionViewV1,
    root: ParitySnapshotV1,
    actions: Vec<SemanticActionV1>,
    successors: Vec<ParitySnapshotV1>,
    completed: usize,
}

impl PreparedRewardPlanV1 {
    pub fn completed_actions(&self) -> usize {
        self.completed
    }
    pub fn remaining_actions(&self) -> usize {
        self.actions.len()
    }
    pub fn commit<J: LockstepAdapterV1>(
        &self,
        runner: &mut LockstepRunnerV1<J, RustLockstepAdapterV1>,
        mut on_exact: impl FnMut(
            usize,
            &LockstepRunnerV1<J, RustLockstepAdapterV1>,
        ) -> Result<(), String>,
    ) -> Result<usize, String> {
        if self.view != runner.current_action_view()? {
            return Err(
                "prepared reward plan belongs to another boundary; prepare adoption again".into(),
            );
        }
        require_same(
            "prepared reward plan root",
            &self.root,
            runner.current_snapshots().1,
        )?;
        runner.commit_exact_actions(&self.actions, |index, runner| {
            // Save the executed prefix even when it differs from the private proposal.
            on_exact(index, runner)?;
            require_same(
                "reward plan successor",
                &self.successors[index - 1],
                runner.current_snapshots().1,
            )
        })
    }
}

pub fn prepare_manual_reward_adoption_v1<J: LockstepAdapterV1>(
    session: &ManualSessionV1,
    handle: &str,
    runner: &LockstepRunnerV1<J, RustLockstepAdapterV1>,
    observer: &mut dyn FnMut(LockstepProgressV1),
) -> Result<PreparedRewardPlanV1, String> {
    let current = runner.rust_adapter();
    let prefix = handle
        .strip_prefix('P')
        .filter(|p| (8..=32).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("use a saved reward plan P…")?
        .to_ascii_lowercase();
    let matches = plan_files(session)?
        .into_iter()
        .filter(|(_, id, _)| id.starts_with(&prefix))
        .collect::<Vec<_>>();
    let (path, _, owner) = match matches.as_slice() {
        [one] => one,
        [] => return Err("unknown reward plan; use drafts".into()),
        _ => return Err("ambiguous reward plan; use the longer reference in drafts".into()),
    };
    let plan = read_plan(path, owner)?;
    let committed = plan.source.committed_suffix(session)?;
    if !plan.actions.starts_with(&committed) {
        return Err("committed actions differ from the reward plan prefix".into());
    }
    let ((_, mut replayed, _), anchor) = replay_manual_scope_observed(
        session.repository(),
        session,
        None,
        Some((&plan.source.session_id, plan.source.committed_actions)),
        observer,
    )?;
    let (origin, _) = *anchor.ok_or("reward plan origin was not observed")?;
    require_same(
        "reward adoption current boundary",
        &replayed.observe().map_err(|e| e.message)?,
        &current.clone().observe().map_err(|e| e.message)?,
    )?;
    prepare_reward_suffix(&plan, &origin, &committed, runner)
}

pub(crate) fn prepare_reward_suffix<J: LockstepAdapterV1>(
    plan: &ManualRewardPlanV1,
    origin: &RustLockstepAdapterV1,
    committed: &[SemanticActionV1],
    runner: &LockstepRunnerV1<J, RustLockstepAdapterV1>,
) -> Result<PreparedRewardPlanV1, String> {
    let current = runner.rust_adapter();
    if !plan.actions.starts_with(committed) {
        return Err("committed actions differ from the reward plan prefix".into());
    }
    let verified = plan.verify(origin)?;
    let root = if committed.is_empty() {
        verified.root
    } else {
        verified.successors[committed.len() - 1].clone()
    };
    require_same(
        "committed reward plan prefix",
        &root,
        &current.clone().observe().map_err(|e| e.message)?,
    )?;
    Ok(PreparedRewardPlanV1 {
        view: runner.current_action_view()?,
        root,
        actions: verified.actions[committed.len()..].to_vec(),
        successors: verified.successors[committed.len()..].to_vec(),
        completed: committed.len(),
    })
}
