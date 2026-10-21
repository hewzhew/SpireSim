use crate::local::{LocalCombatBatch, LocalEnemy, LocalTask};
use crate::local_tactics::{TacticalPosition, MAX_CARDS};
use sts_core::sim::combat::CombatPosition;

fn batch(seed: u64, enemy: LocalEnemy) -> LocalCombatBatch {
    LocalCombatBatch::configured(
        &[(seed, enemy)],
        128,
        &[68],
        LocalTask::DrawSelect,
        None,
        0,
        None,
    )
    .unwrap()
}

#[test]
fn replayed_demonstration_preserves_every_selection_edge_and_source() {
    use sts_core::content::cards::CardId::*;
    let deck = [
        (BurningPact, 0),
        (Strike, 1),
        (Strike, 1),
        (Headbutt, 1),
        (PommelStrike, 1),
    ];
    let make = |search| {
        LocalCombatBatch::configured(
            &[(17, LocalEnemy::ThreeLouse)],
            128,
            &[75],
            LocalTask::DrawSelect,
            search,
            if search.is_some() { 128 } else { 0 },
            Some(&deck),
        )
        .unwrap()
    };
    let mut b = make(Some(256));
    assert!(b.tactical_demonstration(0).is_err());
    let mut first = true;
    loop {
        let f = b.tactical_frame().unwrap();
        if f.slots.is_empty() {
            break;
        }
        let p = &f.positions[0];
        let a = if first {
            first = false;
            play(p, 4).unwrap()
        } else {
            p.actions
                .iter()
                .rposition(|a| a[2] == 1.0 || a[3] == 1.0 || a[4] == 1.0)
                .or_else(|| [5, 6, 0].into_iter().find_map(|k| play(p, k)))
                .unwrap_or_else(|| p.actions.iter().position(|a| a[1] == 1.0).unwrap())
        };
        let mut weights = vec![0.0; p.actions.len()];
        weights[a] = 1.0;
        b.guide(f.revision, &[weights], &[None]).unwrap();
    }
    assert_eq!(b.outcomes()[0].kind, "win");
    let before = b.evidence_json().unwrap();
    let revision = b.tactical_frame().unwrap().revision;
    let demo = b.tactical_demonstration(0).unwrap();
    assert!(b.tactical_demonstration(1).is_err());
    assert!(demo.choices.len() > b.outcomes()[0].decisions);
    for kind in [2, 3, 4] {
        assert!(demo
            .positions
            .iter()
            .zip(&demo.choices)
            .any(|(p, &a)| p.actions[a][kind] == 1.0));
    }
    let mut direct = make(None);
    for (p, &a) in demo.positions.iter().zip(&demo.choices) {
        let f = direct.tactical_frame().unwrap();
        assert_eq!(&f.positions[0], p);
        direct.advance(f.revision, &[a]).unwrap();
    }
    assert_eq!(direct.outcomes()[0].kind, "win");
    assert_eq!(state(&b), state(&direct));
    assert_eq!(
        demo.positions,
        b.tactical_demonstration(0).unwrap().positions
    );
    assert_eq!(before, b.evidence_json().unwrap());
    assert_eq!(revision, b.tactical_frame().unwrap().revision);
}

