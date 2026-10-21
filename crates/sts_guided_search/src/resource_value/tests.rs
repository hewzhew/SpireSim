use super::*;
use crate::{
    local::ConstructedCombat,
    solver::{CombatSolver, SolverConfig},
};
use std::time::Duration;
use sts_core::{
    content::monsters::factory::EncounterId, runtime::combat::CombatCard,
    state::map::node::RoomType,
};

fn root() -> CombatPosition {
    ConstructedCombat {
        seed: 3101,
        encounter: EncounterId::TheGuardian,
        act: 1,
        room: RoomType::MonsterRoomBoss,
        hp: 68,
        deck: vec![
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::TwinStrike, 0),
            (CardId::ShrugItOff, 0),
            (CardId::Carnage, 1),
        ],
        relics: vec![],
        potions: vec![],
        max_potion_uses: Some(0),
    }
    .build()
    .unwrap()
}

fn model(s: &CombatSearchState) -> ValueModel {
    let names = feature_names(s);
    let mut weights = vec![0.; names.len()];
    weights[names.iter().position(|n| n == "hp").unwrap()] = 1.;
    weights[names.iter().position(|n| n == "enemy/hp").unwrap()] = -0.5;
    ValueModel {
        schema: SCHEMA,
        objective: OBJECTIVE.into(),
        feature_names: names,
        hand_encoder: None,
        hidden_weights: vec![weights],
        hidden_bias: vec![0.],
        output_weights: vec![1.],
        output_bias: 0.,
        resource_curve: None,
    }
}

#[test]
fn real_pending_effect_exposes_reason_bounds_and_exact_candidate_instances() {
    use sts_core::state::core::ClientInput;
    // Play the real card and inspect the engine's selection, rather than guess
    // its candidates from the names or positions of other hand cards.
    for (id, reason) in [
        (CardId::TrueGrit, "Exhaust"),
        (CardId::Armaments, "Upgrade"),
    ] {
        let mut p = root();
        let mut card = CombatCard::for_scenario(id, 601);
        if id == CardId::TrueGrit {
            card.upgrades = 1;
        }
        let mut upgraded = CombatCard::for_scenario(CardId::Strike, 603);
        upgraded.upgrades = 1;
        p.combat.zones.hand = vec![
            card,
            CombatCard::for_scenario(CardId::Strike, 602),
            upgraded,
            CombatCard::for_scenario(CardId::Defend, 604),
        ];
        let pending = crate::shuffle_order::step(
            &p,
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
        )
        .unwrap();
        validate_position(&pending).unwrap();
        let state = CombatSearchState::new(pending.clone()).unwrap();
        let names = feature_names(&state);
        let x = features(&state);
        let at = |name: &str| x[names.iter().position(|n| n == name).unwrap()];
        assert_eq!(at(&format!("selection/reason/{reason}")), 1.);
        assert_eq!(at("selection/pile/Hand"), 1.);
        assert_eq!(at("selection/min"), 0.1);
        assert_eq!(at("selection/max"), 0.1);
        assert_eq!(at("hand/0/selection_eligible"), 1.);
        let EngineState::PendingChoice(choice) = &pending.engine else {
            panic!("expected pending choice")
        };
        let request = choice.selection_request().unwrap();
        for (i, card) in pending.combat.zones.hand.iter().enumerate() {
            assert_eq!(
                at(&format!("hand/{i}/selection_eligible")),
                f32::from(
                    request
                        .targets
                        .iter()
                        .any(|t| t.card_lineage() == card.lineage)
                )
            );
        }
        assert_eq!(
            at("selection/candidate/Strike/0"),
            if id == CardId::TrueGrit { 0.2 } else { 0.1 }
        );
        assert_eq!(
            at("selection/candidate/Strike/1"),
            if id == CardId::TrueGrit { 0.1 } else { 0. }
        );
        // Isolate the selection contract: unchanged combat, different allowed
        // instance. This is an encoding test, not a claimed natural trajectory.
        let mut restricted = pending;
        let EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages, ..
        }) = &mut restricted.engine
        else {
            panic!("expected hand selection")
        };
        candidate_lineages.retain(|l| *l == p.combat.zones.hand[1].lineage);
        let y = features(&CombatSearchState::new(restricted).unwrap());
        assert_eq!(
            y[names
                .iter()
                .position(|n| n == "selection/candidates")
                .unwrap()],
            0.1
        );
        if id == CardId::TrueGrit {
            assert_ne!(x, y);
        }
    }
}

