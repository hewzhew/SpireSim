//! Pandora creates a batch of new cards, not parent-to-product transforms.
use super::*;
use sts_oracle_run_control::{
    content::{cards::is_starter_basic, relics::RelicId},
    state::selection::DomainEvent,
};
use sts_parity_lockstep::DecisionSurfaceV1;

pub(super) struct PandoraGrantContext {
    room_id: String,
    prior: BTreeSet<CardLineageId>,
    removed: BTreeSet<CardLineageId>,
}

impl PandoraGrantContext {
    pub(super) fn prepare(
        session: &RunControlSession,
        action: &SemanticActionV1,
    ) -> Result<Option<Self>, String> {
        if !matches!(action, SemanticActionV1::Select {
            surface: DecisionSurfaceV1::BossRelic, choice,
        } if choice.id == "pandorasbox")
        {
            return Ok(None);
        }
        // Taking the relic advances to the next Act; retain the producer room.
        let run = &session.run_state;
        Ok(Some(Self {
            room_id: natural_map_node_id_v1(
                i64::from(run.map.current_x),
                i64::from(run.map.current_y),
            )?,
            prior: run.master_deck.iter().map(|c| c.lineage).collect(),
            removed: run
                .master_deck
                .iter()
                .filter(|c| is_starter_basic(c.id))
                .map(|c| c.lineage)
                .collect(),
        }))
    }