#[test]
fn custom_deck_preserves_duplicates_upgrades_and_count_across_turns() {
    use sts_core::content::cards::CardId;
    let deck = [
        (CardId::AscendersBane, 0),
        (CardId::Defend, 1),
        (CardId::Headbutt, 0),
        (CardId::PommelStrike, 1),
        (CardId::PommelStrike, 0),
        (CardId::ShrugItOff, 1),
        (CardId::BurningPact, 1),
    ];
    let mut b = LocalCombatBatch::configured(
        &[(31, LocalEnemy::Cultist)],
        128,
        &[24],
        LocalTask::DrawSelect,
        None,
        0,
        Some(&deck),
    )
    .unwrap();
    let p = b.tactical_frame().unwrap().positions.remove(0);
    assert_eq!(p.cards.len(), 7);
    assert_eq!(p.cards.iter().filter(|c| c[6] == 1.0).count(), 2);
    assert_eq!(
        p.cards
            .iter()
            .filter(|c| c[6] == 1.0 && c[14] == 1.0)
            .count(),
        1
    );
    let position = state(&b);
    let lineages: Vec<_> = position
        .combat
        .zones
        .hand
        .iter()
        .chain(position.combat.zones.draw_pile.iter())
        .map(|c| c.lineage)
        .collect();
    assert_eq!(lineages.len(), 7);
    for (i, lineage) in lineages.iter().enumerate() {
        assert!(!lineages[..i].contains(lineage));
    }
    let evidence: serde_json::Value = serde_json::from_str(&b.evidence_json().unwrap()).unwrap();
    assert_eq!(
        evidence["episodes"][0]["deck_recipe"],
        serde_json::to_value(deck).unwrap()
    );
    assert_eq!(
        evidence["episodes"][0]["source"],
        "prescribed_custom_deck_encounter"
    );
    loop {
        let f = b.tactical_frame().unwrap();
        if f.slots.is_empty() {
            break;
        }
        assert_eq!(f.positions[0].cards.len(), 7);
        let end = f.positions[0]
            .actions
            .iter()
            .position(|a| a[1] == 1.0)
            .unwrap();
        b.advance(f.revision, &[end]).unwrap();
    }
    assert_eq!(b.outcomes()[0].kind, "loss");
    assert!(b.outcomes()[0].turn > 1);
}

#[test]
fn unsupported_decks_do_not_become_partially_admitted_tasks() {
    use sts_core::content::cards::CardId;
    for deck in [
        vec![],
        vec![(CardId::Strike, 0); 17],
        vec![(CardId::Corruption, 0)],
        vec![(CardId::AscendersBane, 1)],
        vec![(CardId::PommelStrike, 2)],
    ] {
        assert!(LocalCombatBatch::configured(
            &[(31, LocalEnemy::Cultist)],
            128,
            &[24],
            LocalTask::DrawSelect,
            None,
            0,
            Some(&deck)
        )
        .is_err());
    }
    assert!(LocalCombatBatch::configured(
        &[(31, LocalEnemy::Cultist)],
        128,
        &[24],
        LocalTask::Starter,
        None,
        0,
        Some(&[(CardId::Strike, 0)])
    )
    .is_err());
}

fn state(batch: &LocalCombatBatch) -> CombatPosition {
    let v: serde_json::Value = serde_json::from_str(&batch.evidence_json().unwrap()).unwrap();
    serde_json::from_value(v["episodes"][0]["successor"].clone()).unwrap()
}

fn play(p: &TacticalPosition, kind: usize) -> Option<usize> {
    p.actions
        .iter()
        .zip(&p.action_refs)
        .position(|(a, r)| a[0] == 1.0 && r[0] >= 0 && p.cards[r[0] as usize][kind] == 1.0)
}

fn advance(batch: &mut LocalCombatBatch, a: usize) {
    batch
        .advance(batch.tactical_frame().unwrap().revision, &[a])
        .unwrap();
}

fn find(kinds: &[usize]) -> LocalCombatBatch {
    (1..128)
        .map(|s| batch(s, LocalEnemy::JawWorm))
        .find(|b| {
            let f = b.tactical_frame().unwrap();
            kinds.iter().all(|&k| play(&f.positions[0], k).is_some())
        })
        .unwrap()
}

