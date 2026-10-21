use crate::{
    acquisition::{remember_master_deck_acquisitions, JavaMasterDeckAcquisitionV1},
    action::{
        java_grid_select_boundary_identity_v1, JavaActionExecutionV1, JavaHandSelectProfileV1,
        ResolvedJavaActionV1,
    },
    projection::{
        current_node, project_snapshot_v1, rebind_master_deck_after_native_reload_v1,
        JavaStableCardBindingV1,
    },
    recording::{RecordedJavaActionPhaseV1, RecordedJavaEvidenceStoreV1},
    transport::{
        JavaLockstepReceiptV1, JavaLockstepTransportV1, JavaNativeReloadExecutionV1,
        JavaRuntimeObservationV1,
    },
};
use std::collections::{BTreeMap, BTreeSet};
use sts_java_parity_protocol::{
    canonical_game_id, CardBirthCause, CardBirthProduct, CardBirthWitness, RngCallWitness,
};
use sts_parity_lockstep::{
    natural_run_card_binding_v1, AdapterFailureKindV1, AdapterFailureV1, DecisionSurfaceV1,
    DomainEvidenceV1, ExecutionInputsV1, LockstepAdapterV1, NaturalRunBootstrapV1,
    NaturalRunCardSourceV1, NaturalRunCombatCardBirthCauseV1, NaturalRunRngCallLedgerV1,
    NaturalRunRngCallV1, ParityDomainV1, ParitySnapshotV1, SemanticActionV1,
    NATURAL_RUN_RNG_CALL_LEDGER_SCHEMA_V1,
};

#[derive(Clone, Debug)]
pub struct PreparedJavaActionV1 {
    based_on_observation_id: String,
    based_on_frame_id: u64,
    semantic: SemanticActionV1,
    runtime_handle: String,
    execution: JavaActionExecutionV1,
}

#[derive(Clone, Debug)]
struct JavaDeckTransformContextV1 {
    selected_uuid: String,
    prior_uuids: BTreeSet<String>,
    rebound: JavaStableCardBindingV1,
}

pub struct JavaLockstepAdapterV1<T> {
    transport: T,
    bootstrap: NaturalRunBootstrapV1,
    expected_resume_observation: Option<JavaRuntimeObservationV1>,
    pending_native_reload: Option<(
        JavaLockstepResumeStateV1,
        Option<JavaNativeReloadExecutionV1>,
    )>,
    current_observation: Option<JavaRuntimeObservationV1>,
    current_snapshot: Option<ParitySnapshotV1>,
    current_actions: Vec<ResolvedJavaActionV1>,
    current_runtime_card_bindings: BTreeMap<String, String>,
    master_deck_rebindings: BTreeMap<String, JavaStableCardBindingV1>,
    next_acquisition_slots: BTreeMap<String, u32>,
    combat_birth_bindings: BTreeMap<String, JavaStableCardBindingV1>,
    active_combat_room: Option<String>,
    next_combat_action_ordinal: u32,
    hand_select_profile: Option<JavaHandSelectProfileV1>,
    evidence_store: Option<RecordedJavaEvidenceStoreV1>,
    last_rng_call_ledger: Option<NaturalRunRngCallLedgerV1>,
    execution_inputs: ExecutionInputsV1,
}

/// Opaque state required to reconnect a freshly built Rust worker to the same
/// parked Java boundary. It is reconstructed by replaying raw Java evidence,
/// never serialized as a second source of truth.
#[derive(Clone, Debug, PartialEq)]
pub struct JavaLockstepResumeStateV1 {
    expected_observation: JavaRuntimeObservationV1,
    master_deck_rebindings: BTreeMap<String, JavaStableCardBindingV1>,
    next_acquisition_slots: BTreeMap<String, u32>,
    combat_birth_bindings: BTreeMap<String, JavaStableCardBindingV1>,
    active_combat_room: Option<String>,
    next_combat_action_ordinal: u32,
    hand_select_profile: Option<JavaHandSelectProfileV1>,
}

impl JavaLockstepResumeStateV1 {
    pub fn matches_observation(&self, observation: &JavaRuntimeObservationV1) -> bool {
        &self.expected_observation == observation
    }
}