#[test]
fn real_headbutt_choice_exposes_discard_source_and_rejects_unrepresented_effects() {
    use sts_core::state::core::ClientInput;
    let mut p = root();
    p.combat.zones.hand = vec![CombatCard::for_scenario(CardId::Headbutt, 701)];
    p.combat.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::Defend, 702),
        CombatCard::for_scenario(CardId::Strike, 703),
    ]
    .into();
    let pending = crate::shuffle_order::step(
        &p,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(p.combat.entities.monsters[0].id),
        },
    )
    .unwrap();
    validate_position(&pending).unwrap();
    let state = CombatSearchState::new(pending.clone()).unwrap();
    let names = feature_names(&state);
    let x = features(&state);
    for name in ["selection/reason/MoveToDrawPile", "selection/pile/Discard"] {
        assert_eq!(x[names.iter().position(|n| n == name).unwrap()], 1.);
    }
    assert_eq!(
        x[names
            .iter()
            .position(|n| n == "selection/candidate/Defend/0")
            .unwrap()],
        0.1
    );
    let mut unrepresented = pending;
    let EngineState::PendingChoice(PendingChoice::GridSelect { reason, .. }) =
        &mut unrepresented.engine
    else {
        panic!("expected grid selection")
    };
    *reason = GridSelectReason::DiscardToHand;
    assert!(validate_position(&unrepresented).is_err());
    let mut outdated = model(&state);
    outdated.schema = 3;
    assert!(outdated.validate(&state).is_err());
}

#[test]
fn prediction_trace_keeps_exact_materialized_boundaries_and_terminal_facts() {
    use sts_core::state::core::ClientInput;
    let mut p = root();
    p.combat.entities.monsters[0].current_hp = 1;
    p.combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 991),
        CombatCard::for_scenario(CardId::Strike, 992),
    ];
    let original = p.clone();
    let state = CombatSearchState::new(p.clone()).unwrap();
    let model = model(&state);
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(p.combat.entities.monsters[0].id),
        },
    ];
    let trace = model.prediction_trace(&p, &inputs).unwrap();
    let positions = crate::shuffle_order::replay(&p, &inputs).unwrap();
    assert_eq!(trace.replay_inputs, 2);
    assert_eq!(trace.frames.len(), 3);
    for (i, (frame, position)) in trace.frames.iter().zip(&positions).enumerate() {
        assert_eq!(frame.after_inputs, i);
        assert_eq!(
            frame.features,
            features(&CombatSearchState::new(position.clone()).unwrap())
        );
        assert_eq!(frame.predicted_return.is_some(), i < 2);
        assert_eq!(frame.actual_terminal_return.is_some(), i == 2);
    }
    assert_eq!(
        trace.frames[2].actual_terminal_return,
        Some(positions[2].combat.entities.player.current_hp as f32 / 75.)
    );
    assert_eq!(p, original);
    assert!(model
        .prediction_trace(
            &p,
            &[ClientInput::PlayCard {
                card_index: 99,
                target: None
            }]
        )
        .is_err());
    p.combat.entities.player.current_hp = 0;
    let death = model.prediction_trace(&p, &[]).unwrap();
    assert_eq!(death.frames[0].predicted_return, None);
    assert_eq!(death.frames[0].actual_terminal_return, Some(-1.));
    p.combat.meta.ascension_level = 0;
    assert!(model.prediction_trace(&p, &[]).is_err());
}