#[test]
fn tactical_targets_and_duplicate_cards_retain_separate_legal_edges() {
    let b = batch(17, LocalEnemy::ThreeLouse);
    assert!(b.frame().is_err());
    let f = b.tactical_frame().unwrap();
    let p = &f.positions[0];
    assert_eq!(p.cards.len(), 15);
    assert_eq!(p.monsters.len(), 3);
    let mut target_sets = std::collections::BTreeMap::<_, Vec<_>>::new();
    for (a, r) in p.actions.iter().zip(&p.action_refs) {
        if a[0] == 1.0 && r[1] >= 0 {
            target_sets.entry(r[0]).or_default().push(r[1]);
        }
    }
    assert!(!target_sets.is_empty());
    for targets in target_sets.values() {
        assert_eq!(targets, &[0, 1, 2]);
    }
    assert!(p.cards.iter().filter(|c| c[0] == 1.0).count() > 1);
    let other = LocalCombatBatch::configured(
        &[(17, LocalEnemy::ThreeLouse)],
        128,
        &[68],
        LocalTask::DrawSelect,
        Some(256),
        128,
        None,
    )
    .unwrap();
    assert_eq!(f.positions, other.tactical_frame().unwrap().positions);
}

#[test]
fn target_damage_observation_changes_only_for_the_vulnerable_target() {
    let mut b = (1..128)
        .map(|s| batch(s, LocalEnemy::ThreeLouse))
        .find(|b| {
            let p = &b.tactical_frame().unwrap().positions[0];
            play(p, 0).is_some() && play(p, 2).is_some()
        })
        .unwrap();
    let p = b.tactical_frame().unwrap().positions.remove(0);
    let bash = play(&p, 2).unwrap();
    let target = p.action_refs[bash][1];
    advance(&mut b, bash);
    let p = b.tactical_frame().unwrap().positions.remove(0);
    let strike = p.action_refs[play(&p, 0).unwrap()][0];
    let damage: Vec<_> = p
        .actions
        .iter()
        .zip(&p.action_refs)
        .filter(|(_, r)| r[0] == strike)
        .map(|(a, r)| (r[1], a[5]))
        .collect();
    assert_eq!(damage.len(), 3);
    assert_eq!(
        damage.iter().find(|(t, _)| *t == target).unwrap().1,
        9.0 / 20.0
    );
    assert!(damage
        .iter()
        .filter(|(t, _)| *t != target)
        .all(|(_, d)| *d == 6.0 / 20.0));
}

#[test]
fn numerical_selection_prefix_keeps_member_order_even_when_domain_order_is_reversed() {
    use crate::{
        CombatSearchSpace, CombatSearchState, Guidance, GuidanceBudget, SearchBudget, SearchConfig,
        SearchSession, StepStatus,
    };
    use sts_core::state::core::{EngineState, HandSelectReason, PendingChoice};
    let mut root = state(&batch(17, LocalEnemy::JawWorm));
    let hand = root.combat.zones.hand.clone();
    // A constructed two-card selection sentinel; it is not a training root.
    root.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: hand.iter().rev().map(|c| c.lineage).collect(),
        min_cards: 2,
        max_cards: 2,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });
    let mut session = SearchSession::new(
        CombatSearchSpace::default(),
        CombatSearchState::new(root).unwrap(),
        SearchConfig::default(),
        SearchBudget {
            max_services: 20,
            max_engine_steps: 5120,
            max_nodes: 21,
            max_depth: 20,
            deadline: None,
            guidance: GuidanceBudget::default(),
        },
    )
    .unwrap();
    let mut source = session.root();
    let chosen = [hand.len() - 1, 0];
    for (step, card) in [None, Some(chosen[0]), Some(chosen[1])]
        .into_iter()
        .enumerate()
    {
        let p = crate::local_tactics::encode(&session, source).unwrap();
        let a = match card {
            None => p.actions.iter().position(|a| a[2] == 1.0).unwrap(),
            Some(c) => p
                .actions
                .iter()
                .zip(&p.action_refs)
                .position(|(a, r)| a[3] == 1.0 && r[0] == c as i64)
                .unwrap(),
        };
        let mut weights = vec![0.0; p.actions.len()];
        weights[a] = 1.0;
        session
            .submit_guidance(
                source,
                Guidance {
                    weights: Some(weights),
                    value: None,
                },
            )
            .unwrap();
        source = match session.follow_guidance(source).unwrap() {
            StepStatus::NeedsGuidance(s) | StepStatus::Guided(s) => s,
            other => panic!("unexpected selection result {other:?}"),
        };
        if step > 0 {
            let p = crate::local_tactics::encode(&session, source).unwrap();
            for (order, &row) in chosen[..step].iter().enumerate() {
                assert_eq!(p.cards[row][23], (order + 1) as f32 / MAX_CARDS as f32);
            }
        }
    }
    assert!(session.observed_trace(source).unwrap().is_empty());
}