impl<T> JavaLockstepAdapterV1<T>
where
    T: JavaLockstepTransportV1,
{
    pub fn new(transport: T, bootstrap: NaturalRunBootstrapV1) -> Result<Self, String> {
        bootstrap.validate()?;
        Ok(Self {
            transport,
            bootstrap,
            expected_resume_observation: None,
            pending_native_reload: None,
            current_observation: None,
            current_snapshot: None,
            current_actions: Vec::new(),
            current_runtime_card_bindings: BTreeMap::new(),
            master_deck_rebindings: BTreeMap::new(),
            next_acquisition_slots: BTreeMap::new(),
            combat_birth_bindings: BTreeMap::new(),
            active_combat_room: None,
            next_combat_action_ordinal: 0,
            hand_select_profile: None,
            evidence_store: None,
            last_rng_call_ledger: None,
            execution_inputs: ExecutionInputsV1::default(),
        })
    }

    pub fn with_evidence_store(
        transport: T,
        bootstrap: NaturalRunBootstrapV1,
        evidence_store: RecordedJavaEvidenceStoreV1,
    ) -> Result<Self, String> {
        if evidence_store.bootstrap() != &bootstrap {
            return Err("Java evidence store bootstrap does not match the adapter".to_owned());
        }
        let mut adapter = Self::new(transport, bootstrap)?;
        adapter.evidence_store = Some(evidence_store);
        Ok(adapter)
    }

    pub fn resume_with_evidence_store(
        transport: T,
        bootstrap: NaturalRunBootstrapV1,
        evidence_store: RecordedJavaEvidenceStoreV1,
        resume: JavaLockstepResumeStateV1,
    ) -> Result<Self, String> {
        if evidence_store.bootstrap() != &bootstrap {
            return Err("Java evidence store bootstrap does not match the adapter".to_owned());
        }
        let mut adapter = Self::resume(transport, bootstrap, resume)?;
        adapter.evidence_store = Some(evidence_store);
        Ok(adapter)
    }

    /// Continues the exact same raw boundary, preserving all source-proven
    /// runtime leases. Live journal splits and offline replay share this path.
    pub fn resume(
        transport: T,
        bootstrap: NaturalRunBootstrapV1,
        resume: JavaLockstepResumeStateV1,
    ) -> Result<Self, String> {
        validate_resume_state_v1(&resume)?;
        let mut adapter = Self::new(transport, bootstrap)?;
        adapter.expected_resume_observation = Some(resume.expected_observation);
        adapter.master_deck_rebindings = resume.master_deck_rebindings;
        adapter.next_acquisition_slots = resume.next_acquisition_slots;
        adapter.combat_birth_bindings = resume.combat_birth_bindings;
        adapter.active_combat_room = resume.active_combat_room;
        adapter.next_combat_action_ordinal = resume.next_combat_action_ordinal;
        adapter.hand_select_profile = resume.hand_select_profile;
        Ok(adapter)
    }

    /// Attaches to a boundary reconstructed from the original game's native
    /// autosave. Unlike `resume_with_evidence_store`, this does not require raw
    /// frame identity to survive the reload. The first observation must prove
    /// the persisted ordered deck before fresh Java UUID leases are issued;
    /// the lockstep runner then compares the complete canonical snapshot.
    pub fn reattach_after_native_reload_with_evidence_store(
        transport: T,
        bootstrap: NaturalRunBootstrapV1,
        evidence_store: RecordedJavaEvidenceStoreV1,
        resume: JavaLockstepResumeStateV1,
        execution: Option<JavaNativeReloadExecutionV1>,
    ) -> Result<Self, String> {
        if evidence_store.bootstrap() != &bootstrap {
            return Err("Java evidence store bootstrap does not match the adapter".to_owned());
        }
        if let Some(execution) = &execution {
            evidence_store.record_native_reload(execution)?;
        }
        let mut adapter =
            Self::reattach_after_native_reload(transport, bootstrap, resume, execution)?;
        adapter.evidence_store = Some(evidence_store);
        Ok(adapter)
    }

    /// Rebuilds runtime bindings from typed native-reload provenance without
    /// writing evidence. Live capture and offline replay share this path.
    pub fn reattach_after_native_reload(
        transport: T,
        bootstrap: NaturalRunBootstrapV1,
        resume: JavaLockstepResumeStateV1,
        execution: Option<JavaNativeReloadExecutionV1>,
    ) -> Result<Self, String> {
        validate_resume_state_v1(&resume)?;
        let mut adapter = Self::new(transport, bootstrap)?;
        if let Some(execution) = &execution {
            execution.validate()?;
        }
        adapter.pending_native_reload = Some((resume, execution));
        Ok(adapter)
    }

    pub fn into_transport(self) -> T {
        self.transport
    }

    pub fn resume_state_v1(&self) -> Result<JavaLockstepResumeStateV1, String> {
        let expected_observation = self
            .current_observation
            .clone()
            .ok_or_else(|| "Java adapter must observe before exporting resume state".to_owned())?;
        Ok(JavaLockstepResumeStateV1 {
            expected_observation,
            master_deck_rebindings: self.master_deck_rebindings.clone(),
            next_acquisition_slots: self.next_acquisition_slots.clone(),
            combat_birth_bindings: self.combat_birth_bindings.clone(),
            active_combat_room: self.active_combat_room.clone(),
            next_combat_action_ordinal: self.next_combat_action_ordinal,
            hand_select_profile: self.hand_select_profile,
        })
    }

    /// Aggregated RNG calls from every raw Java action used to settle the most
    /// recently executed semantic action.
    pub fn last_rng_call_ledger_v1(&self) -> Option<&NaturalRunRngCallLedgerV1> {
        self.last_rng_call_ledger.as_ref()
    }

    /// The already recorded stable boundary. Reading it performs no I/O.
    pub fn current_raw_observation_v1(&self) -> Option<&JavaRuntimeObservationV1> {
        self.current_observation.as_ref()
    }

    /// Available only after a fully settled successful adapter execution.
    /// The lockstep runner still owns comparison and journal-commit authority.
    pub fn last_recorded_step_v1(&self) -> Option<crate::RecordedJavaStepRefV1<'_>> {
        self.last_rng_call_ledger.as_ref()?;
        let step = self.evidence_store.as_ref()?.last_complete_step_v1()?;
        (self.current_observation.as_ref()? == step.after()).then_some(step)
    }

    fn capture(&mut self) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        let observation = {
            let _timing = sts_parity_lockstep::timing_span_v1("java.status");
            self.transport.status()?
        };
        if let Some(expected) = self.expected_resume_observation.take() {
            if observation != expected {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "live Java boundary differs from the replayed resume boundary",
                ));
            }
        }
        self.record_boundary(&observation)?;
        let mut reload_target = None;
        if let Some((resume, execution)) = self.pending_native_reload.take() {
            if let Some(execution) = &execution {
                if execution.after != observation {
                    return Err(failure(
                        AdapterFailureKindV1::Evidence,
                        "native Continue execution differs from the live reconstructed boundary",
                    ));
                }
            }
            reload_target = resume.expected_observation.frame["public_base"]["decision"]["kind"]
                .as_str()
                .map(str::to_owned);
            self.master_deck_rebindings = rebind_master_deck_after_native_reload_v1(
                &resume.expected_observation,
                &observation,
                &self.bootstrap,
                &resume.master_deck_rebindings,
                &resume.combat_birth_bindings,
            )
            .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
            // Native room reload reconstructs combat objects and ends every
            // prior runtime lease. Combat births and transient selection
            // authorization must be proved again on the new branch.
            self.combat_birth_bindings.clear();
            self.active_combat_room = None;
            self.next_combat_action_ordinal = 0;
            self.hand_select_profile = None;
            self.next_acquisition_slots = resume.next_acquisition_slots;
            if let Some(execution) = execution {
                self.bind_expected_combat_birth_transition(
                    Some(JavaExpectedCombatBirthCauseV1 {
                        shared: NaturalRunCombatCardBirthCauseV1::CombatEntryResolution,
                        java_card_uuid: None,
                    }),
                    &execution.receipt,
                    &observation,
                )?;
            }
        }
        self.require_authorized_hand_select(&observation)?;
        self.align_combat_lifecycle(&observation)?;
        let mut snapshot = self.project_and_store(observation)?;
        if reload_target.as_deref() == Some("boss_relic")
            && snapshot.boundary_kind == "natural_boss_treasure"
        {
            // Continue reconstructs the unopened chest. Only this forced
            // reveal is preparation; choosing/skipping a relic stays committed.
            // The action adapter already requires one primary continuation;
            // optional typed potion discards remain available but are not taken.
            let producer = self.current_observation.as_ref().unwrap().producer.clone();
            let action = unique_current_action(
                &self.current_actions,
                |candidate| {
                    candidate.semantic
                        == SemanticActionV1::Proceed {
                            surface: DecisionSurfaceV1::BossRelic,
                        }
                },
                "native boss chest is not a closed reveal continuation",
            )?;
            let frame = self.current_observation.as_ref().unwrap().frame_id;
            let mut calls = Vec::new();
            snapshot = self.execute_one(
                &action.runtime_handle,
                &action.execution,
                frame,
                &action.semantic,
                RecordedJavaActionPhaseV1::NativeReloadSettlement,
                &producer,
                &mut calls,
            )?;
            if snapshot.boundary_kind != "natural_boss_relic" {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "native boss reveal reached an unexpected boundary",
                ));
            }
        }
        if reload_target.as_deref() == Some("shop")
            && snapshot.boundary_kind == "natural_shop_entry"
        {
            // Match the same reveal used after an adopted map entry. Native
            // Continue stops in front of the merchant; opening the inventory
            // spends no resource and leaves the shared cancel choice available.
            let producer = self.current_observation.as_ref().unwrap().producer.clone();
            let action = unique_current_action(
                &self.current_actions,
                |candidate| {
                    candidate.semantic
                        == SemanticActionV1::Proceed {
                            surface: DecisionSurfaceV1::Shop,
                        }
                },
                "native shop entry omitted its typed reveal continuation",
            )?;
            let frame = self.current_observation.as_ref().unwrap().frame_id;
            let mut calls = Vec::new();
            snapshot = self.execute_one(
                &action.runtime_handle,
                &action.execution,
                frame,
                &action.semantic,
                RecordedJavaActionPhaseV1::NativeReloadSettlement,
                &producer,
                &mut calls,
            )?;
            if snapshot.boundary_kind != "natural_shop" {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "native shop reveal reached an unexpected boundary",
                ));
            }
        }
        Ok(snapshot)
    }

    fn record_boundary(
        &mut self,
        observation: &JavaRuntimeObservationV1,
    ) -> Result<(), AdapterFailureV1> {
        if let Some(store) = self.evidence_store.as_mut() {
            store
                .append_boundary(observation)
                .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
        }
        Ok(())
    }

    fn project_and_store(
        &mut self,
        observation: JavaRuntimeObservationV1,
    ) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        let _timing = sts_parity_lockstep::timing_span_v1("java.project");
        let projected = project_snapshot_v1(
            &observation,
            &self.bootstrap,
            &self.master_deck_rebindings,
            &self.combat_birth_bindings,
            self.hand_select_profile,
        )
        .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
        let snapshot = projected.snapshot;
        remember_master_deck_acquisitions(&snapshot, &mut self.next_acquisition_slots)
            .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
        self.current_observation = Some(observation);
        self.current_snapshot = Some(snapshot.clone());
        self.current_actions = projected.actions;
        self.current_runtime_card_bindings = projected.runtime_card_bindings;
        Ok(snapshot)
    }

    fn align_combat_lifecycle(
        &mut self,
        observation: &JavaRuntimeObservationV1,
    ) -> Result<(), AdapterFailureV1> {
        let room = java_combat_room(observation)
            .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
        match room {
            Some(room) if self.active_combat_room.as_deref() != Some(room.as_str()) => {
                self.combat_birth_bindings.clear();
                self.active_combat_room = Some(room);
                self.next_combat_action_ordinal = 0;
            }
            Some(_) => {}
            None => {
                self.combat_birth_bindings.clear();
                self.active_combat_room = None;
                self.next_combat_action_ordinal = 0;
            }
        }
        Ok(())
    }

    fn require_authorized_hand_select(
        &self,
        observation: &JavaRuntimeObservationV1,
    ) -> Result<(), AdapterFailureV1> {
        if java_is_hand_select(observation) && self.hand_select_profile.is_none() {
            return Err(failure(
                AdapterFailureKindV1::Unsupported,
                "Java hand selection has no source-proven predecessor profile",
            ));
        }
        Ok(())
    }

    fn update_hand_select_authorization(
        &mut self,
        predecessor: &SemanticActionV1,
        successor: &JavaRuntimeObservationV1,
    ) -> Result<(), AdapterFailureV1> {
        if !java_is_hand_select(successor) {
            self.hand_select_profile = None;
            return Ok(());
        }
        self.hand_select_profile = if self.predecessor_is_bound_hand_card(
            predecessor,
            "truegrit",
            1,
        )? || self.predecessor_is_bound_hand_card(
            predecessor,
            "burningpact",
            0,
        )? {
            Some(JavaHandSelectProfileV1::SingleExhaust)
        } else if self.predecessor_is_bound_hand_card(predecessor, "survivor", 0)?
            || self.predecessor_is_bound_hand_card(predecessor, "daggerthrow", 0)?
            || self.predecessor_is_bound_hand_card(predecessor, "acrobatics", 0)?
        {
            Some(JavaHandSelectProfileV1::CardDiscard { cards: 1 })
        } else if self.predecessor_is_bound_hand_card(predecessor, "prepared", 0)? {
            Some(JavaHandSelectProfileV1::CardDiscard {
                cards: if self.predecessor_is_bound_hand_card(predecessor, "prepared", 1)? {
                    2
                } else {
                    1
                },
            })
        } else if self.predecessor_is_bound_hand_card(predecessor, "purity", 0)? {
            Some(JavaHandSelectProfileV1::OptionalExhaust {
                max_cards: if self.predecessor_is_bound_hand_card(predecessor, "purity", 1)? {
                    5
                } else {
                    3
                },
            })
        } else if self.predecessor_is_bound_hand_card(predecessor, "warcry", 0)?
            || self.predecessor_is_bound_hand_card(predecessor, "thinkingahead", 0)?
        {
            Some(JavaHandSelectProfileV1::SinglePutOnDrawPile)
        } else if self.predecessor_is_bound_hand_card(predecessor, "setup", 0)? {
            require_setup_free_once_capture(successor)?;
            Some(JavaHandSelectProfileV1::SinglePutOnDrawPile)
        } else if self.predecessor_is_bound_hand_card(predecessor, "armaments", 0)?
            && !self.predecessor_is_bound_hand_card(predecessor, "armaments", 1)?
        {
            Some(JavaHandSelectProfileV1::SingleUpgrade)
        } else if self.predecessor_is_bound_hand_card(predecessor, "dualwield", 0)? {
            // The selected producer must still be the actual played
            // object, suspended in cardInUse while DualWieldAction waits.
            let in_use = &successor.frame["parity_internal"]["zones"]["card_in_use"];
            let source_matches = self.current_observation.as_ref().is_some_and(|prior| {
                    prior.frame["parity_internal"]["zones"]["hand"]["cards"]
                        .as_array()
                        .is_some_and(|hand| hand.iter().any(|card| {
                            card["instance_uuid"].as_str().is_some_and(|uuid| {
                                matches!(predecessor, SemanticActionV1::PlayCard { card_binding, .. }
                                    if self.current_runtime_card_bindings.get(uuid) == Some(card_binding))
                                    && in_use["instance_uuid"].as_str() == Some(uuid)
                            })
                                && card["card_id"].as_str().map(canonical_game_id)
                                    == in_use["card_id"].as_str().map(canonical_game_id)
                                && card["upgrades"] == in_use["upgrades"]
                        }))
                });
            if !source_matches {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Dual Wield cardInUse disagrees with its source-bound played card",
                ));
            }
            Some(JavaHandSelectProfileV1::SingleCopy)
        } else if matches!(
            predecessor,
            SemanticActionV1::UsePotion {
                potion_id,
                target: None,
                ..
            } if potion_id == "gamblersbrew"
        ) {
            Some(JavaHandSelectProfileV1::GamblingChip)
        } else if matches!(
            predecessor,
            SemanticActionV1::UsePotion { potion_id, target: None, .. }
                if potion_id == "elixir"
        ) {
            Some(JavaHandSelectProfileV1::PotionExhaust)
        } else if let Some(profile) = self.end_turn_hand_select(predecessor, successor)? {
            Some(profile)
        } else if let Some(profile) = distilled_chaos_hand_select_profile(
            predecessor,
            self.current_observation.as_ref(),
            successor,
            &self.current_runtime_card_bindings,
        )? {
            Some(profile)
        } else {
            return Err(failure(
                AdapterFailureKindV1::Unsupported,
                "Java hand selection was not opened by an authorized typed predecessor",
            ));
        };
        Ok(())
    }

    fn end_turn_hand_select(
        &self,
        predecessor: &SemanticActionV1,
        successor: &JavaRuntimeObservationV1,
    ) -> Result<Option<JavaHandSelectProfileV1>, AdapterFailureV1> {
        let end_turn = matches!(predecessor, SemanticActionV1::EndTurn);
        let submitted_retain = matches!(
            predecessor,
            SemanticActionV1::SubmitSelection {
                surface: sts_parity_lockstep::DecisionSurfaceV1::HandSelect,
                ..
            }
        ) && matches!(
            self.hand_select_profile,
            Some(JavaHandSelectProfileV1::Retain { .. })
        );
        if !end_turn && !submitted_retain {
            return Ok(None);
        }
        let Some(DomainEvidenceV1::Observed { value, .. }) = self
            .current_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.domains.get(&ParityDomainV1::Combat))
            .map(|domain| &domain.evidence)
        else {
            return Ok(None);
        };
        let combat: sts_parity_lockstep::NaturalRunCombatV4 = serde_json::from_value(value.clone())
            .map_err(|error| failure(AdapterFailureKindV1::Evidence, error.to_string()))?;
        let sts_parity_lockstep::NaturalRunCombatV4::PlayerDecision { state } = combat else {
            return Ok(None);
        };
        // Retain is before the enemy turn; Tools of the Trade follows the
        // next normal draw. A retain submission can therefore reach Tools.
        let retain = end_turn
            && successor.frame["public_base"]["combat"]["turn"].as_u64()
                == Some(u64::from(state.turn));
        if (!retain
            && successor.frame["public_base"]["combat"]["turn"].as_u64()
                != Some(u64::from(state.turn) + 1))
            || !matches!(
                successor.frame["parity_internal"]["zones"].get("card_in_use"),
                Some(serde_json::Value::Null)
            )
        {
            return Ok(None);
        }
        let power_id = if retain {
            "retaincards"
        } else {
            "toolsofthetrade"
        };
        let Some(power) = state
            .player_powers
            .iter()
            .find(|power| power.power_id == power_id)
        else {
            return Ok(None);
        };
        let cards = usize::try_from(power.amount)
            .ok()
            .filter(|amount| *amount > 0)
            .ok_or_else(|| {
                failure(
                    AdapterFailureKindV1::Evidence,
                    "invalid end-turn selection power amount",
                )
            })?;
        Ok(Some(if retain {
            JavaHandSelectProfileV1::Retain { max_cards: cards }
        } else {
            JavaHandSelectProfileV1::ToolsOfTheTrade { cards }
        }))
    }

    fn predecessor_is_bound_hand_card(
        &self,
        semantic: &SemanticActionV1,
        expected_card_id: &str,
        minimum_upgrades: u64,
    ) -> Result<bool, AdapterFailureV1> {
        let SemanticActionV1::PlayCard {
            card_binding,
            card_id,
            ..
        } = semantic
        else {
            return Ok(false);
        };
        if card_id != expected_card_id {
            return Ok(false);
        }
        let uuid = self
            .current_runtime_card_bindings
            .iter()
            .find_map(|(uuid, binding)| (binding == card_binding).then_some(uuid.as_str()))
            .ok_or_else(|| {
                failure(
                    AdapterFailureKindV1::Evidence,
                    "Java hand-select predecessor has no runtime card binding",
                )
            })?;
        let observation = self.current_observation.as_ref().ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Evidence,
                "Java hand-select predecessor has no raw observation",
            )
        })?;
        let hand = observation.frame["parity_internal"]["zones"]["hand"]["cards"]
            .as_array()
            .ok_or_else(|| {
                failure(
                    AdapterFailureKindV1::Evidence,
                    "Java hand-select predecessor omitted its combat hand",
                )
            })?;
        Ok(hand.iter().any(|card| {
            card["instance_uuid"].as_str() == Some(uuid)
                && card["card_id"].as_str().map(canonical_game_id).as_deref()
                    == Some(expected_card_id)
                && card["upgrades"]
                    .as_u64()
                    .is_some_and(|upgrades| upgrades >= minimum_upgrades)
        }))
    }

    fn execute_one(
        &mut self,
        runtime_handle: &str,
        execution: &JavaActionExecutionV1,
        based_on_frame_id: u64,
        semantic: &SemanticActionV1,
        phase: RecordedJavaActionPhaseV1,
        expected_producer: &sts_parity_lockstep::ProducerIdentityV1,
        rng_calls: &mut Vec<NaturalRunRngCallV1>,
    ) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        let deck_transform = self.deck_transform_context(semantic, execution)?;
        let witnessed_transform = crate::deck_transform::MasterDeckTransformContext::prepare(
            self.current_observation.as_ref().ok_or_else(|| {
                failure(
                    AdapterFailureKindV1::Evidence,
                    "Java action has no predecessor",
                )
            })?,
            semantic,
            execution,
            &self.current_runtime_card_bindings,
            &self.master_deck_rebindings,
        )
        .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
        let card_reward = JavaMasterDeckAcquisitionV1::prepare(
            self.current_observation.as_ref(),
            semantic,
            runtime_handle,
            &self.next_acquisition_slots,
        )
        .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
        let receipt = {
            let _timing = sts_parity_lockstep::timing_span_v1("java.command");
            match execution {
                JavaActionExecutionV1::Act | JavaActionExecutionV1::PurgeShopCard { .. } => {
                    self.transport.act(runtime_handle)?
                }
                JavaActionExecutionV1::SubmitHandSelect {
                    selected_card_uuids,
                } => self
                    .transport
                    .submit_hand_select(runtime_handle, selected_card_uuids)?,
                JavaActionExecutionV1::SubmitGridSelect {
                    selected_card_uuids,
                } => self
                    .transport
                    .submit_grid_select(runtime_handle, selected_card_uuids)?,
            }
        };
        if let Some(store) = self.evidence_store.as_mut() {
            store
                .append_action(phase, semantic, &receipt)
                .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
        }
        if receipt.status != "completed" {
            return Err(failure(
                AdapterFailureKindV1::Rejected,
                format!("Java action ended with status {:?}", receipt.status),
            ));
        }
        if receipt.based_on_frame_id != based_on_frame_id {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java action receipt belongs to a different predecessor frame",
            ));
        }
        let expected_successor = receipt.successor_frame_id.ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Evidence,
                "Java completed action omitted its successor frame",
            )
        })?;
        let observation = {
            let _timing = sts_parity_lockstep::timing_span_v1("java.status");
            self.transport.status()?
        };
        self.record_boundary(&observation)?;
        let actual_frame = observation.frame_id;
        if actual_frame != expected_successor {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java status does not match the action receipt successor",
            ));
        }
        if &observation.producer != expected_producer {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java producer identity changed during a committed action",
            ));
        }
        let rng_call_ledger = validate_rng_call_transition(&receipt, &observation)?;
        self.execution_inputs
            .append(
                crate::execution_inputs::validate_execution_input_transition(
                    &receipt,
                    &observation,
                )?,
            )
            .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
        if let Some(acquisition) = card_reward {
            let (uuid, binding) = acquisition
                .bind(&receipt, &observation)
                .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
            if self.master_deck_rebindings.contains_key(&uuid)
                || self.combat_birth_bindings.contains_key(&uuid)
            {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java card reward reused an existing runtime UUID",
                ));
            }
            self.master_deck_rebindings.insert(uuid, binding);
        }
        self.bind_master_deck_transform_transition(deck_transform, &observation)?;
        if let Some(context) = witnessed_transform {
            context
                .bind(
                    &observation,
                    &mut self.master_deck_rebindings,
                    &self.combat_birth_bindings,
                )
                .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
        }
        self.bind_combat_birth_transition(semantic, &receipt, &observation)?;
        self.update_hand_select_authorization(semantic, &observation)?;
        self.align_combat_lifecycle(&observation)?;
        let successor = self.project_and_store(observation)?;
        rng_calls.extend(rng_call_ledger.0);
        Ok(successor)
    }

    fn deck_transform_context(
        &self,
        semantic: &SemanticActionV1,
        execution: &JavaActionExecutionV1,
    ) -> Result<Option<JavaDeckTransformContextV1>, AdapterFailureV1> {
        let SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::GridSelect,
            choices,
        } = semantic
        else {
            return Ok(None);
        };
        let observation = self.current_observation.as_ref().ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Evidence,
                "Java deck mutation has no predecessor observation",
            )
        })?;
        if observation.frame["public_base"]["decision"]["grid_select"]["selection_reason"].as_str()
            != Some("DeckTransform")
        {
            return Ok(None);
        }
        let identity = java_grid_select_boundary_identity_v1(observation)
            .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
        if identity.selection_kind != "deck_transform" {
            return Ok(None);
        }
        if observation.frame["public_base"]["decision"]["event_id"].as_str() == Some("DrugDealer") {
            // This multi-card transition is bound by actual original transform calls.
            return Ok(None);
        }
        let [choice] = choices.as_slice() else {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java deck transform must select exactly one typed card",
            ));
        };
        let JavaActionExecutionV1::SubmitGridSelect {
            selected_card_uuids,
        } = execution
        else {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java deck transform omitted its selected runtime card",
            ));
        };
        let [selected_uuid] = selected_card_uuids.as_slice() else {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java deck transform did not select exactly one runtime card",
            ));
        };
        if self.current_runtime_card_bindings.get(selected_uuid) != Some(&choice.id) {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java deck transform selection disagrees with its stable binding",
            ));
        }

        let cards = observation.frame["parity_internal"]["master_deck"]
            .as_array()
            .ok_or_else(|| {
                failure(
                    AdapterFailureKindV1::Evidence,
                    "Java deck transform predecessor omitted its master deck",
                )
            })?;
        let mut prior_uuids = BTreeSet::new();
        let mut selected_card = None;
        for card in cards {
            let uuid = card["card_uuid"]
                .as_str()
                .filter(|uuid| !uuid.is_empty())
                .ok_or_else(|| {
                    failure(
                        AdapterFailureKindV1::Evidence,
                        "Java deck transform predecessor omitted a card UUID",
                    )
                })?;
            if !prior_uuids.insert(uuid.to_owned()) {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java deck transform predecessor repeats a card UUID",
                ));
            }
            if uuid == selected_uuid {
                selected_card = Some(card);
            }
        }
        let selected_card = selected_card.ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Evidence,
                "Java deck transform selection is absent from the predecessor deck",
            )
        })?;
        let rebound = if let Some(existing) = self.master_deck_rebindings.get(selected_uuid) {
            existing.clone()
        } else {
            let source = selected_card
                .get("source")
                .filter(|source| !source.is_null())
                .ok_or_else(|| {
                    failure(
                        AdapterFailureKindV1::Evidence,
                        "Java deck transform selected a card without typed provenance",
                    )
                })?;
            let source = serde_json::from_value::<NaturalRunCardSourceV1>(source.clone()).map_err(
                |error| {
                    failure(
                        AdapterFailureKindV1::Evidence,
                        format!("Java deck transform selected a malformed card source: {error}"),
                    )
                },
            )?;
            let binding = natural_run_card_binding_v1(&source)
                .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
            JavaStableCardBindingV1 {
                binding,
                raw_source: Some(source.clone()),
                source,
            }
        };
        if rebound.binding != choice.id {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java deck transform provenance disagrees with its selected binding",
            ));
        }
        Ok(Some(JavaDeckTransformContextV1 {
            selected_uuid: selected_uuid.clone(),
            prior_uuids,
            rebound,
        }))
    }

    fn bind_master_deck_transform_transition(
        &mut self,
        context: Option<JavaDeckTransformContextV1>,
        successor: &JavaRuntimeObservationV1,
    ) -> Result<(), AdapterFailureV1> {
        let Some(context) = context else {
            return Ok(());
        };
        let cards = successor.frame["parity_internal"]["master_deck"]
            .as_array()
            .ok_or_else(|| {
                failure(
                    AdapterFailureKindV1::Evidence,
                    "Java deck transform successor omitted its master deck",
                )
            })?;
        let mut successor_uuids = BTreeSet::new();
        for card in cards {
            let uuid = card["card_uuid"]
                .as_str()
                .filter(|uuid| !uuid.is_empty())
                .ok_or_else(|| {
                    failure(
                        AdapterFailureKindV1::Evidence,
                        "Java deck transform successor omitted a card UUID",
                    )
                })?;
            if !successor_uuids.insert(uuid.to_owned()) {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java deck transform successor repeats a card UUID",
                ));
            }
        }
        let removed = context
            .prior_uuids
            .difference(&successor_uuids)
            .collect::<Vec<_>>();
        let added = successor_uuids
            .difference(&context.prior_uuids)
            .collect::<Vec<_>>();
        if removed.as_slice() != [&context.selected_uuid] || added.len() != 1 {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                format!(
                    "Java single-card transform replaced {} old cards with {} new cards",
                    removed.len(),
                    added.len()
                ),
            ));
        }
        let product_uuid = (*added[0]).clone();
        if self.master_deck_rebindings.iter().any(|(uuid, binding)| {
            uuid != &context.selected_uuid && binding.binding == context.rebound.binding
        }) || self.master_deck_rebindings.contains_key(&product_uuid)
            || self.combat_birth_bindings.contains_key(&product_uuid)
        {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java deck transform reused an active runtime or stable binding",
            ));
        }
        self.master_deck_rebindings.remove(&context.selected_uuid);
        if self
            .master_deck_rebindings
            .insert(product_uuid, context.rebound)
            .is_some()
        {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java deck transform reused an existing runtime UUID",
            ));
        }
        Ok(())
    }

    fn bind_combat_birth_transition(
        &mut self,
        semantic: &SemanticActionV1,
        receipt: &JavaLockstepReceiptV1,
        successor: &JavaRuntimeObservationV1,
    ) -> Result<(), AdapterFailureV1> {
        self.bind_expected_combat_birth_transition(
            self.expected_combat_birth_cause(semantic)?,
            receipt,
            successor,
        )
    }

    fn bind_expected_combat_birth_transition(
        &mut self,
        expected_cause: Option<JavaExpectedCombatBirthCauseV1>,
        receipt: &JavaLockstepReceiptV1,
        successor: &JavaRuntimeObservationV1,
    ) -> Result<(), AdapterFailureV1> {
        let raw_witness = &successor.frame["card_birth_witness"];
        let witness = (!raw_witness.is_null())
            .then(|| {
                serde_json::from_value::<CardBirthWitness>(raw_witness.clone()).map_err(|error| {
                    failure(
                        AdapterFailureKindV1::Evidence,
                        format!("Java card-birth witness is malformed: {error}"),
                    )
                })
            })
            .transpose()?;
        let (expected_cause, witness) = match (expected_cause, witness) {
            (None, None) => return Ok(()),
            (Some(expected), None)
                if expected.shared
                    == NaturalRunCombatCardBirthCauseV1::CombatSelectionResolution =>
            {
                // Selections need birth evidence only when their resumed queue
                // creates a fresh successor card. Normal projection still
                // rejects every UUID without an existing or witnessed binding.
                return Ok(());
            }
            (Some(_), None) => {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java combat action omitted its typed card-birth witness",
                ))
            }
            (None, Some(_)) => {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java non-birth action exposed an unexpected card-birth witness",
                ))
            }
            (Some(expected), Some(witness)) => (expected, witness),
        };
        if witness.kind != "card_birth_witness_v1"
            || witness.from_frame_id != receipt.based_on_frame_id
            || witness.to_frame_id != successor.frame_id
            || witness.request_id != receipt.request_id
            || witness.action_id != receipt.action_id
        {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java card-birth witness does not bind the executed action and adjacent frames",
            ));
        }
        if !witness.complete || witness.unsupported_reason.is_some() {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                format!(
                    "Java card-birth witness is incomplete{}",
                    witness
                        .unsupported_reason
                        .as_deref()
                        .map(|reason| format!(": {reason}"))
                        .unwrap_or_default()
                ),
            ));
        }
        validate_java_birth_cause(&expected_cause, &witness.cause)?;
        if expected_cause.shared == NaturalRunCombatCardBirthCauseV1::CombatEntryResolution {
            self.align_combat_lifecycle(successor)?;
            // Continue is recorded before the character is loaded. Only the
            // Watcher adapter currently owns a combat-entry birth ordinal;
            // other characters must prove an empty entry without advancing it.
            if self.active_combat_room.is_none()
                || successor.frame["public_base"]["run"]["character"].as_str() != Some("WATCHER")
            {
                return if witness.products.is_empty() {
                    Ok(())
                } else {
                    Err(failure(
                        AdapterFailureKindV1::Evidence,
                        "Java entry without an owned combat-birth scope produced cards",
                    ))
                };
            }
        }
        if expected_cause.shared == NaturalRunCombatCardBirthCauseV1::CombatSelectionResolution
            && witness.products.is_empty()
        {
            return Ok(());
        }
        if witness
            .products
            .iter()
            .enumerate()
            .any(|(index, product)| u32::try_from(index) != Ok(product.generation_index))
        {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java card-birth witness has non-contiguous products",
            ));
        }

        let room_id = self.active_combat_room.as_deref().ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Evidence,
                "Java combat action has no active room identity",
            )
        })?;
        let next_ordinal = self
            .next_combat_action_ordinal
            .checked_add(1)
            .ok_or_else(|| {
                failure(
                    AdapterFailureKindV1::Evidence,
                    "Java combat action ordinal overflowed",
                )
            })?;
        let mut additions = Vec::with_capacity(witness.products.len());
        for product in &witness.products {
            if product.java_uuid.is_empty()
                || self
                    .current_runtime_card_bindings
                    .contains_key(&product.java_uuid)
                || self.combat_birth_bindings.contains_key(&product.java_uuid)
                || self.master_deck_rebindings.contains_key(&product.java_uuid)
            {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java combat birth repeats or omits its runtime card UUID",
                ));
            }
            validate_java_birth_product(successor, product)?;
            if let Some(copy) = &product.autoplay_copy {
                if copy.producer_java_uuid == product.java_uuid
                    || (!self
                        .current_runtime_card_bindings
                        .contains_key(&copy.producer_java_uuid)
                        && !self
                            .combat_birth_bindings
                            .contains_key(&copy.producer_java_uuid)
                        && !additions
                            .iter()
                            .any(|(uuid, _)| uuid == &copy.producer_java_uuid))
                {
                    return Err(failure(
                        AdapterFailureKindV1::Evidence,
                        "Java auto-play copy has no previously witnessed producer",
                    ));
                }
            }
            let source = NaturalRunCardSourceV1::CombatBirth {
                room_id: room_id.to_owned(),
                action_ordinal: self.next_combat_action_ordinal,
                cause: expected_cause.shared.clone(),
                product_slot: product.generation_index,
            };
            let binding = natural_run_card_binding_v1(&source)
                .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
            if self
                .combat_birth_bindings
                .values()
                .any(|existing| existing.binding == binding)
                || additions
                    .iter()
                    .any(|(_, existing): &(String, JavaStableCardBindingV1)| {
                        existing.binding == binding
                    })
            {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java combat birth repeats one stable binding",
                ));
            }
            additions.push((
                product.java_uuid.clone(),
                JavaStableCardBindingV1 {
                    binding,
                    source,
                    raw_source: None,
                },
            ));
        }
        self.combat_birth_bindings.extend(additions);
        self.next_combat_action_ordinal = next_ordinal;
        Ok(())
    }

    fn expected_combat_birth_cause(
        &self,
        semantic: &SemanticActionV1,
    ) -> Result<Option<JavaExpectedCombatBirthCauseV1>, AdapterFailureV1> {
        if (matches!(
            semantic,
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::Map,
                ..
            }
        ) || (matches!(
            semantic,
            SemanticActionV1::Proceed {
                surface: DecisionSurfaceV1::DoubleBoss
            }
        ) && self
            .current_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.boundary_kind == "natural_double_boss_complete")))
            && self
                .current_observation
                .as_ref()
                .is_some_and(|observation| {
                    observation.frame["public_base"]["run"]["character"].as_str() == Some("WATCHER")
                })
        {
            return Ok(Some(JavaExpectedCombatBirthCauseV1 {
                shared: NaturalRunCombatCardBirthCauseV1::CombatEntryResolution,
                java_card_uuid: None,
            }));
        }
        if matches!(
            semantic,
            SemanticActionV1::SubmitSelection {
                surface: DecisionSurfaceV1::HandSelect,
                ..
            }
        ) && self
            .current_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.boundary_kind == "natural_hand_select")
            || matches!(
                semantic,
                SemanticActionV1::SubmitSelection {
                    surface: DecisionSurfaceV1::GridSelect,
                    ..
                }
            ) && self
                .current_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.boundary_kind == "natural_combat_grid_select")
        {
            return Ok(Some(JavaExpectedCombatBirthCauseV1 {
                shared: NaturalRunCombatCardBirthCauseV1::CombatSelectionResolution,
                java_card_uuid: None,
            }));
        }
        if self
            .current_snapshot
            .as_ref()
            .is_some_and(|s| s.boundary_kind == "natural_combat_codex")
        {
            return match semantic {
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::CombatCodex,
                    ..
                }
                | SemanticActionV1::Cancel {
                    surface: DecisionSurfaceV1::CombatCodex,
                } => Ok(Some(JavaExpectedCombatBirthCauseV1 {
                    shared: NaturalRunCombatCardBirthCauseV1::CombatSelectionResolution,
                    java_card_uuid: None,
                })),
                SemanticActionV1::DiscardPotion { .. } => Ok(None),
                _ => Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java Codex resolution requires its typed choice",
                )),
            };
        }
        if self
            .current_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.boundary_kind == "natural_combat_discovery")
        {
            return match semantic {
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::CombatDiscovery,
                    ..
                }
                | SemanticActionV1::Cancel {
                    surface: DecisionSurfaceV1::CombatDiscovery,
                } => Ok(Some(JavaExpectedCombatBirthCauseV1 {
                    shared: NaturalRunCombatCardBirthCauseV1::DiscoveryResolution,
                    java_card_uuid: None,
                })),
                SemanticActionV1::DiscardPotion { .. } => Ok(None),
                _ => Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java Discovery resolution requires its typed choice",
                )),
            };
        }
        if self
            .current_snapshot
            .as_ref()
            .is_none_or(|snapshot| snapshot.boundary_kind != "natural_combat")
        {
            return Ok(None);
        }
        let cause = match semantic {
            SemanticActionV1::PlayCard { card_binding, .. } => {
                let matches = self
                    .current_runtime_card_bindings
                    .iter()
                    .filter(|(_, binding)| *binding == card_binding)
                    .map(|(uuid, _)| uuid)
                    .collect::<Vec<_>>();
                let [producer_uuid] = matches.as_slice() else {
                    return Err(failure(
                        AdapterFailureKindV1::Evidence,
                        format!(
                            "Java card-play binding resolves to {} runtime UUIDs instead of one",
                            matches.len()
                        ),
                    ));
                };
                JavaExpectedCombatBirthCauseV1 {
                    shared: NaturalRunCombatCardBirthCauseV1::CardPlay {
                        producer_binding: card_binding.clone(),
                    },
                    java_card_uuid: Some((*producer_uuid).clone()),
                }
            }
            SemanticActionV1::EndTurn => JavaExpectedCombatBirthCauseV1 {
                shared: NaturalRunCombatCardBirthCauseV1::EndTurnResolution,
                java_card_uuid: None,
            },
            SemanticActionV1::UsePotion {
                slot, potion_id, ..
            } => JavaExpectedCombatBirthCauseV1 {
                shared: NaturalRunCombatCardBirthCauseV1::PotionUse {
                    potion_id: potion_id.clone(),
                    slot: *slot,
                },
                java_card_uuid: None,
            },
            _ => return Ok(None),
        };
        Ok(Some(cause))
    }
}

