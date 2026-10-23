use super::*;
use sts_oracle_learning::{
    content::{
        cards::CardId,
        monsters::EnemyId,
        potions::{Potion, PotionId},
    },
    runtime::combat::CombatCard,
    sim::combat::EngineCombatStepper,
    state::core::{ClientInput, EngineState},
    test_support::{blank_test_combat, planned_monster},
};

fn selection_draft() -> AgentCombatDraftV1 {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 11),
        CombatCard::for_scenario(CardId::Wound, 22),
        CombatCard::for_scenario(CardId::Wound, 33),
    ];
    combat.entities.potions = vec![Some(Potion::new(PotionId::GamblersBrew, 1))];
    let prepared = sts_combat_planner::materialize_exact_action_line(
        &EngineCombatStepper,
        &CombatPosition::new(EngineState::CombatPlayerTurn, combat),
        &[ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        }],
        250,
    )
    .unwrap();
    AgentCombatDraftV1::new(
        7,
        prepared.final_position,
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 250,
            allow_potion_expenditure: false,
            allow_potion_discard: false,
            ..Default::default()
        },
    )
    .unwrap()
}

fn runtime() -> AgentCombatDraftRuntimeV1 {
    AgentCombatDraftRuntimeV1 {
        draft: Some(selection_draft()),
        ..Default::default()
    }
}

#[test]
fn draft_selection_language_keeps_sources_preferences_options_and_work_unchanged() {
    use sts_presentation::Locale;
    let mut runtime = runtime();
    runtime
        .draft
        .as_mut()
        .unwrap()
        .advance(sts_combat_planner::CombatPlanningQuantum::deterministic(
            1000, 65536,
        ))
        .unwrap();
    let counters = runtime.draft.as_ref().unwrap().consumed();
    let english = runtime
        .selection_at_verified_root(None, None, None, Locale::En)
        .unwrap();
    let chinese = runtime
        .selection_at_verified_root(None, None, None, Locale::ZhCn)
        .unwrap();
    assert_eq!(english.frame_ref, chinese.frame_ref);
    assert_eq!(english.source_ref, chinese.source_ref);
    assert_eq!(english.root_head_revision, chinese.root_head_revision);
    assert_eq!(english.next_offset, chinese.next_offset);
    assert_eq!(english.reading.preferences, chinese.reading.preferences);
    assert_eq!(english.reading.total, chinese.reading.total);
    assert_eq!(english.reading.offset, chinese.reading.offset);
    assert_eq!(
        english.reading.remaining_work,
        chinese.reading.remaining_work
    );
    let mut normalized = chinese.reading.clone();
    normalized.view = english.reading.view.clone();
    for (original, translated) in english
        .reading
        .families
        .iter()
        .zip(&mut normalized.families)
    {
        translated.label = original.label.clone();
        for (member, target) in original.members.iter().zip(&mut translated.members) {
            target.label = member.label.clone();
        }
    }
    for (original, translated) in english
        .reading
        .outcomes
        .iter()
        .zip(&mut normalized.outcomes)
    {
        translated.option.label = original.option.label.clone();
    }
    for (original, translated) in english.reading.sources.iter().zip(&mut normalized.sources) {
        translated.label = original.label.clone();
    }
    assert_eq!(normalized, english.reading);
    assert!(chinese.reading.view.starts_with("草稿选牌\n"));
    assert!(english.reading.view.contains("Wound") && chinese.reading.view.contains("伤口"));
    assert_eq!(runtime.draft.as_ref().unwrap().consumed(), counters);
    let en_index = runtime
        .selection_at_verified_root(None, None, Some(0), Locale::En)
        .unwrap();
    let zh_index = runtime
        .selection_at_verified_root(None, None, Some(0), Locale::ZhCn)
        .unwrap();
    assert_eq!(
        en_index.reading.sources.len(),
        zh_index.reading.sources.len()
    );
    for (en, zh) in en_index
        .reading
        .sources
        .iter()
        .zip(&zh_index.reading.sources)
    {
        assert_eq!(en.source_ref, zh.source_ref);
        assert_eq!(en.action_count, zh.action_count);
        assert_ne!(en.label, zh.label);
    }
    assert_eq!(runtime.draft.as_ref().unwrap().consumed(), counters);
}

