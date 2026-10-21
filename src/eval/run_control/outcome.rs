use crate::engine::run_loop::FinishedActiveCombat;
pub use crate::eval::combat_baseline_outcome::{
    load_combat_baseline_outcome_v1, save_combat_baseline_outcome_v1,
    validate_combat_baseline_outcome_v1, CombatBaselineOutcomeV1,
    COMBAT_BASELINE_OUTCOME_SCHEMA_NAME, COMBAT_BASELINE_OUTCOME_SCHEMA_VERSION,
};
use crate::runtime::combat::CombatState;
use crate::sim::combat::{combat_terminal, CombatTerminal};
use crate::state::core::ClientInput;

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CombatOutcomeTracker {
    /// Evaluation-local baseline. Combat-learning suffixes deliberately move
    /// this to their own exact root so their outcome remains root-relative.
    active: Option<CombatOutcomeDraft>,
    /// Whole-combat baseline. Unlike `active`, this survives suffix rebases so
    /// run-level history and subsequent strategic decisions retain attrition
    /// and potion use from the actual combat entry.
    #[serde(default)]
    lineage_active: Option<CombatOutcomeDraft>,
    last: Option<CombatBaselineOutcomeV1>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
struct CombatOutcomeDraft {
    start_hp: i32,
    potions_used: u32,
    potions_discarded: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PendingPotionObservation {
    kind: PotionObservationKind,
    uuid: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PotionObservationKind {
    Use,
    Discard,
}

impl CombatOutcomeTracker {
    pub fn retain_active_drafts_only(&mut self) {
        self.last = None;
    }

    pub fn rebase_active(&mut self, combat: &CombatState) {
        let draft = CombatOutcomeDraft {
            start_hp: combat.entities.player.current_hp,
            potions_used: 0,
            potions_discarded: 0,
        };
        self.lineage_active.get_or_insert_with(|| draft.clone());
        self.active = Some(draft);
    }

    pub fn ensure_started(&mut self, combat: Option<&CombatState>) -> bool {
        if self.active.is_some() {
            return false;
        }
        let Some(combat) = combat else {
            return false;
        };
        let draft = CombatOutcomeDraft {
            start_hp: combat.entities.player.current_hp,
            potions_used: 0,
            potions_discarded: 0,
        };
        self.active = Some(draft.clone());
        self.lineage_active = Some(draft);
        true
    }

    pub fn observe_input_before(
        &self,
        combat: Option<&CombatState>,
        input: &ClientInput,
    ) -> Option<PendingPotionObservation> {
        let combat = combat?;
        match input {
            ClientInput::UsePotion { potion_index, .. } => combat
                .entities
                .potions
                .get(*potion_index)
                .and_then(|slot| slot.as_ref())
                .map(|potion| PendingPotionObservation {
                    kind: PotionObservationKind::Use,
                    uuid: potion.uuid,
                }),
            ClientInput::DiscardPotion(slot) => combat
                .entities
                .potions
                .get(*slot)
                .and_then(|slot| slot.as_ref())
                .map(|potion| PendingPotionObservation {
                    kind: PotionObservationKind::Discard,
                    uuid: potion.uuid,
                }),
            _ => None,
        }
    }

    pub fn observe_input_after(
        &mut self,
        observation: Option<PendingPotionObservation>,
        combat: Option<&CombatState>,
    ) {
        let Some(observation) = observation else {
            return;
        };
        let Some(combat) = combat else {
            return;
        };
        if potion_uuid_exists(combat, observation.uuid) {
            return;
        }
        let Some(active) = self.active.as_mut() else {
            return;
        };
        match observation.kind {
            PotionObservationKind::Use => {
                active.potions_used = active.potions_used.saturating_add(1);
                if let Some(lineage) = self.lineage_active.as_mut() {
                    lineage.potions_used = lineage.potions_used.saturating_add(1);
                }
            }
            PotionObservationKind::Discard => {
                active.potions_discarded = active.potions_discarded.saturating_add(1);
                if let Some(lineage) = self.lineage_active.as_mut() {
                    lineage.potions_discarded = lineage.potions_discarded.saturating_add(1);
                }
            }
        }
    }

    pub fn finish(
        &mut self,
        case_id: impl Into<String>,
        finished: &FinishedActiveCombat,
    ) -> CombatBaselineOutcomeV1 {
        let case_id = case_id.into();
        let draft = self.active.take().unwrap_or(CombatOutcomeDraft {
            start_hp: finished.combat_state.entities.player.current_hp,
            potions_used: 0,
            potions_discarded: 0,
        });
        let lineage_draft = self.lineage_active.take().unwrap_or_else(|| draft.clone());
        let outcome = combat_outcome_from_draft(&case_id, &draft, finished);
        let lineage_outcome = combat_outcome_from_draft(&case_id, &lineage_draft, finished);
        self.last = Some(outcome.clone());
        lineage_outcome
    }

    pub fn last(&self) -> Option<&CombatBaselineOutcomeV1> {
        self.last.as_ref()
    }
}

fn combat_outcome_from_draft(
    case_id: &str,
    draft: &CombatOutcomeDraft,
    finished: &FinishedActiveCombat,
) -> CombatBaselineOutcomeV1 {
    let final_hp = finished.combat_state.entities.player.current_hp;
    let terminal = if finished.combat_state.runtime.combat_smoked {
        CombatTerminal::Unresolved
    } else {
        combat_terminal(&finished.engine_state, &finished.combat_state)
    };

    CombatBaselineOutcomeV1 {
        schema_name: COMBAT_BASELINE_OUTCOME_SCHEMA_NAME.to_string(),
        schema_version: COMBAT_BASELINE_OUTCOME_SCHEMA_VERSION,
        case_id: case_id.to_string(),
        terminal,
        start_hp: draft.start_hp,
        final_hp,
        hp_loss: (draft.start_hp - final_hp).max(0),
        turns: finished.combat_state.turn.turn_count,
        potions_used: draft.potions_used,
        potions_discarded: draft.potions_discarded,
        cards_played: finished
            .combat_state
            .turn
            .counters
            .card_ids_played_this_combat
            .len() as u32,
    }
}

fn potion_uuid_exists(combat: &CombatState, uuid: u32) -> bool {
    combat
        .entities
        .potions
        .iter()
        .any(|slot| slot.as_ref().is_some_and(|potion| potion.uuid == uuid))
}

pub fn combat_enemy_hp(combat: &CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .fold(0i32, |total, monster| {
            total.saturating_add(monster.current_hp)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::combat_start_spec::{compile_combat_start_spec, CombatStartSpec};

    #[test]
    fn baseline_validation_rejects_bad_hp_loss() {
        let mut baseline = sample_baseline();
        baseline.hp_loss += 1;

        let err = validate_combat_baseline_outcome_v1(&baseline)
            .expect_err("bad hp_loss should be rejected");

        assert!(err.contains("hp_loss"));
    }

    #[test]
    fn tracker_builds_outcome_from_finished_combat() {
        let mut finished = finished_jaw_worm();

        let mut tracker = CombatOutcomeTracker::default();
        tracker.ensure_started(Some(&finished.combat_state));
        finished.combat_state.entities.player.current_hp = 65;
        for monster in &mut finished.combat_state.entities.monsters {
            monster.current_hp = 0;
        }
        let outcome = tracker.finish("jaw", &finished);

        assert_eq!(outcome.start_hp, 72);
        assert_eq!(outcome.final_hp, 65);
        assert_eq!(outcome.hp_loss, 7);
        assert_eq!(outcome.cards_played, 1);
    }

    #[test]
    fn tracker_does_not_classify_smoke_escape_as_combat_win() {
        let mut finished = finished_jaw_worm();
        let mut tracker = CombatOutcomeTracker::default();
        tracker.ensure_started(Some(&finished.combat_state));
        finished.combat_state.runtime.combat_smoked = true;

        let outcome = tracker.finish("escaped-jaw", &finished);

        assert_eq!(outcome.terminal, CombatTerminal::Unresolved);
    }

    #[test]
    fn enemy_hp_evidence_excludes_escaped_monsters() {
        let mut combat = finished_jaw_worm().combat_state;
        combat.entities.monsters[0].current_hp = 40;

        assert_eq!(combat_enemy_hp(&combat), 40);
        combat.entities.monsters[0].is_escaped = true;
        assert_eq!(combat_enemy_hp(&combat), 0);
    }

    fn finished_jaw_worm() -> FinishedActiveCombat {
        let spec: CombatStartSpec = serde_json::from_str(
            r#"{
                "name": "jaw_worm_starter",
                "player_class": "Ironclad",
                "ascension_level": 0,
                "encounter_id": "JawWorm",
                "room_type": "monster",
                "seed": 1,
                "player_current_hp": 80,
                "player_max_hp": 80,
                "master_deck": [
                    {"id": "Strike_R", "count": 5},
                    {"id": "Defend_R", "count": 4},
                    "Bash"
                ]
            }"#,
        )
        .expect("spec parses");
        let (_engine_state, mut combat_state) =
            compile_combat_start_spec(&spec).expect("spec compiles");
        combat_state.entities.player.current_hp = 72;
        combat_state.turn.counters.card_ids_played_this_combat =
            vec![crate::content::cards::CardId::Strike];
        FinishedActiveCombat {
            engine_state: crate::state::core::EngineState::GameOver(
                crate::state::core::RunResult::Victory,
            ),
            combat_state,
        }
    }

    fn sample_baseline() -> CombatBaselineOutcomeV1 {
        CombatBaselineOutcomeV1 {
            schema_name: COMBAT_BASELINE_OUTCOME_SCHEMA_NAME.to_string(),
            schema_version: COMBAT_BASELINE_OUTCOME_SCHEMA_VERSION,
            case_id: "case".to_string(),
            terminal: CombatTerminal::Win,
            start_hp: 80,
            final_hp: 70,
            hp_loss: 10,
            turns: 4,
            potions_used: 0,
            potions_discarded: 0,
            cards_played: 8,
        }
    }
}