fn validate_resume_state_v1(resume: &JavaLockstepResumeStateV1) -> Result<(), String> {
    if resume
        .expected_observation
        .producer
        .implementation
        .is_empty()
        || resume.expected_observation.frame_id == 0
    {
        return Err("Java resume state has an invalid recorded boundary".to_owned());
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct JavaExpectedCombatBirthCauseV1 {
    shared: NaturalRunCombatCardBirthCauseV1,
    java_card_uuid: Option<String>,
}

fn validate_java_birth_cause(
    expected: &JavaExpectedCombatBirthCauseV1,
    actual: &CardBirthCause,
) -> Result<(), AdapterFailureV1> {
    let matches = match (&expected.shared, actual) {
        (
            NaturalRunCombatCardBirthCauseV1::CombatEntryResolution,
            CardBirthCause::CombatEntryResolution,
        ) => true,
        (
            NaturalRunCombatCardBirthCauseV1::CombatSelectionResolution,
            CardBirthCause::CombatSelectionResolution,
        ) => true,
        (
            NaturalRunCombatCardBirthCauseV1::DiscoveryResolution,
            CardBirthCause::DiscoveryResolution,
        ) => true,
        (
            NaturalRunCombatCardBirthCauseV1::CardPlay { .. },
            CardBirthCause::CardPlay { producer_java_uuid },
        ) => expected.java_card_uuid.as_deref() == Some(producer_java_uuid.as_str()),
        (
            NaturalRunCombatCardBirthCauseV1::EndTurnResolution,
            CardBirthCause::EndTurnResolution,
        ) => true,
        (
            NaturalRunCombatCardBirthCauseV1::PotionUse { potion_id, slot },
            CardBirthCause::PotionUse {
                potion_id: actual_id,
                slot: actual_slot,
            },
        ) => canonical_game_id(actual_id) == *potion_id && usize::from(*slot) == *actual_slot,
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(failure(
            AdapterFailureKindV1::Evidence,
            "Java card-birth witness disagrees with its typed action cause",
        ))
    }
}

fn validate_java_birth_product(
    successor: &JavaRuntimeObservationV1,
    product: &CardBirthProduct,
) -> Result<(), AdapterFailureV1> {
    if product.first_zone.is_some() == product.autoplay_copy.is_some() {
        return Err(failure(
            AdapterFailureKindV1::Evidence,
            "Java combat birth must have exactly one zone-entry or auto-play-copy origin",
        ));
    }
    let zones = &successor.frame["parity_internal"]["zones"];
    // An enemy or resumed Codex may create a card before that transition wins
    // combat. The birth witness remains valid, but completed combat no longer
    // owns piles or bindings, including A20's rewardless boss handoff.
    // Missing zones at a continuing combat boundary must still fail below.
    if zones.is_null()
        && successor.frame["boundary"]["evidence"]["room_phase"].as_str() == Some("COMPLETE")
        && crate::projection::boundary_kind(successor).is_ok_and(|kind| {
            matches!(
                kind,
                "natural_combat_reward"
                    | "natural_double_boss_complete"
                    | "natural_act3_victory_complete"
                    | "natural_true_victory_complete"
                    | "natural_heart_victory"
            )
        })
    {
        return Ok(());
    }
    let mut matching_cards = 0_usize;
    for zone_name in ["draw", "hand", "discard", "exhaust", "limbo"] {
        let cards = zones[zone_name]["cards"].as_array().ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Evidence,
                format!("Java combat-birth successor omitted zone {zone_name:?}"),
            )
        })?;
        matching_cards += cards
            .iter()
            .filter(|card| card["instance_uuid"].as_str() == Some(product.java_uuid.as_str()))
            .map(|card| {
                let card_id = card["card_id"].as_str().ok_or_else(|| {
                    failure(
                        AdapterFailureKindV1::Evidence,
                        "Java combat-birth product omitted its card ID",
                    )
                })?;
                let upgrades = card["upgrades"].as_u64().ok_or_else(|| {
                    failure(
                        AdapterFailureKindV1::Evidence,
                        "Java combat-birth product omitted its upgrades",
                    )
                })?;
                if canonical_game_id(card_id) != canonical_game_id(&product.card_id)
                    || upgrades != u64::from(product.upgrades)
                {
                    return Err(failure(
                        AdapterFailureKindV1::Evidence,
                        "Java combat-birth product changed semantics before its successor",
                    ));
                }
                Ok(())
            })
            .collect::<Result<Vec<_>, _>>()?
            .len();
    }
    if let Some(copy) = &product.autoplay_copy {
        if copy.purged && matching_cards == 0 {
            return Ok(());
        }
        if copy.purged {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java purged auto-play copy remains in a successor zone",
            ));
        }
        // The producer directly witnessed this object; it may be retained by
        // a pending NewQueueCardAction or UseCardAction outside all CardGroups.
        // Recorder.finish verifies its actual retention before complete=true.
        return Ok(());
    }
    if matching_cards == 0 {
        return Err(failure(
            AdapterFailureKindV1::Evidence,
            "Java combat-birth product is absent from the stable successor",
        ));
    }
    Ok(())
}

