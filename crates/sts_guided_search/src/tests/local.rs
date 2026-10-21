use crate::local::{LocalCombatBatch, LocalEnemy, CARD_COUNT};

#[test]
fn numerical_sources_and_all_ordinals_are_validated_before_mutation() {
    let cases = [(17, LocalEnemy::JawWorm), (29, LocalEnemy::Cultist)];
    let mut batch = LocalCombatBatch::new(&cases, 128).unwrap();
    let before = batch.evidence_json().unwrap();
    let frame = batch.frame().unwrap();
    assert!(batch.advance(frame.revision, &[0, usize::MAX]).is_err());
    assert_eq!(before, batch.evidence_json().unwrap());
    let other = LocalCombatBatch::new(&cases, 128).unwrap();
    assert!(batch
        .advance(other.frame().unwrap().revision, &[0, 0])
        .is_err());
    assert_eq!(before, batch.evidence_json().unwrap());
    batch.advance(frame.revision, &[0, 0]).unwrap();
    assert!(batch.advance(frame.revision, &[0, 0]).is_err());
}

#[test]
fn starter_numeric_rows_keep_every_card_and_duplicate_action() {
    let batch = LocalCombatBatch::new(&[(17, LocalEnemy::JawWorm)], 128).unwrap();
    let frame = batch.frame().unwrap();
    let row = &frame.positions[0];
    assert_eq!(row.cards.len(), CARD_COUNT);
    let plays: Vec<_> = row
        .action_cards
        .iter()
        .filter(|&&i| i >= 0)
        .copied()
        .collect();
    assert_eq!(plays.len(), 5);
    for (index, &card) in plays.iter().enumerate() {
        assert!(!plays[..index].contains(&card));
        assert_eq!(row.cards[card as usize][5], 1.0);
    }
    assert!(row.cards.iter().any(|c| c[6] == 1.0 && c[10] > 0.0));
}

#[test]
fn actual_enemy_turns_end_in_loss_but_a_decision_cut_is_unknown() {
    for (limit, expected) in [(1, "unknown_decision_limit"), (128, "loss")] {
        let mut batch = LocalCombatBatch::new(&[(17, LocalEnemy::Cultist)], limit).unwrap();
        loop {
            let frame = batch.frame().unwrap();
            if frame.slots.is_empty() {
                break;
            }
            let end = frame.positions[0]
                .action_cards
                .iter()
                .position(|&a| a == -1)
                .unwrap();
            batch.advance(frame.revision, &[end]).unwrap();
        }
        let result = batch.outcomes().remove(0);
        assert_eq!(result.kind, expected);
        assert!(result.turn > 1);
        assert!(!result.win_replayed);
        assert!(batch.demonstration(0).is_err());
        if expected == "loss" {
            assert!(result.hp <= 0);
        } else {
            assert!(result.hp > 0);
        }
    }
}

#[test]
fn constructed_test_policy_crosses_turns_and_replays_actual_win() {
    let mut batch = LocalCombatBatch::new(&[(17, LocalEnemy::Cultist)], 128).unwrap();
    loop {
        let frame = batch.frame().unwrap();
        if frame.slots.is_empty() {
            break;
        }
        let p = &frame.positions[0];
        let action = [3, 1, 2]
            .into_iter()
            .find_map(|kind| {
                p.action_cards
                    .iter()
                    .position(|&c| c >= 0 && p.cards[c as usize][kind] == 1.0)
            })
            .unwrap_or_else(|| p.action_cards.iter().position(|&c| c == -1).unwrap());
        batch.advance(frame.revision, &[action]).unwrap();
    }
    let result = batch.outcomes().remove(0);
    assert_eq!(result.kind, "win");
    assert!(result.win_replayed && result.replay_engine_steps > 0 && result.turn > 1);
    let evidence: serde_json::Value =
        serde_json::from_str(&batch.evidence_json().unwrap()).unwrap();
    assert_eq!(
        evidence["episodes"][0]["inputs"].as_array().unwrap().len(),
        result.decisions
    );
}

