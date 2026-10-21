use super::*;

#[test]
fn stable_player_signature_in_combat_scope_ignores_gold_deltas() {
    let mut baseline = blank_test_combat();
    baseline
        .entities
        .monsters
        .push(planned_monster(EnemyId::Cultist, 3));

    let mut variant = baseline.clone();
    variant.entities.player.gold += 99;
    variant.entities.player.gold_delta_this_combat += 99;

    assert_eq!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &baseline),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &variant),
    );
}

#[test]
fn stable_monster_signature_ignores_irrelevant_runtime_fields_but_keeps_relevant_ones() {
    let mut cultist = blank_test_combat();
    cultist
        .entities
        .monsters
        .push(planned_monster(EnemyId::Cultist, 3));

    let mut irrelevant_runtime = cultist.clone();
    irrelevant_runtime.entities.monsters[0].hexaghost.activated = true;
    assert_eq!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &cultist),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &irrelevant_runtime),
    );

    let mut changed_cultist = cultist.clone();
    changed_cultist.entities.monsters[0].cultist.first_move = false;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &cultist),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &changed_cultist),
    );

    let mut sentry = blank_test_combat();
    sentry
        .entities
        .monsters
        .push(planned_monster(EnemyId::Sentry, 3));
    let mut changed_sentry = sentry.clone();
    changed_sentry.entities.monsters[0].sentry.first_move = false;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &sentry),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &changed_sentry),
    );

    let mut sphere = blank_test_combat();
    sphere
        .entities
        .monsters
        .push(planned_monster(EnemyId::SphericGuardian, 2));
    let mut changed_sphere = sphere.clone();
    changed_sphere.entities.monsters[0]
        .spheric_guardian
        .second_move = false;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &sphere),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &changed_sphere),
    );

    let mut hexaghost = blank_test_combat();
    hexaghost
        .entities
        .monsters
        .push(planned_monster(EnemyId::Hexaghost, 3));
    let mut relevant_runtime = hexaghost.clone();
    relevant_runtime.entities.monsters[0].hexaghost.activated = true;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &hexaghost),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &relevant_runtime),
    );

    let mut spiker = blank_test_combat();
    spiker
        .entities
        .monsters
        .push(planned_monster(EnemyId::Spiker, 2));
    let mut changed_spiker = spiker.clone();
    changed_spiker.entities.monsters[0].spiker.thorns_count = 5;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &spiker),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &changed_spiker),
    );

    let mut shield = blank_test_combat();
    shield
        .entities
        .monsters
        .push(planned_monster(EnemyId::SpireShield, 3));
    let mut changed_shield = shield.clone();
    changed_shield.entities.monsters[0].spire_shield.move_count = 2;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &shield),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &changed_shield),
    );

    let mut spear = blank_test_combat();
    spear
        .entities
        .monsters
        .push(planned_monster(EnemyId::SpireSpear, 3));
    let mut changed_spear = spear.clone();
    changed_spear.entities.monsters[0].spire_spear.move_count = 2;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &spear),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &changed_spear),
    );

    let mut red_slaver = blank_test_combat();
    red_slaver
        .entities
        .monsters
        .push(planned_monster(EnemyId::SlaverRed, 1));
    let mut changed_red_slaver = red_slaver.clone();
    changed_red_slaver.entities.monsters[0]
        .slaver_red
        .used_entangle = true;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &red_slaver),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &changed_red_slaver),
    );

    let mut changed_red_slaver_first_turn = red_slaver.clone();
    changed_red_slaver_first_turn.entities.monsters[0]
        .slaver_red
        .first_turn = false;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &red_slaver),
        stable_outcome_key(
            &EngineState::CombatPlayerTurn,
            &changed_red_slaver_first_turn
        ),
    );

    let mut gremlin_nob = blank_test_combat();
    gremlin_nob
        .entities
        .monsters
        .push(planned_monster(EnemyId::GremlinNob, 3));
    let mut changed_gremlin_nob = gremlin_nob.clone();
    changed_gremlin_nob.entities.monsters[0]
        .gremlin_nob
        .used_bellow = true;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &gremlin_nob),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &changed_gremlin_nob),
    );
}