fn java_combat_room(observation: &JavaRuntimeObservationV1) -> Result<Option<String>, String> {
    if !(observation.frame["boundary"]["kind"].as_str() == Some("combat_player_decision")
        || crate::action::is_supported_combat_grid_select(observation)
        || java_is_hand_select(observation)
        || (observation.frame["public_base"]["decision"]["kind"].as_str() == Some("card_reward")
            && matches!(
                observation.frame["public_base"]["decision"]["card_reward"]["source_kind"].as_str(),
                Some("combat_discovery" | "codex")
            )))
    {
        return Ok(None);
    }
    current_node(observation)?
        .ok_or_else(|| "Java combat boundary has no current room".to_owned())
        .map(Some)
}

fn java_is_hand_select(observation: &JavaRuntimeObservationV1) -> bool {
    observation.frame["boundary"]["kind"].as_str() == Some("run_decision")
        && observation.frame["public_base"]["decision"]["kind"].as_str() == Some("hand_select")
}

impl<T> LockstepAdapterV1 for JavaLockstepAdapterV1<T>
where
    T: JavaLockstepTransportV1,
{
    type PreparedAction = PreparedJavaActionV1;

    fn observe(&mut self) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        self.capture()
    }

    fn prepare(
        &mut self,
        action: &SemanticActionV1,
        based_on: &ParitySnapshotV1,
    ) -> Result<Self::PreparedAction, AdapterFailureV1> {
        let _timing = sts_parity_lockstep::timing_span_v1("java.prepare");
        let Some(current) = self.current_snapshot.as_ref() else {
            return Err(failure(
                AdapterFailureKindV1::Rejected,
                "Java adapter must observe before preparing an action",
            ));
        };
        if current != based_on {
            return Err(failure(
                AdapterFailureKindV1::Rejected,
                "Java action was based on a stale or foreign observation",
            ));
        }
        let matches = self
            .current_actions
            .iter()
            .filter(|candidate| &candidate.semantic == action)
            .collect::<Vec<_>>();
        let [resolved] = matches.as_slice() else {
            return Err(failure(
                AdapterFailureKindV1::Unsupported,
                format!(
                    "Java action resolves to {} current candidates instead of one",
                    matches.len()
                ),
            ));
        };
        Ok(PreparedJavaActionV1 {
            based_on_observation_id: based_on.observation_id.clone(),
            based_on_frame_id: self
                .current_observation
                .as_ref()
                .expect("observed snapshot retains its raw observation")
                .frame_id,
            semantic: resolved.semantic.clone(),
            runtime_handle: resolved.runtime_handle.clone(),
            execution: resolved.execution.clone(),
        })
    }

    fn execute(
        &mut self,
        prepared: Self::PreparedAction,
    ) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        let _timing = sts_parity_lockstep::timing_span_v1("java.execute");
        let current = self.current_snapshot.as_ref().ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Rejected,
                "Java adapter has no current observation",
            )
        })?;
        let current_frame = self
            .current_observation
            .as_ref()
            .expect("snapshot and raw observation are stored together")
            .frame_id;
        if current.observation_id != prepared.based_on_observation_id
            || current_frame != prepared.based_on_frame_id
            || !self.current_actions.iter().any(|candidate| {
                candidate.semantic == prepared.semantic
                    && candidate.runtime_handle == prepared.runtime_handle
                    && candidate.execution == prepared.execution
            })
        {
            return Err(failure(
                AdapterFailureKindV1::Rejected,
                "Java prepared action became stale before execution",
            ));
        }

        self.last_rng_call_ledger = None;
        self.execution_inputs = ExecutionInputsV1::observed_empty();
        let producer = current.producer.clone();
        let finishes_astrolabe = matches!(
            &prepared.semantic,
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::GridSelect,
                ..
            }
        ) && crate::action::astrolabe_grid_selection_v1(
            self.current_observation.as_ref().unwrap(),
        )
        .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?
        .is_some_and(|selected| selected.len() == 2);
        let finishes_empty_cage =
            matches!(
                &prepared.semantic,
                SemanticActionV1::SubmitSelection {
                    surface: DecisionSurfaceV1::GridSelect,
                    ..
                }
            ) && crate::action::empty_cage_grid_v1(self.current_observation.as_ref().unwrap())
                .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
        let mut rng_calls = Vec::new();
        let mut successor = self.execute_one(
            &prepared.runtime_handle,
            &prepared.execution,
            prepared.based_on_frame_id,
            &prepared.semantic,
            RecordedJavaActionPhaseV1::Committed,
            &producer,
            &mut rng_calls,
        )?;

        if let JavaActionExecutionV1::PurgeShopCard { selected_card_uuid } = &prepared.execution {
            let SemanticActionV1::Select { choice, .. } = &prepared.semantic else {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java purge omitted its adopted target",
                ));
            };
            if self.current_runtime_card_bindings.get(selected_card_uuid) != Some(&choice.id)
                || successor.boundary_kind != "natural_shop_purge_card"
            {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java purge preparation changed its source-bound target",
                ));
            }
            for _ in 0..2 {
                let action = unique_current_action(
                    &self.current_actions,
                    |action| action.semantic == prepared.semantic,
                    "Java purge continuation is not uniquely bound to its adopted card",
                )?;
                let frame = self.current_observation.as_ref().unwrap().frame_id;
                successor = self.execute_one(
                    &action.runtime_handle,
                    &action.execution,
                    frame,
                    &action.semantic,
                    RecordedJavaActionPhaseV1::InterstitialSettlement,
                    &producer,
                    &mut rng_calls,
                )?;
                if successor.boundary_kind == "natural_shop" {
                    break;
                }
                if successor.boundary_kind != "natural_shop_purge_confirm" {
                    return Err(failure(
                        AdapterFailureKindV1::Evidence,
                        "Java purge reached an unexpected boundary",
                    ));
                }
            }
            if successor.boundary_kind != "natural_shop" {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java purge exceeded bounded confirmation",
                ));
            }
        }
        // Opening the merchant only reveals the inventory. Leaving remains an
        // available shared choice; no purchase or resource decision is made.
        if successor.boundary_kind == "natural_shop_entry" {
            let (semantic, expected) = match &prepared.semantic {
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Map,
                    ..
                }
                | SemanticActionV1::Cancel {
                    surface: DecisionSurfaceV1::CombatReward,
                } => (
                    SemanticActionV1::Proceed {
                        surface: DecisionSurfaceV1::Shop,
                    },
                    "natural_shop",
                ),
                SemanticActionV1::Cancel {
                    surface: DecisionSurfaceV1::Shop,
                } => (
                    SemanticActionV1::Cancel {
                        surface: DecisionSurfaceV1::Shop,
                    },
                    "natural_map",
                ),
                _ => {
                    return Err(failure(
                        AdapterFailureKindV1::Unsupported,
                        "Java shop entry has no adopted enter or leave intent",
                    ))
                }
            };
            let action = unique_current_action(
                &self.current_actions,
                |action| action.semantic == semantic,
                "Java shop entry omitted its typed continuation",
            )?;
            let frame = self.current_observation.as_ref().unwrap().frame_id;
            successor = self.execute_one(
                &action.runtime_handle,
                &action.execution,
                frame,
                &action.semantic,
                RecordedJavaActionPhaseV1::InterstitialSettlement,
                &producer,
                &mut rng_calls,
            )?;
            if successor.boundary_kind != expected {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java shop continuation reached an unexpected boundary",
                ));
            }
        }

        // The original game exposes two deterministic Talk pages before the
        // Neow reward screen, while the simulator models one introduction
        // intent. Settle only this closed, single-action UI interstitial.
        if matches!(
            prepared.semantic,
            SemanticActionV1::Proceed {
                surface: DecisionSurfaceV1::Neow
            }
        ) {
            for _ in 0..2 {
                if !is_neow_introduction(&successor) {
                    break;
                }
                let matches = self
                    .current_actions
                    .iter()
                    .filter(|candidate| candidate.semantic == prepared.semantic)
                    .collect::<Vec<_>>();
                let [interstitial] = matches.as_slice() else {
                    return Err(failure(
                        AdapterFailureKindV1::Evidence,
                        "Java Neow introduction is not a closed single-action interstitial",
                    ));
                };
                let handle = interstitial.runtime_handle.clone();
                let execution = interstitial.execution.clone();
                let frame_id = self
                    .current_observation
                    .as_ref()
                    .expect("captured interstitial retains its observation")
                    .frame_id;
                successor = self.execute_one(
                    &handle,
                    &execution,
                    frame_id,
                    &prepared.semantic,
                    RecordedJavaActionPhaseV1::InterstitialSettlement,
                    &producer,
                    &mut rng_calls,
                )?;
            }
            if is_neow_introduction(&successor) {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java Neow introduction exceeded its bounded settlement",
                ));
            }
        }
        if is_campfire_option(&prepared.semantic, "rest")
            || is_campfire_option(&prepared.semantic, "recall")
        {
            successor = self.settle_campfire_complete(successor, &producer, &mut rng_calls)?;
        } else if is_campfire_deck_card(&prepared.semantic) {
            if successor.boundary_kind != "natural_campfire_smith_confirm" {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java Smith card selection did not reach its confirmation boundary",
                ));
            }
            let interstitial = unique_current_action(
                &self.current_actions,
                |candidate| candidate.semantic == prepared.semantic,
                "Java Smith confirmation is not bound to exactly one selected card",
            )?;
            let frame_id = self
                .current_observation
                .as_ref()
                .expect("captured Smith confirmation retains its observation")
                .frame_id;
            successor = self.execute_one(
                &interstitial.runtime_handle,
                &interstitial.execution,
                frame_id,
                &interstitial.semantic,
                RecordedJavaActionPhaseV1::InterstitialSettlement,
                &producer,
                &mut rng_calls,
            )?;
            if successor.boundary_kind == "natural_campfire_smith_confirm" {
                return Err(failure(
                    AdapterFailureKindV1::Evidence,
                    "Java Smith confirmation exceeded its bounded settlement",
                ));
            }
            successor = self.settle_campfire_complete(successor, &producer, &mut rng_calls)?;
        }
        // Rust folds the mandatory chest-open/leave clicks into the adjacent
        // reward choice. Retain each raw boundary and receipt, and settle only
        // one exactly identified continuation with no strategic alternative.
        let boss_interstitial = match &prepared.semantic {
            SemanticActionV1::SubmitSelection {
                surface: DecisionSurfaceV1::GridSelect,
                ..
            } if finishes_empty_cage
                && successor.boundary_kind == "natural_boss_treasure_complete" =>
            {
                Some("natural_map")
            }
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::GridSelect,
                ..
            } if finishes_astrolabe
                && successor.boundary_kind == "natural_boss_treasure_complete" =>
            {
                Some("natural_map")
            }
            SemanticActionV1::Proceed {
                surface: DecisionSurfaceV1::CombatReward,
            } if successor.boundary_kind == "natural_boss_treasure" => Some("natural_boss_relic"),
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::BossRelic,
                ..
            }
            | SemanticActionV1::Cancel {
                surface: DecisionSurfaceV1::BossRelic,
            } if successor.boundary_kind == "natural_boss_treasure_complete" => Some("natural_map"),
            _ => None,
        };
        if let Some(expected) = boss_interstitial {
            let action = unique_current_action(
                &self.current_actions,
                |candidate| {
                    matches!(
                        candidate.semantic,
                        SemanticActionV1::Proceed {
                            surface: DecisionSurfaceV1::BossRelic
                        }
                    )
                },
                "boss chest is not a closed single-action continuation",
            )?;
            let frame = self.current_observation.as_ref().unwrap().frame_id;
            successor = self.execute_one(
                &action.runtime_handle,
                &action.execution,
                frame,
                &action.semantic,
                RecordedJavaActionPhaseV1::InterstitialSettlement,
                &producer,
                &mut rng_calls,
            )?;
            if successor.boundary_kind != expected {
                return Err(failure(
                    AdapterFailureKindV1::Unsupported,
                    "boss chest continuation reached an unexpected boundary",
                ));
            }
        }
        self.last_rng_call_ledger = Some(aggregate_rng_calls(rng_calls)?);
        Ok(successor)
    }

    fn last_rng_call_ledger_v1(&self) -> Option<&NaturalRunRngCallLedgerV1> {
        self.last_rng_call_ledger.as_ref()
    }

    fn last_execution_inputs_v1(&self) -> ExecutionInputsV1 {
        self.execution_inputs.clone()
    }
}

