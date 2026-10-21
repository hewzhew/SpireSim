use super::*;

/// Proposes one complete current-turn allocation for two attacking thieves.
///
/// These are narrow, replay-checked corridors rather than action priors. They
/// neither prune ordinary turn plans nor claim that the proposed successor is
/// terminally good.
pub fn combat_plan_turn_prefix_proposal_v1(
    position: &CombatPosition,
) -> Option<CombatPlanTurnPrefixProposalV1> {
    let combat = &position.combat;
    if let Some(proposal) = press_single_thief_escape_window(combat) {
        return Some(proposal);
    }
    let thieves = attacking_double_thieves(combat)?;
    secure_kill_behind_exhaust_block(combat, &thieves)
        .or_else(|| split_pressure_around_defensive_bridge(combat, &thieves))
}

fn press_single_thief_escape_window(
    combat: &CombatState,
) -> Option<CombatPlanTurnPrefixProposalV1> {
    let mut living = combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action());
    let thief = living.next()?;
    if living.next().is_some()
        || !matches!(enemy_id(thief), EnemyId::Looter | EnemyId::Mugger)
        || thief.thief.stolen_gold <= 0
        || !matches!(
            project_monster_move_preview_in_combat(combat, thief).visible_intent,
            VisibleIntentKind::Defend | VisibleIntentKind::Escape
        )
    {
        return None;
    }

    let strikes = combat
        .zones
        .hand
        .iter()
        .filter(|card| {
            card.id == CardId::Strike
                && card.cost_for_turn_java() == 1
                && cards::can_play_card(card, combat).is_ok()
        })
        .take(2)
        .collect::<Vec<_>>();
    if strikes.len() != 2 {
        return None;
    }
    let thunderclap = playable_one_cost_card(combat, CardId::ThunderClap);
    let required_energy = 2_u8.saturating_add(u8::from(thunderclap.is_some()));
    if combat.turn.energy < required_energy {
        return None;
    }

    let mut steps = Vec::with_capacity(4);
    if let Some(thunderclap) = thunderclap {
        steps.push(CombatPlanPrefixStepV1::PlayCard {
            card_lineage: thunderclap.lineage,
            target: None,
        });
    }
    steps.extend(
        strikes
            .into_iter()
            .map(|strike| CombatPlanPrefixStepV1::PlayCard {
                card_lineage: strike.lineage,
                target: Some(thief.id),
            }),
    );
    steps.push(CombatPlanPrefixStepV1::EndTurn);
    Some(CombatPlanTurnPrefixProposalV1 {
        kind: CombatPlanPrefixKindV1::PressSingleThiefEscapeWindow,
        service_scope: CombatPlanPrefixServiceScopeV1::ContinuationOnly,
        steps,
    })
}

fn attacking_double_thieves(combat: &CombatState) -> Option<Vec<&MonsterEntity>> {
    let thieves = combat
        .entities
        .monsters
        .iter()
        .filter(|monster| {
            monster.is_alive_for_action()
                && matches!(enemy_id(monster), EnemyId::Looter | EnemyId::Mugger)
        })
        .collect::<Vec<_>>();
    if thieves.len() != 2
        || combat
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.is_alive_for_action())
            .count()
            != 2
        || thieves.iter().any(|monster| {
            !matches!(
                project_monster_move_preview_in_combat(combat, monster).visible_intent,
                VisibleIntentKind::Attack
                    | VisibleIntentKind::AttackBuff
                    | VisibleIntentKind::AttackDebuff
                    | VisibleIntentKind::AttackDefend
            )
        })
    {
        return None;
    }
    Some(thieves)
}