#[test]
fn resource_curve_preserves_probability_order_physical_ceiling_and_objective() {
    let state = CombatSearchState::new(root()).unwrap();
    let mut curve = model(&state);
    curve.objective = RESOURCE_CURVE_OBJECTIVE.into();
    curve.hidden_weights[0].fill(0.);
    curve.output_bias = 1.2;
    curve.resource_curve = Some(ResourceCurveHead {
        slope_weights: vec![0.],
        slope_bias: 3_f64.exp_m1().ln() as f32,
    });
    curve.validate(&state).unwrap();
    let mut x = features(&state);
    x[7] = 19. / 75.; // Exact integer ceiling must survive f32 rounding.
    let p = |goal| curve.resource_goal_probability_features(&x, goal).unwrap();
    assert!(p(0) > p(10) && p(10) > p(19) && p(19) > 0.);
    assert_eq!(p(20), 0.);
    let c = x[7] as f64;
    let n = 10000;
    let area = (0..=n)
        .map(|i| {
            sigmoid(1.2 - 3. * c * i as f64 / n as f64) * if i == 0 || i == n { 0.5 } else { 1. }
        })
        .sum::<f64>()
        * c
        / n as f64;
    assert!((curve.predict_features(&x) as f64 - (sigmoid(1.2) - 1. + area)).abs() < 1e-6);
    let legacy = model(&state);
    assert_eq!(legacy.resource_goal_probability_features(&x, 0), None);
    let encoded = serde_json::to_vec(&curve).unwrap();
    let decoded: ValueModel = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(curve.predict_features(&x), decoded.predict_features(&x));
    curve.objective = SEARCH_CONTINUATION_OBJECTIVE.into();
    assert!(curve.validate(&state).is_err());
    curve.objective = RESOURCE_CURVE_OBJECTIVE.into();
    curve.resource_curve.as_mut().unwrap().slope_weights.clear();
    assert!(curve.validate(&state).is_err());
    curve.resource_curve = None;
    assert!(curve.validate(&state).is_err());
}

#[test]
fn resource_curve_integration_handles_large_and_nearly_flat_logits() {
    for (a, raw_b) in [(1000., 1000.), (1000., -1000.), (-1000., 1000.), (1., -30.)] {
        let y = resource_curve_return(a, softplus(raw_b), 0.8);
        assert!(y.is_finite() && (-1. ..=0.8).contains(&y));
    }
    assert!(
        (resource_curve_return(1., softplus(-30.), 0.8) - (1.8 * sigmoid(1.) - 1.)).abs() < 1e-12
    );
}

#[test]
fn observed_terminal_resources_bypass_the_learned_curve() {
    let mut p = root();
    p.combat.entities.monsters[0].current_hp = 1;
    p.combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 991)];
    let s = CombatSearchState::new(p.clone()).unwrap();
    let mut curve = model(&s);
    curve.objective = RESOURCE_CURVE_OBJECTIVE.into();
    curve.resource_curve = Some(ResourceCurveHead {
        slope_weights: vec![0.],
        slope_bias: 0.,
    });
    let win = crate::shuffle_order::step(
        &p,
        sts_core::state::core::ClientInput::PlayCard {
            card_index: 0,
            target: Some(p.combat.entities.monsters[0].id),
        },
    )
    .unwrap();
    let hp = win.combat.entities.player.current_hp as u32;
    let s = CombatSearchState::new(win).unwrap();
    assert_eq!(curve.resource_goal_probability(&s, hp), Some(1.));
    assert_eq!(curve.resource_goal_probability(&s, hp + 1), Some(0.));
    p.combat.entities.player.current_hp = 0;
    let loss = CombatSearchState::new(p).unwrap();
    assert_eq!(curve.resource_goal_probability(&loss, 0), Some(0.));
}