impl<T> JavaLockstepAdapterV1<T>
where
    T: JavaLockstepTransportV1,
{
    fn settle_campfire_complete(
        &mut self,
        mut successor: ParitySnapshotV1,
        producer: &sts_parity_lockstep::ProducerIdentityV1,
        rng_calls: &mut Vec<NaturalRunRngCallV1>,
    ) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        if successor.boundary_kind != "natural_campfire_complete" {
            return Ok(successor);
        }
        let interstitial = unique_current_action(
            &self.current_actions,
            |candidate| {
                matches!(
                    candidate.semantic,
                    SemanticActionV1::Proceed {
                        surface: DecisionSurfaceV1::Campfire
                    }
                )
            },
            "Java completed campfire is not a closed single-action interstitial",
        )?;
        let frame_id = self
            .current_observation
            .as_ref()
            .expect("captured campfire completion retains its observation")
            .frame_id;
        successor = self.execute_one(
            &interstitial.runtime_handle,
            &interstitial.execution,
            frame_id,
            &interstitial.semantic,
            RecordedJavaActionPhaseV1::InterstitialSettlement,
            producer,
            rng_calls,
        )?;
        if successor.boundary_kind == "natural_campfire_complete" {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Java completed campfire exceeded its bounded settlement",
            ));
        }
        Ok(successor)
    }
}