/// Secures an exact lethal on the lower thief while covering the surviving
/// thief's visible attack with a draw/block card and already-held exhaust fuel.
///
/// The block check deliberately ignores whatever Shrug It Off draws. A
/// proposal therefore needs enough visible margin from cards already in hand;
/// an additional drawn status or skill may help but is never assumed.
fn secure_kill_behind_exhaust_block(
    combat: &CombatState,
    thieves: &[&MonsterEntity],
) -> Option<CombatPlanTurnPrefixProposalV1> {
    if combat.turn.energy < 3 {
        return None;
    }
    let shrug = playable_one_cost_card(combat, CardId::ShrugItOff)?;
    let second_wind = playable_one_cost_card(combat, CardId::SecondWind)?;
    let mut thieves = thieves.to_vec();
    thieves.sort_by(|left, right| {
        left.current_hp
            .saturating_add(left.block)
            .cmp(&right.current_hp.saturating_add(right.block))
            .then_with(|| left.id.cmp(&right.id))
    });
    let target = thieves[0];
    let survivor = thieves[1];
    if target.thief.stolen_gold <= 0 {
        return None;
    }
    let strike = combat.zones.hand.iter().find(|card| {
        card.id == CardId::Strike
            && card.cost_for_turn_java() == 1
            && cards::can_play_card(card, combat).is_ok()
            && cards::evaluate_card_for_play(card, combat, Some(target.id)).base_damage_mut
                >= target.current_hp.saturating_add(target.block)
    })?;

    let exhaustible_non_attacks = combat
        .zones
        .hand
        .iter()
        .filter(|card| {
            card.lineage != shrug.lineage
                && card.lineage != second_wind.lineage
                && card.lineage != strike.lineage
                && get_card_definition(card.id).card_type != CardType::Attack
        })
        .count();
    if exhaustible_non_attacks == 0 {
        return None;
    }
    let shrug_block = cards::evaluate_card_for_play(shrug, combat, None).base_block_mut;
    let second_wind_block = cards::evaluate_card_for_play(second_wind, combat, None).base_block_mut;
    let visible_survivor_damage =
        project_monster_move_preview_in_combat(combat, survivor).total_damage?;
    let covered_damage = combat
        .entities
        .player
        .block
        .saturating_add(shrug_block)
        .saturating_add(
            second_wind_block.saturating_mul(i32::try_from(exhaustible_non_attacks).ok()?),
        );
    if covered_damage < visible_survivor_damage {
        return None;
    }

    Some(CombatPlanTurnPrefixProposalV1 {
        kind: CombatPlanPrefixKindV1::SecureThiefKillBehindExhaustBlock,
        service_scope: CombatPlanPrefixServiceScopeV1::ContinuationOnly,
        steps: vec![
            CombatPlanPrefixStepV1::PlayCard {
                card_lineage: shrug.lineage,
                target: None,
            },
            CombatPlanPrefixStepV1::PlayCard {
                card_lineage: strike.lineage,
                target: Some(target.id),
            },
            CombatPlanPrefixStepV1::PlayCard {
                card_lineage: second_wind.lineage,
                target: None,
            },
            CombatPlanPrefixStepV1::EndTurn,
        ],
    })
}

fn split_pressure_around_defensive_bridge(
    combat: &CombatState,
    thieves: &[&MonsterEntity],
) -> Option<CombatPlanTurnPrefixProposalV1> {
    let power_through = playable_one_cost_card(combat, CardId::PowerThrough)?;
    let strikes = combat
        .zones
        .hand
        .iter()
        .filter(|card| {
            card.id == CardId::Strike
                && card.cost_for_turn_java() == 1
                && cards::can_play_card(card, combat).is_ok()
        })
        .take(2)
        .collect::<Vec<_>>();
    if strikes.len() != 2 || combat.turn.energy < 3 {
        return None;
    }

    let mut thieves = thieves.to_vec();
    thieves.sort_by(|left, right| {
        left.current_hp
            .cmp(&right.current_hp)
            .then_with(|| left.id.cmp(&right.id))
    });
    let lower_hp_thief = thieves[0];
    let higher_hp_thief = thieves[1];
    Some(CombatPlanTurnPrefixProposalV1 {
        kind: CombatPlanPrefixKindV1::SplitThiefPressureAroundDefensiveBridge,
        service_scope: CombatPlanPrefixServiceScopeV1::RootEligible,
        steps: vec![
            CombatPlanPrefixStepV1::PlayCard {
                card_lineage: strikes[0].lineage,
                target: Some(higher_hp_thief.id),
            },
            CombatPlanPrefixStepV1::PlayCard {
                card_lineage: power_through.lineage,
                target: None,
            },
            CombatPlanPrefixStepV1::PlayCard {
                card_lineage: strikes[1].lineage,
                target: Some(lower_hp_thief.id),
            },
            CombatPlanPrefixStepV1::EndTurn,
        ],
    })
}

fn playable_one_cost_card(combat: &CombatState, id: CardId) -> Option<&CombatCard> {
    combat.zones.hand.iter().find(|card| {
        card.id == id
            && card.cost_for_turn_java() == 1
            && cards::can_play_card(card, combat).is_ok()
    })
}