#[test]
fn draft_validation_languages_keep_rejections_and_limits_independent() {
    use sts_presentation::Locale;
    for locale in [Locale::En, Locale::ZhCn] {
        assert!(validate_quantum(MAX_GENERATION_WORK, MAX_ENGINE_STEPS, locale).is_ok());
        assert!(validate_quantum(MAX_GENERATION_WORK + 1, 0, locale).is_err());
        assert!(validate_quantum(0, MAX_ENGINE_STEPS + 1, locale).is_err());
        assert!(validate_option_ref("T0", locale).is_ok());
        assert!(validate_option_ref("", locale).is_err());
        for source in ["S0", "S9", "S10"] {
            assert!(validate_selection_request(None, Some(source), None, locale).is_ok());
        }
        for source in ["", "A0", "S00", "S-1", "S0 "] {
            assert!(validate_selection_request(None, Some(source), None, locale).is_err());
        }
        let edit = AgentCombatDraftSelectionEditV1 {
            source_ref: "S0".into(),
            preferences: vec![],
        };
        assert!(validate_selection_request(Some(&edit), Some("S0"), None, locale).is_err());
        assert_eq!(edit.source_ref, "S0");
    }
    for (english, chinese) in [
        (
            validate_quantum(MAX_GENERATION_WORK + 1, 0, Locale::En),
            validate_quantum(MAX_GENERATION_WORK + 1, 0, Locale::ZhCn),
        ),
        (
            validate_option_ref("", Locale::En),
            validate_option_ref("", Locale::ZhCn),
        ),
        (
            validate_selection_request(None, Some("A0"), None, Locale::En),
            validate_selection_request(None, Some("A0"), None, Locale::ZhCn),
        ),
    ] {
        assert_ne!(english.unwrap_err(), chinese.unwrap_err());
    }
}

#[test]
fn selection_transport_retains_progress_and_exposes_only_public_occurrences() {
    let mut runtime = runtime();
    let observed = runtime
        .selection_at_verified_root(None, None, None, sts_presentation::Locale::En)
        .unwrap();
    let frame = runtime.draft.as_ref().unwrap().frame_ref().unwrap();
    assert_eq!(observed.frame_ref, frame);
    assert_eq!(serde_json::to_value(&observed).unwrap()["frame_ref"], frame);
    let edit = AgentCombatDraftSelectionEditV1 {
        source_ref: observed.source_ref.clone().unwrap(),
        preferences: vec![CombatDraftSelectionMemberV1 {
            family: 0,
            choices: vec![1, 2],
        }],
    };
    edit.validate(sts_presentation::Locale::En).unwrap();
    let response = runtime
        .selection_at_verified_root(Some(&edit), None, None, sts_presentation::Locale::En)
        .unwrap();
    assert_eq!(
        runtime.draft.as_ref().unwrap().consumed(),
        CombatPlanningCounters::default()
    );
    assert_eq!(response.source_ref, observed.source_ref);
    assert_eq!(
        runtime
            .draft
            .as_ref()
            .unwrap()
            .selection_view()
            .unwrap()
            .preferences,
        edit.preferences
    );
    let serialized = serde_json::to_string(&response).unwrap();
    assert!(!serialized.contains("constructed_scenario"));
    assert!(!serialized.contains("SubmitSelection"));
    assert!(!serialized.contains("exact_state_hash"));
    println!(
        "Selection {}\n{}",
        response.source_ref.as_ref().unwrap(),
        response.reading.view
    );
    runtime
        .draft
        .as_mut()
        .unwrap()
        .advance(CombatPlanningQuantum::deterministic(1_000, 65_536))
        .unwrap();
    let before = runtime.draft.as_ref().unwrap().consumed();
    let view = runtime.draft.as_ref().unwrap().env.observe().unwrap();
    let detail = runtime
        .draft
        .as_ref()
        .unwrap()
        .env
        .inspect_handle(view.options[0].handle)
        .unwrap();
    let clear = AgentCombatDraftSelectionEditV1 {
        source_ref: edit.source_ref,
        preferences: vec![],
    };
    runtime
        .selection_at_verified_root(Some(&clear), None, None, sts_presentation::Locale::En)
        .unwrap();
    assert!(runtime
        .draft
        .as_ref()
        .unwrap()
        .selection_view()
        .unwrap()
        .preferences
        .is_empty());
    assert_eq!(runtime.draft.as_ref().unwrap().consumed(), before);
    assert_eq!(runtime.draft.as_ref().unwrap().env.observe().unwrap(), view);
    assert_eq!(
        runtime
            .draft
            .as_ref()
            .unwrap()
            .env
            .inspect_handle(view.options[0].handle)
            .unwrap(),
        detail
    );
}