#[test]
fn numerical_burning_pact_binds_selected_instance_and_executes_real_draws() {
    let mut b = find(&[4]);
    let p = b.tactical_frame().unwrap().positions.remove(0);
    advance(&mut b, play(&p, 4).unwrap());
    let before = state(&b);
    let p = b.tactical_frame().unwrap().positions.remove(0);
    assert_eq!(p.context[17], 1.0);
    advance(&mut b, p.actions.iter().position(|a| a[2] == 1.0).unwrap());
    let p = b.tactical_frame().unwrap().positions.remove(0);
    let a = p.actions.iter().rposition(|a| a[3] == 1.0).unwrap();
    let selected = p.action_refs[a][0] as usize;
    let lineage = before.combat.zones.hand[selected].lineage;
    assert_eq!(p.cards[selected][22], 1.0);
    advance(&mut b, a);
    assert_eq!(
        state(&b),
        before,
        "virtual assembly must not execute the engine"
    );
    let p = b.tactical_frame().unwrap().positions.remove(0);
    assert_eq!(p.cards[selected][23], 1.0 / MAX_CARDS as f32);
    advance(&mut b, p.actions.iter().position(|a| a[4] == 1.0).unwrap());
    let after = state(&b);
    assert!(after
        .combat
        .zones
        .exhaust_pile
        .iter()
        .any(|c| c.lineage == lineage));
    assert_eq!(
        after.combat.zones.hand.len(),
        before.combat.zones.hand.len() + 1
    );
    assert_eq!(
        after.combat.zones.draw_pile.len() + 2,
        before.combat.zones.draw_pile.len()
    );
    let evidence: serde_json::Value = serde_json::from_str(&b.evidence_json().unwrap()).unwrap();
    assert_eq!(
        evidence["episodes"][0]["inputs"].as_array().unwrap().len(),
        2
    );
    assert_eq!(evidence["schema"], 2);
}

#[test]
fn numerical_headbutt_selects_from_discard_and_preserves_source_identity() {
    let mut b = find(&[0, 1, 5]);
    let p = b.tactical_frame().unwrap().positions.remove(0);
    let a = play(&p, 1).unwrap();
    let defend = state(&b).combat.zones.hand[p.action_refs[a][0] as usize].lineage;
    advance(&mut b, a);
    let p = b.tactical_frame().unwrap().positions.remove(0);
    advance(&mut b, play(&p, 0).unwrap());
    let p = b.tactical_frame().unwrap().positions.remove(0);
    advance(&mut b, play(&p, 5).unwrap());
    let p = b.tactical_frame().unwrap().positions.remove(0);
    assert_eq!(p.context[18], 1.0);
    advance(&mut b, p.actions.iter().position(|a| a[2] == 1.0).unwrap());
    let p = b.tactical_frame().unwrap().positions.remove(0);
    let a = p
        .actions
        .iter()
        .zip(&p.action_refs)
        .position(|(a, r)| a[3] == 1.0 && p.cards[r[0] as usize][1] == 1.0)
        .unwrap();
    assert_eq!(
        p.cards[p.action_refs[a][0] as usize][10], 1.0,
        "member is a discard row"
    );
    advance(&mut b, a);
    let p = b.tactical_frame().unwrap().positions.remove(0);
    advance(&mut b, p.actions.iter().position(|a| a[4] == 1.0).unwrap());
    let after = state(&b);
    assert!(!after
        .combat
        .zones
        .discard_pile
        .iter()
        .any(|c| c.lineage == defend));
    assert_eq!(
        after.combat.zones.draw_pile.first().unwrap().lineage,
        defend
    );
}