#[test]
fn representation_exposes_card_trigger_not_damage_hits_and_keeps_order() {
    let mut p = root();
    let m = p.combat.entities.monsters[0].id;
    sts_core::engine::action_handlers::powers::handle_apply_power(
        m,
        m,
        PowerId::SharpHide,
        4,
        &mut p.combat,
    );
    p.combat.zones.hand = vec![CombatCard::for_scenario(CardId::TwinStrike, 101)];
    let a = CombatSearchState::new(p.clone()).unwrap();
    let names = feature_names(&a);
    let at = |name: &str| names.iter().position(|n| n == name).unwrap();
    let x = features(&a);
    assert_eq!(x[at("zone/0/numbers/3")], 4. / 50.);
    p.combat.zones.hand[0].id = CardId::Strike;
    let b = features(&CombatSearchState::new(p.clone()).unwrap());
    assert_eq!(x[at("zone/0/numbers/3")], b[at("zone/0/numbers/3")]);
    p.combat.entities.monsters[0].guardian.damage_taken += 1;
    let b = features(&CombatSearchState::new(p.clone()).unwrap());
    assert_ne!(x[at("guardian/remaining")], b[at("guardian/remaining")]);
    p.combat.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::Strike, 102),
        CombatCard::for_scenario(CardId::Defend, 103),
    ]
    .into();
    let a = features(&CombatSearchState::new(p.clone()).unwrap());
    let mut reversed = p
        .combat
        .zones
        .discard_pile
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    reversed.reverse();
    p.combat.zones.discard_pile = reversed.into();
    assert_ne!(a, features(&CombatSearchState::new(p).unwrap()));
}

#[test]
fn hand_features_keep_instance_costs_and_effect_requests_together() {
    let mut p = root();
    p.combat.turn.energy = 1;
    let player = p.combat.entities.player.id;
    sts_core::engine::action_handlers::powers::handle_apply_power(
        player,
        player,
        PowerId::Strength,
        4,
        &mut p.combat,
    );
    let mut discounted = CombatCard::for_scenario(CardId::Strike, 202);
    discounted.cost_for_turn = Some(0);
    p.combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 201),
        discounted,
        CombatCard::for_scenario(CardId::Carnage, 203),
        CombatCard::for_scenario(CardId::ShrugItOff, 204),
        CombatCard::for_scenario(CardId::AscendersBane, 205),
    ];
    let state = CombatSearchState::new(p).unwrap();
    let names = feature_names(&state);
    let x = features(&state);
    let at = |name: &str| x[names.iter().position(|n| n == name).unwrap()];
    assert_eq!(at("hand/0/cost"), 1. / 3.);
    assert_eq!(at("hand/1/cost"), 0.);
    assert_eq!(at("hand/0/damage_per_hit"), 10. / 50.);
    assert_eq!(at("hand/1/damage_per_hit"), 10. / 50.);
    assert_eq!(at("hand/0/playable"), 1.);
    assert_eq!(at("hand/2/playable"), 0.); // Insufficient energy, still has an attack request.
    assert!(at("hand/2/damage_per_hit") > 0.);
    assert_eq!(at("hand/3/damage_per_hit"), 0.); // Strength does not turn a Skill into damage.
    assert_eq!(at("hand/3/block"), 8. / 50.);
    assert_eq!(at("hand/3/draw"), 1. / 5.);
    assert_eq!(at("hand/4/playable"), 0.);
    assert_eq!(at("hand/4/ethereal"), 1.);
    assert!(names
        .iter()
        .zip(&x)
        .filter(|(n, _)| n.starts_with("hand/5/"))
        .all(|(_, &v)| v == 0.));
}

#[test]
fn shared_hand_model_masks_padding_and_validates_exported_layout() {
    let state = CombatSearchState::new(root()).unwrap();
    let mut model = model(&state);
    let start = model
        .feature_names
        .iter()
        .position(|n| n == "hand/0/present")
        .unwrap();
    let width = (model.feature_names.len() - start) / 10;
    let damage = model
        .feature_names
        .iter()
        .position(|n| n == "hand/0/damage_per_hit")
        .unwrap()
        - start;
    let mut weights = vec![0.; width];
    weights[damage] = 1.;
    model.hand_encoder = Some(HandEncoder {
        start,
        width,
        weights: vec![weights],
        bias: vec![0.1],
    });
    model.hidden_weights = vec![vec![0.; start + 2]];
    model.hidden_weights[0][start] = 1.;
    model.hidden_weights[0][start + 1] = 1.;
    model.validate(&state).unwrap();
    let mut x = features(&state);
    x[start..].fill(0.);
    x[start] = 1.;
    x[start + damage] = 0.2;
    x[start + width] = 1.;
    x[start + width + damage] = 0.4;
    // Two real cards produce .3 and .5; eight empty slots contribute nothing.
    assert!((model.predict_features(&x) - 0.58).abs() < 1e-6);
    let exported: ValueModel =
        serde_json::from_str(&serde_json::to_string(&model).unwrap()).unwrap();
    assert_eq!(model.predict_features(&x), exported.predict_features(&x));
    let mut malformed = model.clone();
    malformed.hand_encoder.as_mut().unwrap().width += 1;
    assert!(malformed.validate(&state).is_err());
    model.hand_encoder.as_mut().unwrap().bias[0] = f32::NAN;
    assert!(model.validate(&state).is_err());
}