    pub(super) fn bind(
        self,
        session: &RunControlSession,
        bindings: &mut BTreeMap<CardLineageId, BoundCardSourceV1>,
    ) -> Result<(), String> {
        let run = &session.run_state;
        let source = DomainEventSource::Relic(RelicId::PandorasBox);
        let removed = run
            .emitted_events
            .iter()
            .filter_map(|e| match e {
                DomainEvent::CardRemoved {
                    card,
                    source: actual,
                } if *actual == source => Some(card),
                _ => None,
            })
            .collect::<Vec<_>>();
        let products = run
            .emitted_events
            .iter()
            .filter_map(|e| match e {
                DomainEvent::CardObtained {
                    card,
                    source: actual,
                } if *actual == source => Some(card),
                _ => None,
            })
            .collect::<Vec<_>>();
        let after = run
            .master_deck
            .iter()
            .map(|c| c.lineage)
            .collect::<BTreeSet<_>>();
        let removed_ids = removed.iter().map(|c| c.lineage).collect::<BTreeSet<_>>();
        let product_ids = products.iter().map(|c| c.lineage).collect::<BTreeSet<_>>();
        let count = self.removed.len();
        if !run.relics.iter().any(|r| r.id == RelicId::PandorasBox)
            || removed.len() != count
            || removed_ids != self.removed
            || removed.iter().any(|c| !is_starter_basic(c.id))
            || self
                .prior
                .difference(&after)
                .copied()
                .collect::<BTreeSet<_>>()
                != self.removed
            || products.len() != count
            || product_ids.len() != count
            || after
                .difference(&self.prior)
                .copied()
                .collect::<BTreeSet<_>>()
                != product_ids
        {
            return Err("Pandora lacks complete unique removal/acquisition witnesses".into());
        }
        let mut additions = Vec::new();
        // Original addToBottom reverses generation order in the confirmation
        // group. CardObtained emissions witness that obtain order; deck order
        // is used only to validate the resulting objects, never to assign IDs.
        for (slot, card) in products.into_iter().rev().enumerate() {
            if !run.master_deck.iter().any(|actual| {
                actual.lineage == card.lineage
                    && actual.id == card.id
                    && actual.upgrades == card.upgrades
            }) {
                return Err("Pandora product disagrees with its acquisition witness".into());
            }
            let source = NaturalRunCardSourceV1::RelicGrant {
                room_id: self.room_id.clone(),
                relic_id: "pandorasbox".into(),
                product_slot: u32::try_from(slot).map_err(|_| "Pandora batch is too large")?,
            };
            let binding = natural_run_card_binding_v1(&source)?;
            if bindings.contains_key(&card.lineage)
                || bindings.values().any(|b| b.binding == binding)
            {
                return Err("Pandora product collides with an existing binding".into());
            }
            additions.push((
                card.lineage,
                BoundCardSourceV1 {
                    binding,
                    source,
                    expected_card_id: sts_java_parity_protocol::canonical_game_id(
                        card.id.stable_id(),
                    ),
                    expected_upgrades: card.upgrades,
                },
            ));
        }
        for lineage in self.removed {
            bindings.remove(&lineage);
        }
        bindings.extend(additions);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_run_control::{
        content::events::neow::NeowEligibilityV1, eval::run_control::RunControlConfig,
        state::rewards::BossRelicChoiceState,
    };
    use sts_parity_lockstep::DomainEvidenceV1;

    fn fixture() -> (
        RustLockstepAdapterV1,
        RustLockstepAdapterV1,
        SemanticActionV1,
    ) {
        let config = RunControlConfig {
            seed: 2026101811,
            player_class: "Silent",
            ascension_level: 20,
            final_act: true,
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        };
        let bootstrap = crate::natural_run_bootstrap_v1(&config).unwrap();
        let mut session = RunControlSession::new(config);
        session.run_state.floor_num = 17;
        session.run_state.map.current_x = 0;
        session.run_state.map.current_y = 15;
        session.run_state.event_state = None;
        session.engine_state =
            EngineState::BossRelicSelect(BossRelicChoiceState::new(vec![RelicId::PandorasBox]));
        let mut before = RustLockstepAdapterV1::new(session, bootstrap, "test", "pandora").unwrap();
        let snapshot = before.observe().unwrap();
        let action = before
            .current_actions
            .iter()
            .find(|a| {
                matches!(&a.semantic,
                    SemanticActionV1::Select { choice, .. } if choice.id == "pandorasbox"
                )
            })
            .unwrap()
            .semantic
            .clone();
        let mut after = before.clone();
        let prepared = after.prepare(&action, &snapshot).unwrap();
        after.execute(prepared).unwrap();
        (before, after, action)
    }

    #[test]
    fn pandora_acquisitions_follow_construction_witnesses_across_act_transition() {
        let (before, mut after, action) = fixture();
        assert_eq!(after.session.run_state.act_num, 2);
        assert_eq!(after.session.run_state.map.current_y, -1);
        let snapshot = after.observe().unwrap();
        assert!(matches!(
            snapshot.domains[&sts_parity_lockstep::ParityDomainV1::MasterDeck].evidence,
            DomainEvidenceV1::Observed { .. }
        ));
        let products = after
            .session
            .run_state
            .emitted_events
            .iter()
            .filter_map(|e| match e {
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Relic(RelicId::PandorasBox),
                } => Some(card),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(products.len(), 10);
        for (slot, card) in products.iter().rev().enumerate() {
            assert_eq!(
                after.bindings[&card.lineage].source,
                NaturalRunCardSourceV1::RelicGrant {
                    room_id: "F16X00".into(),
                    relic_id: "pandorasbox".into(),
                    product_slot: slot as u32,
                }
            );
        }
        let mut reordered = after.session.clone();
        reordered.run_state.master_deck.reverse();
        let mut bindings = before.bindings.clone();
        PandoraGrantContext::prepare(&before.session, &action)
            .unwrap()
            .unwrap()
            .bind(&reordered, &mut bindings)
            .unwrap();
        assert_eq!(
            bindings, after.bindings,
            "current deck order cannot assign a product's source"
        );
    }

    #[test]
    fn pandora_rejects_missing_repeated_and_wrong_owner_witnesses_without_binding_any_product() {
        let (before, after, action) = fixture();
        for case in 0..4 {
            let mut session = after.session.clone();
            let index = session
                .run_state
                .emitted_events
                .iter()
                .position(|e| {
                    matches!(
                        e,
                        DomainEvent::CardObtained {
                            source: DomainEventSource::Relic(RelicId::PandorasBox),
                            ..
                        }
                    )
                })
                .unwrap();
            match case {
                0 => {
                    session.run_state.emitted_events.remove(index);
                }
                1 => {
                    session
                        .run_state
                        .emitted_events
                        .push(session.run_state.emitted_events[index].clone());
                }
                2 => {
                    if let DomainEvent::CardObtained { source, .. } =
                        &mut session.run_state.emitted_events[index]
                    {
                        *source = DomainEventSource::Relic(RelicId::Astrolabe);
                    }
                }
                _ => {
                    if let DomainEvent::CardObtained { card, .. } =
                        &mut session.run_state.emitted_events[index]
                    {
                        card.upgrades += 1;
                    }
                }
            }
            let mut bindings = before.bindings.clone();
            assert!(PandoraGrantContext::prepare(&before.session, &action)
                .unwrap()
                .unwrap()
                .bind(&session, &mut bindings)
                .is_err());
            assert_eq!(bindings, before.bindings);
        }
    }

    #[test]
    fn pandora_without_starters_creates_no_sources() {
        let (mut before, _, action) = fixture();
        before
            .session
            .run_state
            .master_deck
            .retain(|c| !is_starter_basic(c.id));
        let snapshot = before.observe().unwrap();
        let prepared = before.prepare(&action, &snapshot).unwrap();
        let after = before.execute(prepared).unwrap();
        assert!(matches!(
            after.domains[&sts_parity_lockstep::ParityDomainV1::MasterDeck].evidence,
            DomainEvidenceV1::Observed { .. }
        ));
        assert!(!before
            .bindings
            .values()
            .any(|b| matches!(b.source, NaturalRunCardSourceV1::RelicGrant { .. })));
        assert_eq!(before.session.run_state.master_deck.len(), 3);
    }
}