#[test]
fn selection_transport_rejects_stale_sources_and_preserves_advice_on_invalid_edit() {
    let mut runtime = runtime();
    let source_ref = runtime
        .selection_at_verified_root(None, None, None, sts_presentation::Locale::En)
        .unwrap()
        .source_ref
        .unwrap();
    let edit = AgentCombatDraftSelectionEditV1 {
        source_ref: source_ref.clone(),
        preferences: vec![CombatDraftSelectionMemberV1 {
            family: 0,
            choices: vec![1],
        }],
    };
    runtime
        .selection_at_verified_root(Some(&edit), None, None, sts_presentation::Locale::En)
        .unwrap();
    let invalid = AgentCombatDraftSelectionEditV1 {
        source_ref: source_ref.clone(),
        preferences: vec![
            CombatDraftSelectionMemberV1 {
                family: 0,
                choices: vec![2],
            },
            CombatDraftSelectionMemberV1 {
                family: 0,
                choices: vec![99],
            },
        ],
    };
    assert!(runtime
        .selection_at_verified_root(Some(&invalid), None, None, sts_presentation::Locale::En)
        .is_err());
    assert_eq!(
        runtime
            .draft
            .as_ref()
            .unwrap()
            .selection_view()
            .unwrap()
            .preferences,
        edit.preferences
    );
    runtime
        .draft
        .as_mut()
        .unwrap()
        .advance(CombatPlanningQuantum::deterministic(1_000, 65_536))
        .unwrap();
    let option = runtime
        .draft
        .as_ref()
        .unwrap()
        .env
        .observe()
        .unwrap()
        .options[0]
        .handle
        .to_string();
    runtime.draft.as_mut().unwrap().choose(&option).unwrap();
    // Even if a caller bypassed transport invalidation, exact source binding
    // refuses ordinals from the parent's selection page.
    assert!(runtime
        .selection_at_verified_root(Some(&edit), None, None, sts_presentation::Locale::En)
        .is_err());
    runtime.draft.as_mut().unwrap().undo().unwrap();
    assert_eq!(
        runtime
            .draft
            .as_ref()
            .unwrap()
            .selection_view()
            .unwrap()
            .preferences,
        edit.preferences
    );
    runtime.clear();
    runtime.draft = Some(selection_draft());
    let new_source = runtime
        .selection_at_verified_root(None, None, None, sts_presentation::Locale::En)
        .unwrap()
        .source_ref
        .unwrap();
    assert_ne!(new_source, source_ref);
    assert!(runtime
        .selection_at_verified_root(Some(&edit), None, None, sts_presentation::Locale::En)
        .is_err());
    assert!(runtime
        .draft
        .as_ref()
        .unwrap()
        .selection_view()
        .unwrap()
        .preferences
        .is_empty());
}

#[test]
fn selection_transport_requires_observation_before_mutation() {
    let mut runtime = runtime();
    let edit = AgentCombatDraftSelectionEditV1 {
        source_ref: "S0".to_owned(),
        preferences: vec![],
    };
    assert!(runtime
        .selection_at_verified_root(Some(&edit), None, None, sts_presentation::Locale::En)
        .is_err());
    assert_eq!(
        runtime.draft.as_ref().unwrap().consumed(),
        CombatPlanningCounters::default()
    );
}