#[test]
fn model_search_keeps_replay_and_pause_contract_and_rejects_bad_models() {
    let p = root();
    let state = CombatSearchState::new(p.clone()).unwrap();
    let model = model(&state);
    model.validate(&state).unwrap();
    let mut continuation_model = model.clone();
    continuation_model.objective = CONTINUATION_OBJECTIVE.into();
    continuation_model.validate(&state).unwrap();
    continuation_model.objective = SEARCH_CONTINUATION_OBJECTIVE.into();
    continuation_model.validate(&state).unwrap();
    continuation_model.objective = "arbitrary_optimal_value".into();
    assert!(continuation_model.validate(&state).is_err());
    let mut bad = model.clone();
    bad.hidden_weights[0].pop();
    assert!(bad.validate(&state).is_err());
    let mut unsupported = state.position().clone();
    unsupported.combat.entities.monsters[0].enemy_id = EnemyId::JawWorm;
    assert!(model
        .validate(&CombatSearchState::new(unsupported).unwrap())
        .is_err());
    let mut excessive = model.clone();
    excessive.output_bias = 100.;
    assert!(excessive.predict(&state) <= 74. / 75.);
    let mut bad = model.clone();
    bad.hidden_bias[0] = f32::NAN;
    assert!(bad.validate(&state).is_err());
    let config = SolverConfig {
        initial_width: 2,
        max_width: 8,
        max_potion_uses: Some(0),
        resource_model: Some(model),
        ..Default::default()
    };
    let mut whole = CombatSolver::new(p.clone(), config.clone()).unwrap();
    let mut chunked = CombatSolver::new(p, config).unwrap();
    whole.advance(3000, Duration::from_secs(30)).unwrap();
    for _ in 0..30 {
        chunked.advance(100, Duration::from_secs(30)).unwrap();
    }
    assert!(whole.stats().resource_evaluations > 0);
    assert_eq!(
        serde_json::to_value(whole.stats()).unwrap(),
        serde_json::to_value(chunked.stats()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(whole.plans()).unwrap(),
        serde_json::to_value(chunked.plans()).unwrap()
    );
    for plan in whole.plans() {
        assert_eq!(
            crate::shuffle_order::replay(whole.root(), &plan.inputs)
                .unwrap()
                .last(),
            Some(&plan.successor)
        );
    }
    let data = data::collect(&whole, 60, 0, 0).unwrap();
    assert!(data.rows.iter().any(|r| r.terminal == Some(-1.)));
    assert!(data
        .rows
        .iter()
        .skip(1)
        .enumerate()
        .all(|(i, r)| r.origin.as_ref().is_some_and(|(parent, _)| *parent <= i)));
    assert!(data
        .rows
        .iter()
        .any(|r| r.terminal.is_none() && r.observed_return.is_none()));
}

#[test]
fn response_collection_exposes_real_mode_shift_consequences_and_budget_gaps() {
    use sts_core::state::core::ClientInput;
    let p = ConstructedCombat {
        seed: 3101,
        encounter: EncounterId::TheGuardian,
        act: 1,
        room: RoomType::MonsterRoomBoss,
        hp: 68,
        deck: vec![
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 1),
            (CardId::AscendersBane, 0),
            (CardId::Carnage, 1),
            (CardId::PommelStrike, 1),
            (CardId::ShrugItOff, 0),
            (CardId::Inflame, 1),
            (CardId::Disarm, 0),
            (CardId::Impervious, 0),
            (CardId::Headbutt, 0),
        ],
        relics: vec![RelicId::Vajra],
        potions: vec![],
        max_potion_uses: Some(0),
    }
    .build()
    .unwrap();
    // Actual legal opening, not patched monster HP / phase / intent.
    let prefix = [
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        ClientInput::PlayCard {
            card_index: 3,
            target: Some(1),
        },
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        ClientInput::EndTurn,
        ClientInput::PlayCard {
            card_index: 2,
            target: None,
        },
    ];
    let p = crate::shuffle_order::replay(&p, &prefix)
        .unwrap()
        .pop()
        .unwrap();
    let solver = CombatSolver::new(
        p,
        SolverConfig {
            max_potion_uses: Some(0),
            ..Default::default()
        },
    )
    .unwrap();
    let partial = data::collect(&solver, 1800, 0, 1).unwrap();
    assert!(partial.response_work <= 1);
    assert!(partial
        .responses
        .iter()
        .flat_map(|r| &r.alternatives)
        .any(|a| !a.complete && a.unresolved > 0));
    let data = data::collect(&solver, 1800, 0, 4000).unwrap();
    assert!(data.response_work > 0 && data.response_work <= 4000);
    let root = data.responses.iter().find(|r| r.source == 0).unwrap();
    assert!(root.alternatives.iter().all(|a| a.complete));
    let at = |name: &str| data.feature_names.iter().position(|n| n == name).unwrap();
    let same_turn = root
        .alternatives
        .iter()
        .filter(|a| data.rows[a.first].features[at("turn")] == data.rows[0].features[at("turn")]);
    let mut shifted = vec![];
    let mut open = vec![];
    for alternative in same_turn {
        let best = alternative.best_boundary.unwrap();
        let trace = &data.traces[alternative.witness_trace.unwrap()];
        assert!(trace
            .rows
            .windows(2)
            .any(|pair| pair == [0, alternative.first]));
        assert_eq!(trace.rows.last(), Some(&best));
        assert!(data
            .traces
            .iter()
            .any(|trace| trace.rows.last() == Some(&best)));
        let hp = (data.rows[best].features[at("hp")] * 75.).round() as i32;
        if data.rows[alternative.first].features[at("guardian/open")] == 0. {
            shifted.push(hp);
        } else {
            open.push(hp);
        }
    }
    assert!(shifted.contains(&68));
    assert!(open.iter().any(|&hp| hp < 68));
}

