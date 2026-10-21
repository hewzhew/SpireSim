use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const PUBLIC_ACTOR_CENSUS_SCHEMA_V9: &str = "sts-public-actor-census-v9";
pub const PUBLIC_ACTOR_CENSUS_INSPECTION_SCHEMA_V1: &str = "sts-public-actor-census-inspection-v1";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PublicActorCensusSummaryV1 {
    #[serde(default)]
    pub episodes: usize,
    #[serde(default)]
    pub successes: usize,
    #[serde(default)]
    pub paused: usize,
    #[serde(default)]
    pub committed_defeats: usize,
    #[serde(default)]
    pub unsupported_decision_surface: usize,
    #[serde(default)]
    pub mechanics_or_protocol_errors: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PublicActorCensusReportV9 {
    pub schema: String,
    pub summary: PublicActorCensusSummaryV1,
    #[serde(default)]
    pub episodes: Vec<PublicActorEpisodeProjectionV9>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PublicActorEpisodeProjectionV9 {
    pub seed: u64,
    #[serde(default)]
    pub actor: PublicActorIdentityProjectionV1,
    #[serde(default)]
    pub outcome: PublicActorOutcomeProjectionV1,
    #[serde(default)]
    pub combat_research_plan_attempts: Vec<CombatResearchPlanAttemptProjectionV1>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PublicActorIdentityProjectionV1 {
    #[serde(default)]
    pub actor_name: String,
    #[serde(default)]
    pub actor_revision: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PublicActorOutcomeProjectionV1 {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub reason: Option<PublicActorOutcomeReasonProjectionV1>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PublicActorOutcomeReasonProjectionV1 {
    #[serde(default)]
    pub kind: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct CombatResearchPlanAttemptProjectionV1 {
    #[serde(default)]
    pub act: u32,
    #[serde(default)]
    pub floor: u32,
    #[serde(default)]
    pub encounter_id: String,
    #[serde(default)]
    pub hp: i32,
    #[serde(default)]
    pub max_hp: i32,
    #[serde(default)]
    pub turn: u32,
    #[serde(default)]
    pub lanes: Vec<CombatResearchLaneProjectionV1>,
    #[serde(default)]
    pub exit: String,
    #[serde(default)]
    pub candidates: Vec<CombatResearchCandidateProjectionV1>,
    #[serde(default)]
    pub non_dominated_candidate_ordinals: Vec<usize>,
    #[serde(default)]
    pub selection: String,
    #[serde(default)]
    pub selected_candidate_ordinal: Option<usize>,
    #[serde(default)]
    pub selected_supporting_provenances: Vec<CombatResearchProvenanceProjectionV1>,
    #[serde(default)]
    pub plan_steps: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct CombatResearchLaneProjectionV1 {
    #[serde(default)]
    pub provenance: CombatResearchProvenanceProjectionV1,
    #[serde(default)]
    pub work_unit: String,
    #[serde(default)]
    pub work_consumed: usize,
    #[serde(default)]
    pub exit: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CombatResearchCandidateProjectionV1 {
    #[serde(default)]
    pub supporting_provenances: Vec<CombatResearchProvenanceProjectionV1>,
    #[serde(default)]
    pub plan_steps: usize,
    #[serde(default)]
    pub satisfies_requested_quality: bool,
    pub outcome: CombatResearchOutcomeProjectionV1,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CombatResearchProvenanceProjectionV1 {
    #[serde(default)]
    pub producer_name: String,
    #[serde(default)]
    pub producer_revision: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResearchOutcomeProjectionV1 {
    pub final_hp: i32,
    pub final_gold: i32,
    pub persistent: CombatResearchPersistentOutcomeProjectionV1,
    pub consumed_entry_potion_slots: Vec<usize>,
    pub total_potions_consumed: u32,
    pub generated_potions: Vec<CombatResearchGeneratedPotionProjectionV1>,
    pub final_relics: Vec<CombatResearchRelicStateProjectionV1>,
    pub meta_changes: Vec<CombatResearchMetaChangeProjectionV1>,
    pub pending_rewards: Vec<CombatResearchPendingRewardProjectionV1>,
    pub combat_mugged: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResearchPersistentOutcomeProjectionV1 {
    pub max_hp: i32,
    pub recoverable_gold_delta: i32,
    pub unrecovered_stolen_gold: i32,
    pub ritual_dagger_value: i32,
    pub genetic_algorithm_value: i32,
    pub external_burden_count: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResearchGeneratedPotionProjectionV1 {
    pub slot: usize,
    pub potion_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResearchRelicStateProjectionV1 {
    pub id: String,
    #[serde(default = "default_relic_counter")]
    pub counter: i32,
    #[serde(default)]
    pub used_up: bool,
    #[serde(default)]
    pub amount: i32,
}

fn default_relic_counter() -> i32 {
    -1
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CombatResearchMetaChangeProjectionV1 {
    AddCardToMasterDeck {
        card_id: String,
    },
    ModifyCardMisc {
        master_deck_ordinal: usize,
        amount: i32,
    },
    UpgradeMasterDeckCard {
        master_deck_ordinal: usize,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResearchRewardCardProjectionV1 {
    pub card_id: String,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CombatResearchPendingRewardProjectionV1 {
    Gold {
        amount: i32,
    },
    StolenGold {
        amount: i32,
    },
    Card {
        cards: Vec<CombatResearchRewardCardProjectionV1>,
    },
    Relic {
        relic_id: String,
    },
    Potion {
        potion_id: String,
    },
    EmeraldKey,
    SapphireKey,
}

#[derive(Clone, Debug, Default)]
pub struct PublicActorCensusInspectFilterV1 {
    pub episode: Option<usize>,
    pub attempt: Option<usize>,
    pub encounter: Option<String>,
    pub floor: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PublicActorCensusInspectionV1 {
    pub schema_name: &'static str,
    pub source_schema: String,
    pub summary: PublicActorCensusSummaryV1,
    pub episodes: Vec<PublicActorEpisodeInspectionV1>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PublicActorEpisodeInspectionV1 {
    pub episode_ordinal: usize,
    pub seed: u64,
    pub actor_name: String,
    pub actor_revision: u32,
    pub outcome_status: String,
    pub outcome_reason_kind: Option<String>,
    pub combat_attempts: Vec<CombatResearchPlanAttemptInspectionV1>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatResearchPlanAttemptInspectionV1 {
    pub attempt_ordinal: usize,
    pub act: u32,
    pub floor: u32,
    pub encounter_id: String,
    pub entry_hp: i32,
    pub entry_max_hp: i32,
    pub turn: u32,
    pub exit: String,
    pub selection: String,
    pub selected_plan_steps: Option<usize>,
    /// Present only when the stored provenance and step count identify one
    /// candidate uniquely. V8 does not store a selected candidate ordinal.
    pub selected_candidate_ordinal: Option<usize>,
    pub selected_candidate_projection_ambiguous: bool,
    pub lanes: Vec<CombatResearchLaneInspectionV1>,
    pub candidates: Vec<CombatResearchCandidateInspectionV1>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatResearchLaneInspectionV1 {
    pub producer_name: String,
    pub producer_revision: u32,
    pub work_unit: String,
    pub work_consumed: usize,
    pub exit: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatResearchCandidateInspectionV1 {
    pub candidate_ordinal: usize,
    pub supporting_provenances: Vec<CombatResearchProvenanceProjectionV1>,
    pub plan_steps: usize,
    pub satisfies_requested_quality: bool,
    pub non_dominated: bool,
    pub selected_by_unique_report_projection: bool,
    pub final_hp: i32,
    pub final_gold: i32,
    pub persistent: CombatResearchPersistentOutcomeProjectionV1,
    pub consumed_entry_potion_slots: Vec<usize>,
    pub total_potions_consumed: u32,
    pub generated_potions: Vec<CombatResearchGeneratedPotionProjectionV1>,
    pub final_relics: Vec<CombatResearchRelicStateProjectionV1>,
    pub meta_changes: Vec<CombatResearchMetaChangeProjectionV1>,
    pub pending_rewards: Vec<CombatResearchPendingRewardProjectionV1>,
    pub combat_mugged: bool,
}

pub fn load_public_actor_census_v9(path: &Path) -> Result<PublicActorCensusReportV9, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read census report {}: {error}", path.display()))?;
    parse_public_actor_census_v9(&bytes)
}

pub fn parse_public_actor_census_v9(bytes: &[u8]) -> Result<PublicActorCensusReportV9, String> {
    let report = serde_json::from_slice::<PublicActorCensusReportV9>(bytes)
        .map_err(|error| format!("invalid public actor census JSON: {error}"))?;
    validate_public_actor_census_v9(&report)?;
    Ok(report)
}

pub fn validate_public_actor_census_v9(report: &PublicActorCensusReportV9) -> Result<(), String> {
    if report.schema != PUBLIC_ACTOR_CENSUS_SCHEMA_V9 {
        return Err(format!(
            "unsupported census schema {:?}; expected {:?}",
            report.schema, PUBLIC_ACTOR_CENSUS_SCHEMA_V9
        ));
    }
    if report.summary.episodes != report.episodes.len() {
        return Err(format!(
            "census summary declares {} episodes but stores {}",
            report.summary.episodes,
            report.episodes.len()
        ));
    }
    for (episode_ordinal, episode) in report.episodes.iter().enumerate() {
        for (attempt_ordinal, attempt) in episode.combat_research_plan_attempts.iter().enumerate() {
            let mut seen = HashSet::new();
            for ordinal in &attempt.non_dominated_candidate_ordinals {
                if *ordinal >= attempt.candidates.len() {
                    return Err(format!(
                        "episode {episode_ordinal} attempt {attempt_ordinal} names non-dominated candidate {ordinal}, but only {} candidates exist",
                        attempt.candidates.len()
                    ));
                }
                if !seen.insert(*ordinal) {
                    return Err(format!(
                        "episode {episode_ordinal} attempt {attempt_ordinal} repeats non-dominated candidate {ordinal}"
                    ));
                }
            }
            for (candidate_ordinal, candidate) in attempt.candidates.iter().enumerate() {
                if candidate.supporting_provenances.is_empty()
                    || candidate
                        .supporting_provenances
                        .windows(2)
                        .any(|pair| pair[0] >= pair[1])
                {
                    return Err(format!(
                        "episode {episode_ordinal} attempt {attempt_ordinal} candidate {candidate_ordinal} has empty, unsorted, or duplicate supporting provenances"
                    ));
                }
                if candidate
                    .outcome
                    .consumed_entry_potion_slots
                    .windows(2)
                    .any(|slots| slots[0] >= slots[1])
                {
                    return Err(format!(
                        "episode {episode_ordinal} attempt {attempt_ordinal} candidate {candidate_ordinal} has unsorted or duplicate consumed potion slots"
                    ));
                }
                if candidate
                    .outcome
                    .generated_potions
                    .windows(2)
                    .any(|potions| potions[0].slot >= potions[1].slot)
                {
                    return Err(format!(
                        "episode {episode_ordinal} attempt {attempt_ordinal} candidate {candidate_ordinal} has unsorted or duplicate generated potion slots"
                    ));
                }
            }
            if let Some(selected_ordinal) = attempt.selected_candidate_ordinal {
                let selected = attempt.candidates.get(selected_ordinal).ok_or_else(|| {
                    format!(
                        "episode {episode_ordinal} attempt {attempt_ordinal} selects candidate {selected_ordinal}, but only {} candidates exist",
                        attempt.candidates.len()
                    )
                })?;
                if selected.supporting_provenances != attempt.selected_supporting_provenances
                    || selected.plan_steps != attempt.plan_steps
                {
                    return Err(format!(
                        "episode {episode_ordinal} attempt {attempt_ordinal} selected candidate identity disagrees with supporting provenances or plan length"
                    ));
                }
                if !seen.contains(&selected_ordinal) {
                    return Err(format!(
                        "episode {episode_ordinal} attempt {attempt_ordinal} selects candidate {selected_ordinal} outside the non-dominated frontier"
                    ));
                }
            } else if !attempt.selected_supporting_provenances.is_empty() || attempt.plan_steps != 0
            {
                return Err(format!(
                    "episode {episode_ordinal} attempt {attempt_ordinal} stores selected evidence without a selected candidate ordinal"
                ));
            }
        }
    }
    Ok(())
}

pub fn inspect_public_actor_census_v9(
    report: &PublicActorCensusReportV9,
    filter: &PublicActorCensusInspectFilterV1,
) -> Result<PublicActorCensusInspectionV1, String> {
    if let Some(episode) = filter.episode {
        if episode >= report.episodes.len() {
            return Err(format!(
                "episode {episode} is out of range 0..{}",
                report.episodes.len()
            ));
        }
    }
    let mut episodes = Vec::new();
    for (episode_ordinal, episode) in report.episodes.iter().enumerate() {
        if filter
            .episode
            .is_some_and(|wanted| wanted != episode_ordinal)
        {
            continue;
        }
        let combat_attempts = inspect_attempts(episode, filter);
        if !combat_attempts.is_empty()
            || (filter.encounter.is_none() && filter.floor.is_none() && filter.attempt.is_none())
        {
            episodes.push(PublicActorEpisodeInspectionV1 {
                episode_ordinal,
                seed: episode.seed,
                actor_name: episode.actor.actor_name.clone(),
                actor_revision: episode.actor.actor_revision,
                outcome_status: episode.outcome.status.clone(),
                outcome_reason_kind: episode
                    .outcome
                    .reason
                    .as_ref()
                    .map(|reason| reason.kind.clone()),
                combat_attempts,
            });
        }
    }
    Ok(PublicActorCensusInspectionV1 {
        schema_name: PUBLIC_ACTOR_CENSUS_INSPECTION_SCHEMA_V1,
        source_schema: report.schema.clone(),
        summary: report.summary.clone(),
        episodes,
    })
}

fn inspect_attempts(
    episode: &PublicActorEpisodeProjectionV9,
    filter: &PublicActorCensusInspectFilterV1,
) -> Vec<CombatResearchPlanAttemptInspectionV1> {
    episode
        .combat_research_plan_attempts
        .iter()
        .enumerate()
        .filter(|(attempt_ordinal, attempt)| {
            filter
                .attempt
                .is_none_or(|wanted| wanted == *attempt_ordinal)
                && filter.floor.is_none_or(|wanted| wanted == attempt.floor)
                && filter
                    .encounter
                    .as_deref()
                    .is_none_or(|wanted| wanted.eq_ignore_ascii_case(&attempt.encounter_id))
        })
        .map(|(attempt_ordinal, attempt)| {
            let non_dominated = attempt
                .non_dominated_candidate_ordinals
                .iter()
                .copied()
                .collect::<HashSet<_>>();
            let selected_plan_steps = attempt
                .selected_candidate_ordinal
                .map(|_| attempt.plan_steps);
            let selected_candidate_ordinal = attempt.selected_candidate_ordinal;
            CombatResearchPlanAttemptInspectionV1 {
                attempt_ordinal,
                act: attempt.act,
                floor: attempt.floor,
                encounter_id: attempt.encounter_id.clone(),
                entry_hp: attempt.hp,
                entry_max_hp: attempt.max_hp,
                turn: attempt.turn,
                exit: attempt.exit.clone(),
                selection: attempt.selection.clone(),
                selected_plan_steps,
                selected_candidate_ordinal,
                selected_candidate_projection_ambiguous: false,
                lanes: attempt
                    .lanes
                    .iter()
                    .map(|lane| CombatResearchLaneInspectionV1 {
                        producer_name: lane.provenance.producer_name.clone(),
                        producer_revision: lane.provenance.producer_revision,
                        work_unit: lane.work_unit.clone(),
                        work_consumed: lane.work_consumed,
                        exit: lane.exit.clone(),
                    })
                    .collect(),
                candidates: attempt
                    .candidates
                    .iter()
                    .enumerate()
                    .map(
                        |(candidate_ordinal, candidate)| CombatResearchCandidateInspectionV1 {
                            candidate_ordinal,
                            supporting_provenances: candidate.supporting_provenances.clone(),
                            plan_steps: candidate.plan_steps,
                            satisfies_requested_quality: candidate.satisfies_requested_quality,
                            non_dominated: non_dominated.contains(&candidate_ordinal),
                            selected_by_unique_report_projection: selected_candidate_ordinal
                                == Some(candidate_ordinal),
                            final_hp: candidate.outcome.final_hp,
                            final_gold: candidate.outcome.final_gold,
                            persistent: candidate.outcome.persistent,
                            consumed_entry_potion_slots: candidate
                                .outcome
                                .consumed_entry_potion_slots
                                .clone(),
                            total_potions_consumed: candidate.outcome.total_potions_consumed,
                            generated_potions: candidate.outcome.generated_potions.clone(),
                            final_relics: candidate.outcome.final_relics.clone(),
                            meta_changes: candidate.outcome.meta_changes.clone(),
                            pending_rewards: candidate.outcome.pending_rewards.clone(),
                            combat_mugged: candidate.outcome.combat_mugged,
                        },
                    )
                    .collect(),
            }
        })
        .collect()
}

pub fn render_public_actor_census_inspection_v1(
    inspection: &PublicActorCensusInspectionV1,
) -> String {
    let summary = &inspection.summary;
    let mut output = format!(
        "Public actor census V8 | episodes {} | successes {} | unsupported {} | protocol errors {}\n",
        summary.episodes,
        summary.successes,
        summary.unsupported_decision_surface,
        summary.mechanics_or_protocol_errors
    );
    let mut attempt_count = 0usize;
    for episode in &inspection.episodes {
        let reason = episode
            .outcome_reason_kind
            .as_deref()
            .map(|reason| format!("/{reason}"))
            .unwrap_or_default();
        let _ = writeln!(
            output,
            "Episode {} | seed {} | {}@{} | {}{}",
            episode.episode_ordinal,
            episode.seed,
            episode.actor_name,
            episode.actor_revision,
            episode.outcome_status,
            reason
        );
        for attempt in &episode.combat_attempts {
            attempt_count += 1;
            let selected = match (
                attempt.selected_candidate_ordinal,
                attempt.selected_plan_steps,
                attempt.selected_candidate_projection_ambiguous,
            ) {
                (Some(ordinal), Some(steps), _) => {
                    format!(" | selected C{ordinal} ({steps} steps)")
                }
                (None, Some(steps), true) => {
                    format!(" | selected projection ambiguous ({steps} steps)")
                }
                _ => String::new(),
            };
            let _ = writeln!(
                output,
                "  Attempt {} | A{} F{} {} T{} | HP {}/{} | {} | {}{}",
                attempt.attempt_ordinal,
                attempt.act,
                attempt.floor,
                attempt.encounter_id,
                attempt.turn,
                attempt.entry_hp,
                attempt.entry_max_hp,
                attempt.exit,
                attempt.selection,
                selected
            );
            for lane in &attempt.lanes {
                let _ = writeln!(
                    output,
                    "    lane {}@{} | {} {} | {}",
                    lane.producer_name,
                    lane.producer_revision,
                    lane.work_consumed,
                    lane.work_unit,
                    lane.exit
                );
            }
            let persistent_varies =
                candidate_field_varies(&attempt.candidates, |candidate| &candidate.persistent);
            let generated_potions_vary = candidate_field_varies(&attempt.candidates, |candidate| {
                &candidate.generated_potions
            });
            let varying_relic_ordinals = varying_relic_ordinals(&attempt.candidates);
            let meta_changes_vary =
                candidate_field_varies(&attempt.candidates, |candidate| &candidate.meta_changes);
            let pending_rewards_vary =
                candidate_field_varies(&attempt.candidates, |candidate| &candidate.pending_rewards);
            let combat_mugged_varies =
                candidate_field_varies(&attempt.candidates, |candidate| &candidate.combat_mugged);
            for candidate in &attempt.candidates {
                let frontier = if candidate.non_dominated { "ND" } else { "--" };
                let selected = if candidate.selected_by_unique_report_projection {
                    " selected"
                } else {
                    ""
                };
                let quality = if candidate.satisfies_requested_quality {
                    " quality"
                } else {
                    ""
                };
                let _ = writeln!(
                    output,
                    "    C{} [{}] | {} steps | HP {} | gold {} | potions {} {:?} | supports {}{}{}",
                    candidate.candidate_ordinal,
                    frontier,
                    candidate.plan_steps,
                    candidate.final_hp,
                    candidate.final_gold,
                    candidate.total_potions_consumed,
                    candidate.consumed_entry_potion_slots,
                    candidate
                        .supporting_provenances
                        .iter()
                        .map(|provenance| format!(
                            "{}@{}",
                            provenance.producer_name, provenance.producer_revision
                        ))
                        .collect::<Vec<_>>()
                        .join(","),
                    selected,
                    quality
                );
                if persistent_varies {
                    let _ = writeln!(
                        output,
                        "      persistent {}",
                        compact_json(&candidate.persistent)
                    );
                }
                if generated_potions_vary {
                    let _ = writeln!(
                        output,
                        "      generated potions {}",
                        compact_json(&candidate.generated_potions)
                    );
                }
                if !varying_relic_ordinals.is_empty() {
                    let _ = writeln!(
                        output,
                        "      relic differences {}",
                        render_relic_states(&candidate.final_relics, &varying_relic_ordinals)
                    );
                }
                if meta_changes_vary {
                    let _ = writeln!(
                        output,
                        "      meta changes {}",
                        compact_json(&candidate.meta_changes)
                    );
                }
                if pending_rewards_vary {
                    let _ = writeln!(
                        output,
                        "      pending rewards {}",
                        compact_json(&candidate.pending_rewards)
                    );
                }
                if combat_mugged_varies {
                    let _ = writeln!(output, "      combat mugged {}", candidate.combat_mugged);
                }
            }
        }
    }
    if attempt_count == 0 {
        output.push_str("No combat research attempts matched the requested filter.\n");
    }
    output
}

fn compact_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<serialization failed>".to_string())
}

fn candidate_field_varies<T: PartialEq + ?Sized>(
    candidates: &[CombatResearchCandidateInspectionV1],
    field: impl Fn(&CombatResearchCandidateInspectionV1) -> &T,
) -> bool {
    let Some(first) = candidates.first() else {
        return false;
    };
    candidates
        .iter()
        .skip(1)
        .any(|candidate| field(candidate) != field(first))
}

fn varying_relic_ordinals(candidates: &[CombatResearchCandidateInspectionV1]) -> Vec<usize> {
    let Some(first) = candidates.first() else {
        return Vec::new();
    };
    let maximum_relics = candidates
        .iter()
        .map(|candidate| candidate.final_relics.len())
        .max()
        .unwrap_or(0);
    (0..maximum_relics)
        .filter(|ordinal| {
            candidates.iter().skip(1).any(|candidate| {
                candidate.final_relics.get(*ordinal) != first.final_relics.get(*ordinal)
            })
        })
        .collect()
}

fn render_relic_states(
    relics: &[CombatResearchRelicStateProjectionV1],
    varying_ordinals: &[usize],
) -> String {
    let rendered = varying_ordinals
        .iter()
        .map(|ordinal| {
            let Some(relic) = relics.get(*ordinal) else {
                return format!("#{ordinal}=<missing>");
            };
            let mut state = Vec::new();
            if relic.counter != default_relic_counter() {
                state.push(format!("counter={}", relic.counter));
            }
            if relic.used_up {
                state.push("used_up".to_string());
            }
            if relic.amount != 0 {
                state.push(format!("amount={}", relic.amount));
            }
            if state.is_empty() {
                format!("#{ordinal}={}", relic.id)
            } else {
                format!("#{ordinal}={}({})", relic.id, state.join(","))
            }
        })
        .collect::<Vec<_>>();
    format!("[{}]", rendered.join(", "))
}