fn validate_rng_call_transition(
    receipt: &JavaLockstepReceiptV1,
    successor: &JavaRuntimeObservationV1,
) -> Result<NaturalRunRngCallLedgerV1, AdapterFailureV1> {
    let raw_witness = &successor.frame["rng_call_witness"];
    if raw_witness.is_null() {
        return Err(failure(
            AdapterFailureKindV1::Evidence,
            "Java action omitted its typed RNG-call witness",
        ));
    }
    let witness =
        serde_json::from_value::<RngCallWitness>(raw_witness.clone()).map_err(|error| {
            failure(
                AdapterFailureKindV1::Evidence,
                format!("Java RNG-call witness is malformed: {error}"),
            )
        })?;
    if witness.kind != "rng_call_witness_v1"
        || witness.schema != NATURAL_RUN_RNG_CALL_LEDGER_SCHEMA_V1
        || witness.from_frame_id != receipt.based_on_frame_id
        || witness.to_frame_id != successor.frame_id
        || witness.request_id != receipt.request_id
        || witness.action_id != receipt.action_id
    {
        return Err(failure(
            AdapterFailureKindV1::Evidence,
            "Java RNG-call witness does not bind the executed action and adjacent frames",
        ));
    }
    if !witness.complete || witness.unsupported_reason.is_some() {
        return Err(failure(
            AdapterFailureKindV1::Evidence,
            format!(
                "Java RNG-call witness is incomplete{}",
                witness
                    .unsupported_reason
                    .as_deref()
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default()
            ),
        ));
    }
    NaturalRunRngCallLedgerV1::new(witness.calls).map_err(|message| {
        failure(
            AdapterFailureKindV1::Evidence,
            format!("Java RNG-call witness violates the shared contract: {message}"),
        )
    })
}