#[test]
fn response_entries_include_setup_turns_and_expose_unexpanded_sources() {
    let solver = CombatSolver::new(root(), SolverConfig::default()).unwrap();
    let partial = data::collect(&solver, 0, 0, 1).unwrap();
    let entry = partial.responses.iter().find(|r| r.source == 0).unwrap();
    assert!(!entry.source_expanded && entry.alternatives.is_empty());
    assert!(partial.rows[0].children.is_none() && partial.response_work <= 1);

    let data = data::collect(&solver, 1800, 0, 4000).unwrap();
    let entry = data.responses.iter().find(|r| r.source == 0).unwrap();
    assert!(entry.source_expanded && entry.alternatives.iter().all(|a| a.complete));
    let at = |name: &str| data.feature_names.iter().position(|n| n == name).unwrap();
    // No single action changes the mode here. This real opening still needs
    // full-response data, instead of requiring a one-card mode fork to qualify.
    assert!(entry.alternatives.iter().all(|a| {
        let x = &data.rows[a.first].features;
        x[at("turn")] != data.rows[0].features[at("turn")]
            || (x[at("guardian/open")] == data.rows[0].features[at("guardian/open")]
                && x[at("guardian/triggered")] == 0.)
    }));

    let before_continuations = data::collect(&solver, 1800, 0, 0).unwrap().rows.len();
    let continued = data::collect(&solver, 1800, 500, 4000).unwrap();
    assert!(continued
        .responses
        .iter()
        .any(|r| r.source >= before_continuations));
}