#[test]
fn hp_intervention_and_search_share_the_exact_initial_position() {
    let cases = [(17, LocalEnemy::JawWorm)];
    let direct = LocalCombatBatch::with_starting_hp(&cases, 128, &[24]).unwrap();
    let search = LocalCombatBatch::search(&cases, 128, &[24], 256, 0).unwrap();
    assert_eq!(
        direct.frame().unwrap().positions,
        search.frame().unwrap().positions
    );
    let evidence: serde_json::Value =
        serde_json::from_str(&search.evidence_json().unwrap()).unwrap();
    assert_eq!(evidence["episodes"][0]["starting_hp"], 24);
    assert_eq!(search.frame().unwrap().positions[0].context[0], 24.0 / 75.0);
    assert!(LocalCombatBatch::with_starting_hp(&cases, 128, &[0]).is_err());
    assert!(LocalCombatBatch::with_starting_hp(&cases, 128, &[]).is_err());
}

#[test]
fn search_guidance_validates_all_sources_and_rows_before_mutation() {
    let cases = [(17, LocalEnemy::JawWorm), (29, LocalEnemy::Cultist)];
    let mut search = LocalCombatBatch::search(&cases, 128, &[24, 24], 32, 0).unwrap();
    let frame = search.frame().unwrap();
    let weights: Vec<_> = frame
        .positions
        .iter()
        .map(|p| vec![1.0; p.action_cards.len()])
        .collect();
    let before = search.evidence_json().unwrap();
    assert!(search
        .guide(frame.revision, &weights, &[None, Some(f64::NAN)])
        .is_err());
    assert_eq!(before, search.evidence_json().unwrap());
    assert!(search.advance(frame.revision, &[0, 0]).is_err());
    let mut invalid = weights.clone();
    invalid[1].pop();
    assert!(search
        .guide(frame.revision, &invalid, &[None, None])
        .is_err());
    assert_eq!(before, search.evidence_json().unwrap());
    search
        .guide(frame.revision, &weights, &[None, None])
        .unwrap();
    assert!(search
        .guide(frame.revision, &weights, &[None, None])
        .is_err());
}

#[test]
fn search_loss_observation_and_depth_limit_remain_unknown() {
    for (depth, budget, hp) in [(128, 1, 1), (1, 32, 68)] {
        let mut search =
            LocalCombatBatch::search(&[(17, LocalEnemy::JawWorm)], depth, &[hp], budget, 128)
                .unwrap();
        loop {
            let frame = search.frame().unwrap();
            if frame.slots.is_empty() {
                break;
            }
            let p = &frame.positions[0];
            let mut weights = vec![0.0; p.action_cards.len()];
            weights[p.action_cards.iter().position(|&a| a == -1).unwrap()] = 1.0;
            search.guide(frame.revision, &[weights], &[None]).unwrap();
        }
        let o = &search.outcomes()[0];
        assert!(o.kind.starts_with("unknown"));
        assert!(o.services <= budget);
        if hp == 1 {
            assert!(o.terminal_observations > 0);
        } else {
            assert!(o.coverage_gaps > 0);
        }
        assert!(!o.win_replayed);
        assert!(search.demonstration(0).is_err());
    }
}

#[test]
fn numerical_search_can_produce_a_complete_shared_core_win() {
    let mut search =
        LocalCombatBatch::search(&[(17, LocalEnemy::Cultist)], 128, &[68], 256, 128).unwrap();
    loop {
        let frame = search.frame().unwrap();
        if frame.slots.is_empty() {
            break;
        }
        let p = &frame.positions[0];
        let a = [3, 1, 2]
            .into_iter()
            .find_map(|kind| {
                p.action_cards
                    .iter()
                    .position(|&c| c >= 0 && p.cards[c as usize][kind] == 1.0)
            })
            .unwrap_or_else(|| p.action_cards.iter().position(|&c| c == -1).unwrap());
        let mut weights = vec![0.0; p.action_cards.len()];
        weights[a] = 1.0;
        search
            .guide(frame.revision, &[weights], &[Some(1.0)])
            .unwrap();
    }
    let o = &search.outcomes()[0];
    assert_eq!(o.kind, "win");
    assert!(o.win_replayed && o.replay_engine_steps > 0);
    assert!(o.turn > 1 && o.decisions <= o.services);
    let demo = search.demonstration(0).unwrap();
    let mut direct = LocalCombatBatch::new(&[(17, LocalEnemy::Cultist)], 128).unwrap();
    for (position, &a) in demo.positions.iter().zip(&demo.choices) {
        let frame = direct.frame().unwrap();
        assert_eq!(&frame.positions[0], position);
        direct.advance(frame.revision, &[a]).unwrap();
    }
    assert_eq!(direct.outcomes()[0].kind, "win");
}