fn aggregate_rng_calls(
    mut calls: Vec<NaturalRunRngCallV1>,
) -> Result<NaturalRunRngCallLedgerV1, AdapterFailureV1> {
    let mut next_ordinals = BTreeMap::<String, u32>::new();
    for call in &mut calls {
        let ordinal = next_ordinals.entry(call.owner.clone()).or_default();
        call.owner_ordinal = *ordinal;
        *ordinal = ordinal.checked_add(1).ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Evidence,
                "Java semantic RNG-call ordinal overflowed",
            )
        })?;
    }
    NaturalRunRngCallLedgerV1::new(calls).map_err(|message| {
        failure(
            AdapterFailureKindV1::Evidence,
            format!("Java semantic RNG-call ledger is invalid: {message}"),
        )
    })
}

fn unique_current_action(
    actions: &[ResolvedJavaActionV1],
    predicate: impl Fn(&ResolvedJavaActionV1) -> bool,
    message: &str,
) -> Result<ResolvedJavaActionV1, AdapterFailureV1> {
    let matching = actions
        .iter()
        .filter(|candidate| predicate(candidate))
        .cloned()
        .collect::<Vec<_>>();
    let [action] = matching.as_slice() else {
        return Err(failure(AdapterFailureKindV1::Evidence, message));
    };
    Ok(action.clone())
}

fn is_campfire_option(action: &SemanticActionV1, id: &str) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Campfire,
            choice,
        } if choice.kind == "campfire_option" && choice.id == id && choice.occurrence == 0
    )
}

fn is_campfire_deck_card(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Campfire,
            choice,
        } if choice.kind == "deck_card" && choice.occurrence == 0
    )
}

fn is_neow_introduction(snapshot: &ParitySnapshotV1) -> bool {
    let Some(room) = snapshot.domains.get(&ParityDomainV1::Room) else {
        return false;
    };
    matches!(
        &room.evidence,
        DomainEvidenceV1::Observed { value, .. }
            if value["kind"].as_str() == Some("neow")
                && value["stage"].as_str() == Some("introduction")
    )
}

fn failure(kind: AdapterFailureKindV1, message: impl Into<String>) -> AdapterFailureV1 {
    AdapterFailureV1::new(kind, message)
}