fn nested_draft(potion: bool) -> AgentCombatDraftV1 {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
    combat.turn.energy = 3;
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 11),
        CombatCard::for_scenario(CardId::Wound, 22),
        CombatCard::for_scenario(CardId::Wound, 33),
    ];
    combat.zones.draw_pile = vec![
        CombatCard::for_scenario(CardId::Strike, 55),
        CombatCard::for_scenario(CardId::Defend, 66),
    ]
    .into();
    if potion {
        combat.entities.potions = vec![Some(Potion::new(PotionId::GamblersBrew, 1))];
    } else {
        combat
            .zones
            .hand
            .push(CombatCard::for_scenario(CardId::BurningPact, 44));
    }
    AgentCombatDraftV1::new(
        7,
        CombatPosition::new(EngineState::CombatPlayerTurn, combat),
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 250,
            allow_potion_expenditure: potion,
            allow_potion_discard: false,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn nested_burning_pact_and_gamblers_brew_are_inspected_and_edited_in_the_same_draft() {
    for potion in [false, true] {
        let mut runtime = AgentCombatDraftRuntimeV1 {
            draft: Some(nested_draft(potion)),
            ..Default::default()
        };
        let mut found = false;
        for _ in 0..32 {
            runtime
                .draft
                .as_mut()
                .unwrap()
                .advance(CombatPlanningQuantum::deterministic(1, 500))
                .unwrap();
            let page = runtime
                .draft
                .as_ref()
                .unwrap()
                .env
                .selection_sources(0, 16)
                .unwrap();
            if page.sources.iter().any(|s| s.action_count() == 1) {
                found = true;
                break;
            }
        }
        assert!(found);
        let used = runtime.draft.as_ref().unwrap().consumed();
        let before = runtime.draft.as_ref().unwrap().env.observe().unwrap();
        let index = runtime
            .selection_at_verified_root(None, None, None, sts_presentation::Locale::En)
            .unwrap();
        println!("{}", index.reading.view);
        assert!(index.source_ref.is_none());
        assert!(index.reading.view.contains(if potion {
            "Gambler's Brew"
        } else {
            "Burning Pact"
        }));
        let source_ref = &index.reading.sources[0].source_ref;
        assert_eq!(index.reading.sources[0].action_count, 1);
        let detail = runtime
            .selection_at_verified_root(None, Some(source_ref), None, sts_presentation::Locale::En)
            .unwrap();
        assert!(detail
            .reading
            .view
            .contains(if potion { "Use P0" } else { "Play H3" }));
        let members = &detail.reading.families[0].members;
        assert_eq!(
            members
                .iter()
                .map(|m| (m.ordinal, m.label.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "Defend"), (1, "Wound"), (2, "Wound")]
        );
        let edit = AgentCombatDraftSelectionEditV1 {
            source_ref: source_ref.to_owned(),
            preferences: vec![CombatDraftSelectionMemberV1 {
                family: 0,
                choices: if potion { vec![1, 2] } else { vec![2] },
            }],
        };
        let edited = runtime
            .selection_at_verified_root(Some(&edit), None, None, sts_presentation::Locale::En)
            .unwrap();
        println!(
            "Selection {}\n{}",
            edited.source_ref.as_ref().unwrap(),
            edited.reading.view
        );
        assert!(edited.reading.view.contains(if potion {
            "Preferred: Wound (1), Wound (2)"
        } else {
            "Preferred: Wound (2)"
        }));
        assert_eq!(runtime.draft.as_ref().unwrap().consumed(), used);
        assert_eq!(
            runtime.draft.as_ref().unwrap().env.observe().unwrap(),
            before
        );
        let source = runtime
            .selection_binding
            .resolve(source_ref, sts_presentation::Locale::En)
            .unwrap();
        runtime
            .draft
            .as_mut()
            .unwrap()
            .advance(CombatPlanningQuantum::deterministic(1000, 250000))
            .unwrap();
        let used = runtime.draft.as_ref().unwrap().consumed();
        let retained = runtime.draft.as_ref().unwrap().env.observe().unwrap();
        let outcomes = runtime
            .draft
            .as_ref()
            .unwrap()
            .env
            .selection_outcomes(&source, 0)
            .unwrap();
        assert!(outcomes.total > 0);
        let mut offset = 0;
        let mut seen = std::collections::HashSet::new();
        loop {
            let view = runtime
                .draft
                .as_ref()
                .unwrap()
                .env
                .selection_outcomes(&source, offset)
                .unwrap();
            let response = runtime
                .selection_at_verified_root(
                    None,
                    Some(source_ref),
                    Some(offset),
                    sts_presentation::Locale::En,
                )
                .unwrap();
            assert_eq!(response.next_offset, view.next_offset);
            assert!(response
                .reading
                .view
                .contains(&format!("Outcomes: {}", view.total)));
            assert!(!response.reading.view.contains("  0: Play"));
            assert_eq!(response.reading.outcomes.len(), view.outcomes.len());
            for (entry, exact) in response.reading.outcomes.iter().zip(&view.outcomes) {
                assert_eq!(entry.option.option_ref, exact.handle.to_string());
                assert_eq!(entry.selection, exact.selection);
                assert_eq!(entry.preferred, exact.preferred);
                assert_eq!(entry.option.successor, exact.successor);
                assert_eq!(entry.option.action_count, exact.action_count);
                let ordinary = retained
                    .options
                    .iter()
                    .find(|o| o.handle == exact.handle)
                    .unwrap();
                assert_eq!(entry.option.successor, ordinary.successor);
            }
            for outcome in &view.outcomes {
                assert!(seen.insert(outcome.handle));
                assert_eq!(
                    outcome.preferred,
                    edit.preferences.contains(&outcome.selection)
                );
                let exact = runtime
                    .draft
                    .as_ref()
                    .unwrap()
                    .env
                    .inspect_handle(outcome.handle)
                    .unwrap();
                assert_eq!(outcome.successor.hp, exact.successor.player.hp);
                assert_eq!(
                    outcome.successor.potions,
                    exact
                        .successor
                        .potions
                        .iter()
                        .map(|p| p.as_ref().map(|p| p.potion_id))
                        .collect::<Vec<_>>()
                );
                assert_eq!(outcome.action_count, exact.actions.len());
            }
            let Some(next) = view.next_offset else { break };
            assert!(next > offset);
            offset = next;
        }
        assert_eq!(seen.len(), outcomes.total);
        assert_eq!(runtime.draft.as_ref().unwrap().consumed(), used);
        let invalid = AgentCombatDraftSelectionEditV1 {
            source_ref: source_ref.to_owned(),
            preferences: vec![CombatDraftSelectionMemberV1 {
                family: 0,
                choices: vec![99],
            }],
        };
        assert!(runtime
            .selection_at_verified_root(Some(&invalid), None, None, sts_presentation::Locale::En)
            .is_err());
        assert_eq!(
            runtime
                .draft
                .as_ref()
                .unwrap()
                .env
                .selection_view_at(&source)
                .unwrap()
                .preferences,
            edit.preferences
        );
        let clear = AgentCombatDraftSelectionEditV1 {
            source_ref: source_ref.to_owned(),
            preferences: vec![],
        };
        let cleared = runtime
            .selection_at_verified_root(Some(&clear), None, None, sts_presentation::Locale::En)
            .unwrap();
        assert_eq!(cleared.reading.remaining_work, Some(0));
        let after_clear = runtime
            .draft
            .as_ref()
            .unwrap()
            .env
            .selection_outcomes(&source, 0)
            .unwrap();
        assert_eq!(after_clear.total, outcomes.total);
        assert!(after_clear.outcomes.iter().all(|o| !o.preferred));
        let mut unseen = seen.clone();
        for offset in (0..after_clear.total).step_by(8) {
            let page = runtime
                .draft
                .as_ref()
                .unwrap()
                .env
                .selection_outcomes(&source, offset)
                .unwrap();
            for outcome in page.outcomes {
                assert!(unseen.remove(&outcome.handle));
            }
        }
        assert!(unseen.is_empty());
        assert_eq!(runtime.draft.as_ref().unwrap().consumed(), used);
        assert_eq!(
            runtime.draft.as_ref().unwrap().env.observe().unwrap(),
            retained
        );
        for option in &retained.options {
            runtime
                .draft
                .as_ref()
                .unwrap()
                .env
                .inspect_handle(option.handle)
                .unwrap();
        }
        runtime
            .draft
            .as_mut()
            .unwrap()
            .env
            .choose_handle(retained.options[0].handle)
            .unwrap();
        assert!(runtime
            .selection_at_verified_root(Some(&edit), None, None, sts_presentation::Locale::En)
            .is_err());
        runtime.draft.as_mut().unwrap().env.undo().unwrap();
        let mut restored = runtime.draft.as_ref().unwrap().env.observe().unwrap();
        let explored = restored
            .options
            .iter_mut()
            .find(|o| o.handle == retained.options[0].handle)
            .unwrap();
        assert!(explored.continuation.take().is_some());
        assert_eq!(restored, retained);
    }
}

#[test]
fn late_zero_card_preference_finds_retained_cancel_without_rewriting_or_generation() {
    let mut runtime = AgentCombatDraftRuntimeV1 {
        draft: Some(nested_draft(true)),
        ..Default::default()
    };
    let source_ref = (0..32)
        .find_map(|_| {
            runtime
                .draft
                .as_mut()
                .unwrap()
                .advance(CombatPlanningQuantum::deterministic(1, 500))
                .unwrap();
            let index = runtime
                .selection_at_verified_root(None, None, None, sts_presentation::Locale::En)
                .unwrap();
            index
                .reading
                .sources
                .iter()
                .find(|source| source.action_count == 1)
                .map(|source| source.source_ref.clone())
        })
        .expect("nested Brew source");
    runtime
        .draft
        .as_mut()
        .unwrap()
        .advance(CombatPlanningQuantum::deterministic(2000, 250000))
        .unwrap();
    let before = runtime.draft.as_ref().unwrap().env.observe().unwrap();
    let counters = runtime.draft.as_ref().unwrap().consumed();
    let response = runtime
        .selection_at_verified_root(
            Some(&AgentCombatDraftSelectionEditV1 {
                source_ref: source_ref.clone(),
                preferences: vec![CombatDraftSelectionMemberV1 {
                    family: 0,
                    choices: vec![],
                }],
            }),
            None,
            None,
            sts_presentation::Locale::En,
        )
        .unwrap();
    let alias = response
        .reading
        .outcomes
        .first()
        .expect("preferred result on first page");
    assert!(alias.preferred);
    assert!(alias.selection.choices.is_empty());
    assert!(alias.equivalence.is_some());
    assert!(alias.option.label.contains("0 cards via Cancel"));
    let option_ref = alias.option.option_ref.clone();
    let retained = runtime
        .draft
        .as_ref()
        .unwrap()
        .env
        .inspect(&option_ref)
        .unwrap();
    assert!(matches!(
        &retained.actions[1],
        sts_complete_turn_view::CompleteTurnSelectedActionV1::Atomic {
            action:
                sts_oracle_learning::agent::information::action::PublicCombatAtomicActionV1::Cancel,
            ..
        }
    ));
    assert_eq!(
        runtime.draft.as_ref().unwrap().env.observe().unwrap(),
        before
    );
    assert_eq!(runtime.draft.as_ref().unwrap().consumed(), counters);
    let mut offset = 0;
    let mut handles = std::collections::HashSet::new();
    let mut saw_unpreferred = false;
    loop {
        let page = runtime
            .selection_at_verified_root(
                None,
                Some(&source_ref),
                Some(offset),
                sts_presentation::Locale::En,
            )
            .unwrap();
        for outcome in &page.reading.outcomes {
            assert!(handles.insert(outcome.option.option_ref.clone()));
            assert!(!saw_unpreferred || !outcome.preferred);
            saw_unpreferred |= !outcome.preferred;
        }
        let Some(next) = page.next_offset else { break };
        offset = next;
    }
    assert_eq!(handles.len(), response.reading.total);
    runtime.draft.as_mut().unwrap().choose(&option_ref).unwrap();
    assert!(runtime
        .selection_at_verified_root(None, Some(&source_ref), None, sts_presentation::Locale::En)
        .is_err());
    runtime.draft.as_mut().unwrap().undo().unwrap();
    assert_eq!(
        runtime
            .draft
            .as_ref()
            .unwrap()
            .env
            .inspect(&option_ref)
            .unwrap(),
        retained
    );
    assert_eq!(runtime.draft.as_ref().unwrap().consumed(), counters);
}

#[test]
fn selection_request_modes_and_bounded_source_retention() {
    assert!(
        validate_selection_request(None, Some("S0"), Some(0), sts_presentation::Locale::En).is_ok()
    );
    assert!(
        validate_selection_request(None, Some("S00"), None, sts_presentation::Locale::En).is_err()
    );
    let mut binding = SelectionSourceBinding::default();
    let mut first = None;
    for _ in 0..40 {
        let source = selection_draft().selection_source().unwrap();
        let name = binding.observe(source.clone()).unwrap();
        first.get_or_insert(name.clone());
        assert_eq!(binding.observe(source).unwrap(), name);
    }
    assert_eq!(binding.entries.len(), 32);
    assert!(binding
        .resolve(first.as_deref().unwrap(), sts_presentation::Locale::En)
        .is_err());
    assert!(binding.resolve("S39", sts_presentation::Locale::En).is_ok());
}