fn distilled_chaos_hand_select_profile(
    predecessor: &SemanticActionV1,
    prior: Option<&JavaRuntimeObservationV1>,
    successor: &JavaRuntimeObservationV1,
    bindings: &BTreeMap<String, String>,
) -> Result<Option<JavaHandSelectProfileV1>, AdapterFailureV1> {
    if !matches!(predecessor, SemanticActionV1::UsePotion { potion_id, target: None, .. }
        if potion_id == "distilledchaos")
    {
        return Ok(None);
    }
    // The potion is the player action, while cardInUse identifies the original
    // auto-played card suspended at its selection. Bind that actual object to a
    // previously observed draw/discard card; localized selection text is not proof.
    let in_use = &successor.frame["parity_internal"]["zones"]["card_in_use"];
    let uuid = in_use["instance_uuid"].as_str().ok_or_else(|| {
        failure(
            AdapterFailureKindV1::Evidence,
            "Distilled Chaos selection omitted cardInUse",
        )
    })?;
    let prior = prior.ok_or_else(|| {
        failure(
            AdapterFailureKindV1::Evidence,
            "Distilled Chaos selection omitted its predecessor",
        )
    })?;
    let matches = ["draw", "discard"]
        .into_iter()
        .flat_map(|zone| {
            prior.frame["parity_internal"]["zones"][zone]["cards"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter(|card| card["instance_uuid"].as_str() == Some(uuid))
        .collect::<Vec<_>>();
    let [source] = matches.as_slice() else {
        return Err(failure(
            AdapterFailureKindV1::Evidence,
            "Distilled Chaos cardInUse has no unique prior draw/discard source",
        ));
    };
    let card_id = in_use["card_id"].as_str().map(canonical_game_id);
    let upgrades = in_use["upgrades"].as_u64();
    if !bindings.contains_key(uuid)
        || card_id.is_none()
        || upgrades.is_none()
        || source["card_id"].as_str().map(canonical_game_id) != card_id
        || source["upgrades"].as_u64() != upgrades
    {
        return Err(failure(
            AdapterFailureKindV1::Evidence,
            "Distilled Chaos cardInUse disagrees with its source-bound card",
        ));
    }
    Ok(match (card_id.as_deref(), upgrades) {
        (Some("truegrit"), Some(1..)) => Some(JavaHandSelectProfileV1::SingleExhaust),
        (Some("burningpact"), Some(_)) => Some(JavaHandSelectProfileV1::SingleExhaust),
        (Some("purity"), Some(upgrades)) => Some(JavaHandSelectProfileV1::OptionalExhaust {
            max_cards: if upgrades == 0 { 3 } else { 5 },
        }),
        (Some("warcry" | "thinkingahead"), Some(_)) => {
            Some(JavaHandSelectProfileV1::SinglePutOnDrawPile)
        }
        (Some("setup"), Some(_)) => {
            require_setup_free_once_capture(successor)?;
            Some(JavaHandSelectProfileV1::SinglePutOnDrawPile)
        }
        (Some("armaments"), Some(0)) => Some(JavaHandSelectProfileV1::SingleUpgrade),
        (Some("dualwield"), Some(_)) => Some(JavaHandSelectProfileV1::SingleCopy),
        _ => None,
    })
}

fn require_setup_free_once_capture(
    observation: &JavaRuntimeObservationV1,
) -> Result<(), AdapterFailureV1> {
    let zones = &observation.frame["parity_internal"]["zones"];
    for name in ["hand", "draw", "discard", "exhaust", "limbo"] {
        let cards = zones[name]["cards"].as_array().ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Evidence,
                "Setup omitted a captured card zone",
            )
        })?;
        if cards
            .iter()
            .any(|card| card["free_to_play_once"].as_bool().is_none())
        {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "Setup requires observed free-to-play-once flags on every card",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recorded_thinking_ahead_authorizes_topdeck_only_for_its_source_bound_predecessor() {
        use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
        let before = observation_from("observed_thinking_ahead", "before.json");
        let after = observation_from("observed_thinking_ahead", "after.json");
        let action: crate::RecordedJavaActionV1 =
            serde_json::from_value(load_from("observed_thinking_ahead", "action.json")).unwrap();
        assert!(validate_adjacent(&before, &action, &after));
        let bootstrap = serde_json::from_value(
            load_from("observed_thinking_ahead", "manifest.json")["bootstrap"].clone(),
        )
        .unwrap();
        // Authorization only: no transport method is called and no JVM is started.
        let transport =
            crate::LiveJavaLockstepTransportV1::new("unused", std::time::Duration::from_millis(1));
        let mut adapter = JavaLockstepAdapterV1::new(transport, bootstrap).unwrap();
        let SemanticActionV1::PlayCard { card_binding, .. } = &action.action else {
            panic!()
        };
        let uuid = action
            .receipt
            .action_id
            .strip_prefix("play_card:")
            .unwrap()
            .strip_suffix(":none")
            .unwrap();
        adapter
            .current_runtime_card_bindings
            .insert(uuid.into(), card_binding.clone());
        adapter.current_observation = Some(before);
        adapter
            .update_hand_select_authorization(&action.action, &after)
            .unwrap();
        assert_eq!(
            adapter.hand_select_profile,
            Some(JavaHandSelectProfileV1::SinglePutOnDrawPile)
        );
        let mut unbound = action.action;
        let SemanticActionV1::PlayCard { card_binding, .. } = &mut unbound else {
            panic!()
        };
        *card_binding = "unwitnessed/other-card".into();
        assert!(adapter
            .update_hand_select_authorization(&unbound, &after)
            .is_err());
    }

    #[test]
    fn setup_requires_sampled_free_once_flags_before_authorizing_its_selection() {
        let mut observed = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "fixture".into(),
                version: "1".into(),
                session_epoch: "test".into(),
            },
            frame_id: 1,
            frame: json!({"parity_internal":{"zones":{}}}),
            actions: vec![],
        };
        for zone in ["hand", "draw", "discard", "exhaust", "limbo"] {
            observed.frame["parity_internal"]["zones"][zone] =
                json!({"cards":[{"free_to_play_once":false}]});
        }
        require_setup_free_once_capture(&observed).unwrap();
        for zone in ["hand", "draw", "discard", "exhaust", "limbo"] {
            let mut missing = observed.clone();
            missing.frame["parity_internal"]["zones"][zone]["cards"][0]
                .as_object_mut()
                .unwrap()
                .remove("free_to_play_once");
            assert!(require_setup_free_once_capture(&missing).is_err());
        }
    }

    #[test]
    fn recorded_codex_product_retires_at_the_real_double_boss_completion() {
        use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
        let before = observation_from("observed_codex", "boss-before.json");
        let after = observation_from("observed_codex", "boss-after.json");
        let action: crate::RecordedJavaActionV1 =
            serde_json::from_value(load_from("observed_codex", "boss-action.json")).unwrap();
        assert!(validate_adjacent(&before, &action, &after));
        assert_eq!(
            crate::projection::boundary_kind(&after).unwrap(),
            "natural_double_boss_complete"
        );
        assert!(matches!(
            action.action,
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::CombatCodex,
                ..
            }
        ));
        let witness: CardBirthWitness =
            serde_json::from_value(after.frame["card_birth_witness"].clone()).unwrap();
        assert!(witness.complete);
        assert_eq!(witness.products.len(), 1);
        let product = &witness.products[0];
        assert_eq!(product.card_id, "Quick Slash");
        assert_eq!(
            product.first_zone,
            Some(sts_java_parity_protocol::CardBirthZone::Draw)
        );
        validate_java_birth_product(&after, product).unwrap();
        for phase in [json!("COMBAT"), serde_json::Value::Null] {
            let mut invalid = after.clone();
            invalid.frame["boundary"]["evidence"]["room_phase"] = phase;
            assert!(validate_java_birth_product(&invalid, product).is_err());
        }
        let mut continuing = after;
        continuing.frame["boundary"]["kind"] = json!("combat_player_decision");
        assert!(validate_java_birth_product(&continuing, product).is_err());
    }

    #[test]
    fn combat_birth_piles_require_a_completed_combat_boundary() {
        let product = CardBirthProduct {
            generation_index: 0,
            java_uuid: "new-dazed".to_owned(),
            card_id: "Dazed".to_owned(),
            upgrades: 0,
            first_zone: Some(sts_java_parity_protocol::CardBirthZone::Discard),
            autoplay_copy: None,
        };
        let reward = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "fixture".to_owned(),
                version: "1".to_owned(),
                session_epoch: "test".to_owned(),
            },
            frame_id: 2,
            frame: json!({
                "boundary": {"kind": "run_decision", "evidence": {"room_phase": "COMPLETE"}},
                "public_base": {
                    "decision": {"kind": "combat_reward"},
                    "map": {"current_x": 0, "current_y": 12,
                        "nodes": [{"x": 0, "y": 12, "room_type": "MonsterRoomElite"}]}
                },
                "parity_internal": {}
            }),
            actions: vec![],
        };
        assert!(validate_java_birth_product(&reward, &product).is_ok());
        for phase in [json!("COMBAT"), serde_json::Value::Null] {
            let mut invalid = reward.clone();
            invalid.frame["boundary"]["evidence"]["room_phase"] = phase;
            assert!(validate_java_birth_product(&invalid, &product).is_err());
        }
        let mut continuing = reward.clone();
        continuing.frame["boundary"]["kind"] = json!("combat_player_decision");
        assert!(validate_java_birth_product(&continuing, &product).is_err());
        let mut treasure = reward.clone();
        treasure.frame["public_base"]["map"]["nodes"][0]["room_type"] = json!("TreasureRoom");
        assert!(validate_java_birth_product(&treasure, &product).is_err());
        let mut partial = reward.clone();
        partial.frame["parity_internal"]["zones"] = json!({"draw": {"cards": []}});
        assert!(validate_java_birth_product(&partial, &product).is_err());
    }
}
