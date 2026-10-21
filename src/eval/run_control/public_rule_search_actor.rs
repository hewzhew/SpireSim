//! Public, commit-only hybrid actor. Strategic decisions use the existing rule
//! policy; combat gets a small exact-in-the-abstraction current-hand search.
//! The abstraction covers only direct damage/block cards. It never reads a
//! private successor or treats a search miss as evidence that a line is bad.

use super::*;
use serde::{Deserialize, Serialize};

pub const PUBLIC_RULE_SEARCH_TASK_ACTOR_NAME_V1: &str = "public_rule_search_task_actor";
pub const PUBLIC_RULE_SEARCH_TASK_ACTOR_REVISION_V1: u32 = 4;

#[derive(Clone, Default)]
pub struct PublicRuleSearchTaskActorV1 {
    strategic_and_fallback: PublicDeterministicTaskActorV1,
    stats: PublicRuleSearchStatsV1,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicRuleSearchStatsV1 {
    pub combat_decisions: u64,
    pub direct_search_choices: u64,
    pub potion_reserve_choices: u64,
    pub self_damage_guard_choices: u64,
}

impl PublicRuleSearchTaskActorV1 {
    pub fn stats(&self) -> &PublicRuleSearchStatsV1 {
        &self.stats
    }
}

impl TaskActorV1 for PublicRuleSearchTaskActorV1 {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only(
            PUBLIC_RULE_SEARCH_TASK_ACTOR_NAME_V1,
            PUBLIC_RULE_SEARCH_TASK_ACTOR_REVISION_V1,
        )
    }

    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
        let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
            return self.strategic_and_fallback.next_intent(view);
        };
        let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
            return self.strategic_and_fallback.next_intent(view);
        };
        let Ok(fallback) = public_deterministic_task_decision_v1(decision) else {
            return self.strategic_and_fallback.next_intent(view);
        };
        self.stats.combat_decisions += 1;
        let baseline_ordinal = fallback.choice.ordinal_path[0];
        let Some(ordinal) = searched_combat_ordinal(observation, decision, baseline_ordinal) else {
            return TaskActorIntentV1::Act {
                choice: fallback.choice,
            };
        };
        match complete_task_choice_v1(decision, ordinal) {
            Ok(choice) if decision.validate_choice(&choice).is_ok() => {
                if ordinal != baseline_ordinal {
                    if lethal_self_damage(
                        CombatBaselineViewV1::from_task(observation),
                        &decision.candidates[baseline_ordinal],
                    ) {
                        self.stats.self_damage_guard_choices += 1;
                    } else if should_reserve_potion(CombatBaselineViewV1::from_task(observation))
                        && matches!(
                            decision.candidates[baseline_ordinal],
                            TaskPublicCandidateV1::CombatAtomic {
                                action: PublicCombatAtomicActionV1::UsePotion { .. }
                            }
                        )
                    {
                        self.stats.potion_reserve_choices += 1;
                    } else {
                        self.stats.direct_search_choices += 1;
                    }
                }
                TaskActorIntentV1::Act { choice }
            }
            _ => TaskActorIntentV1::Act {
                choice: fallback.choice,
            },
        }
    }
}

pub type PublicRuleSearchStructuredActorV1 = TaskPublicCombatPlanFollowerActorV1<
    VulnerableSetupPayoffPlanGeneratorV1,
    PublicRuleSearchTaskActorV1,
>;

pub fn public_rule_search_task_actor_v1() -> PublicRuleSearchStructuredActorV1 {
    TaskPublicCombatPlanFollowerActorV1::new(
        TaskActorIdentityV1::task_actor_view_only(
            PUBLIC_RULE_SEARCH_TASK_ACTOR_NAME_V1,
            PUBLIC_RULE_SEARCH_TASK_ACTOR_REVISION_V1,
        ),
        VulnerableSetupPayoffPlanGeneratorV1,
        PublicRuleSearchTaskActorV1::default(),
    )
}

#[derive(Clone, Copy)]
struct DirectAction {
    ordinal: usize,
    cost: u8,
    target: Option<usize>,
    damage: i32,
    block: i32,
    all_enemies: bool,
    is_attack: bool,
}

#[derive(Clone)]
struct DirectPlan {
    remaining: Vec<i32>,
    block: i32,
    hp: i32,
    damage: i32,
    actions: usize,
    first: Option<usize>,
}

#[derive(Clone, Copy)]
struct DirectOutcome {
    score: i64,
    survives: bool,
    clears: bool,
}

fn direct_card(card: CardId) -> bool {
    matches!(
        card,
        CardId::Strike | CardId::Defend | CardId::Cleave | CardId::IronWave | CardId::HeavyBlade
    )
}

fn searched_combat_ordinal(
    observation: &crate::agent::information::state::PublicCombatStateV1,
    decision: &TaskPublicDecisionV1,
    fallback_ordinal: usize,
) -> Option<usize> {
    let view = CombatBaselineViewV1::from_task(observation);
    let admitted = admitted_task_combat_candidate_ordinals_v1(observation, &decision.candidates);
    let incoming = visible_incoming_damage(view)
        .saturating_sub(view.player.block)
        .max(0);
    let reserve_potion = should_reserve_potion(view);
    let fallback_ordinal = if (reserve_potion
        && matches!(
            decision.candidates.get(fallback_ordinal),
            Some(TaskPublicCandidateV1::CombatAtomic {
                action: PublicCombatAtomicActionV1::UsePotion { .. }
            })
        ))
        || lethal_self_damage(view, &decision.candidates[fallback_ordinal])
    {
        best_ordinal(admitted.iter().copied().filter_map(|ordinal| {
            let candidate = &decision.candidates[ordinal];
            if lethal_self_damage(view, candidate)
                || (reserve_potion
                    && matches!(
                        candidate,
                        TaskPublicCandidateV1::CombatAtomic {
                            action: PublicCombatAtomicActionV1::UsePotion { .. }
                        }
                    ))
            {
                None
            } else {
                Some((
                    ordinal,
                    combat_candidate_score(view, CombatBaselineCandidateV1::from_task(candidate)),
                ))
            }
        }))
    } else {
        fallback_ordinal
    };

    // Group by visible hand occurrence. A card with several legal targets is
    // one choice within its group, so it cannot be used twice in the plan.
    let mut groups = vec![Vec::new(); view.cards.hand.cards.len()];
    for ordinal in admitted {
        let Some(TaskPublicCandidateV1::CombatAtomic {
            action:
                PublicCombatAtomicActionV1::PlayCard {
                    hand_index,
                    target_monster_index,
                },
        }) = decision.candidates.get(ordinal)
        else {
            continue;
        };
        let Some(card) = view.cards.hand.cards.get(*hand_index) else {
            continue;
        };
        if !direct_card(card.card_id) || card.effective_cost < 0 {
            continue;
        }
        let cost = if card.free_to_play_once {
            0
        } else {
            card.effective_cost as u8
        };
        if cost > view.turn.energy {
            continue;
        }
        let definition = get_card_definition(card.card_id);
        let damage = target_monster_index
            .and_then(|target| card.damage_by_monster_order.get(target).copied())
            .unwrap_or(card.current_damage)
            .max(0);
        groups[*hand_index].push(DirectAction {
            ordinal,
            cost,
            target: *target_monster_index,
            damage,
            block: card.current_block.max(0),
            all_enemies: definition.is_multi_damage,
            is_attack: definition.card_type == CardType::Attack,
        });
    }
    groups.retain(|group| !group.is_empty());
    if groups.is_empty() || groups.len() > 8 {
        return Some(fallback_ordinal);
    }
    let initial = DirectPlan {
        remaining: view
            .monsters
            .iter()
            .map(|monster| {
                if monster.alive {
                    monster.hp.saturating_add(monster.block).max(0)
                } else {
                    0
                }
            })
            .collect(),
        block: view.player.block.max(0),
        hp: view.player.hp,
        damage: 0,
        actions: 0,
        first: None,
    };
    let mut best = (direct_plan_outcome(view, &initial), 0, None);
    let mut visits = 0_usize;
    let completed = if visible_sharp_hide_on_attack(view, CardType::Attack) > 0 {
        // Block before attack and the order of kills both matter when attacks
        // trigger retaliation. Explore order only for this order-sensitive case.
        search_direct_ordered(
            view,
            &groups,
            0,
            view.turn.energy,
            &initial,
            &mut best,
            &mut visits,
        )
    } else {
        search_direct_groups(
            view,
            &groups,
            0,
            view.turn.energy,
            &initial,
            &mut best,
            &mut visits,
        )
    };
    if !completed {
        return Some(fallback_ordinal);
    }
    let Some(first) = best.2 else {
        return Some(fallback_ordinal);
    };
    if lethal_self_damage(view, &decision.candidates[first]) {
        return Some(fallback_ordinal);
    }
    // A one-turn abstraction cannot value attack tempo against later buffs or
    // damage. Use it only for a visible lethal rescue or a full immediate kill;
    // ordinary turns remain with the strategic/tactical rule policy.
    let visible_lethal = incoming >= view.player.hp;
    if best.0.clears || (visible_lethal && best.0.survives) {
        Some(first)
    } else {
        Some(fallback_ordinal)
    }
}

fn should_reserve_potion(view: CombatBaselineViewV1<'_>) -> bool {
    let exposed = visible_incoming_damage(view)
        .saturating_sub(view.player.block)
        .max(0);
    !view.encounter.is_boss_fight
        && !view.encounter.is_elite_fight
        && view.player.hp > view.player.max_hp / 2
        && exposed <= (view.player.max_hp / 10).max(4)
}

fn lethal_self_damage(view: CombatBaselineViewV1<'_>, candidate: &TaskPublicCandidateV1) -> bool {
    let TaskPublicCandidateV1::CombatAtomic {
        action: PublicCombatAtomicActionV1::PlayCard { hand_index, .. },
    } = candidate
    else {
        return false;
    };
    let Some(card) = view.cards.hand.cards.get(*hand_index) else {
        return false;
    };
    let cost: i32 = match card.card_id {
        CardId::Bloodletting => 3,
        CardId::Hemokinesis => 2,
        CardId::Offering => 6,
        _ => 0,
    };
    let retaliation =
        visible_sharp_hide_on_attack(view, get_card_definition(card.card_id).card_type)
            .saturating_sub(view.player.block)
            .max(0);
    cost.saturating_add(retaliation) >= view.player.hp
}

fn search_direct_groups(
    view: CombatBaselineViewV1<'_>,
    groups: &[Vec<DirectAction>],
    index: usize,
    energy: u8,
    plan: &DirectPlan,
    best: &mut (DirectOutcome, usize, Option<usize>),
    visits: &mut usize,
) -> bool {
    *visits += 1;
    if *visits > 20_000 {
        return false;
    }
    if index == groups.len() {
        consider_direct_plan(view, plan, best);
        return true;
    }
    if !search_direct_groups(view, groups, index + 1, energy, plan, best, visits) {
        return false;
    }
    for action in &groups[index] {
        if action.cost > energy {
            continue;
        }
        let Some(next) = advance_direct_plan(view, plan, action) else {
            continue;
        };
        if !search_direct_groups(
            view,
            groups,
            index + 1,
            energy - action.cost,
            &next,
            best,
            visits,
        ) {
            return false;
        }
    }
    true
}

fn search_direct_ordered(
    view: CombatBaselineViewV1<'_>,
    groups: &[Vec<DirectAction>],
    used: u8,
    energy: u8,
    plan: &DirectPlan,
    best: &mut (DirectOutcome, usize, Option<usize>),
    visits: &mut usize,
) -> bool {
    *visits += 1;
    if *visits > 20_000 {
        return false;
    }
    consider_direct_plan(view, plan, best);
    for (index, group) in groups.iter().enumerate() {
        let bit = 1_u8 << index;
        if used & bit != 0 {
            continue;
        }
        for action in group {
            if action.cost > energy {
                continue;
            }
            let Some(next) = advance_direct_plan(view, plan, action) else {
                continue;
            };
            if !search_direct_ordered(
                view,
                groups,
                used | bit,
                energy - action.cost,
                &next,
                best,
                visits,
            ) {
                return false;
            }
        }
    }
    true
}

fn consider_direct_plan(
    view: CombatBaselineViewV1<'_>,
    plan: &DirectPlan,
    best: &mut (DirectOutcome, usize, Option<usize>),
) {
    let outcome = direct_plan_outcome(view, plan);
    if outcome.score > best.0.score
        || (outcome.score == best.0.score && (plan.actions, plan.first) < (best.1, best.2))
    {
        *best = (outcome, plan.actions, plan.first);
    }
}

fn advance_direct_plan(
    view: CombatBaselineViewV1<'_>,
    plan: &DirectPlan,
    action: &DirectAction,
) -> Option<DirectPlan> {
    let mut next = plan.clone();
    next.actions += 1;
    next.first.get_or_insert(action.ordinal);
    if action.is_attack {
        // Sharp Hide resolves before this card's block and damage. A line that
        // dies here cannot collect the damage from this or later cards.
        let retaliation = view
            .monsters
            .iter()
            .zip(&next.remaining)
            .filter(|(_, remaining)| **remaining > 0)
            .flat_map(|(monster, _)| monster.powers.iter())
            .filter(|power| power.power == PowerId::SharpHide)
            .fold(0_i32, |total, power| {
                total.saturating_add(power.amount.max(0))
            });
        let absorbed = next.block.min(retaliation);
        next.block -= absorbed;
        next.hp = next.hp.saturating_sub(retaliation - absorbed);
        if next.hp <= 0 {
            return None;
        }
    }
    next.block = next.block.saturating_add(action.block);
    if action.all_enemies {
        for remaining in &mut next.remaining {
            let dealt = (*remaining).min(action.damage);
            *remaining -= dealt;
            next.damage += dealt;
        }
    } else if let Some(target) = action.target {
        if let Some(remaining) = next.remaining.get_mut(target) {
            let dealt = (*remaining).min(action.damage);
            *remaining -= dealt;
            next.damage += dealt;
        }
    }
    Some(next)
}

fn direct_plan_outcome(view: CombatBaselineViewV1<'_>, plan: &DirectPlan) -> DirectOutcome {
    let mut incoming = 0_i32;
    let mut kills = 0_i64;
    let mut living = 0_usize;
    for (index, monster) in view.monsters.iter().enumerate() {
        if !monster.alive {
            continue;
        }
        if plan.remaining[index] <= 0 {
            kills += 1;
        } else {
            living += 1;
            incoming = incoming.saturating_add(visible_monster_attack_damage(monster));
        }
    }
    let future_loss = incoming.saturating_sub(plan.block).max(0);
    let loss = view
        .player
        .hp
        .saturating_sub(plan.hp)
        .saturating_add(future_loss);
    let death = future_loss >= plan.hp && living > 0;
    DirectOutcome {
        score: i64::from(plan.damage) * 8 + kills * 120 - i64::from(loss) * 40
            + if living == 0 { 100_000 } else { 0 }
            - if death { 200_000 } else { 0 },
        survives: !death,
        clears: living == 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::task::task_public_combat_decision_v1;
    use crate::content::monsters::EnemyId;
    use crate::eval::run_control::{
        LearningBoundaryV1, LearningEnvV3, RunControlConfig, RunControlSession,
    };
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{ActiveCombat, CombatContext, EngineState, RoomCombatContext};
    use crate::state::map::node::RoomType;

    fn combat_decision(
        hand: &[CardId],
        hp: i32,
        enemy_hp: i32,
        attack: i32,
        energy: u8,
    ) -> TaskPublicDecisionV1 {
        combat_decision_for_enemy(hand, hp, enemy_hp, attack, energy, EnemyId::JawWorm)
    }

    fn combat_decision_for_enemy(
        hand: &[CardId],
        hp: i32,
        enemy_hp: i32,
        attack: i32,
        energy: u8,
        enemy: EnemyId,
    ) -> TaskPublicDecisionV1 {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = hand
            .iter()
            .enumerate()
            .map(|(index, card)| CombatCard::for_scenario(*card, index as u32 + 11))
            .collect();
        combat
            .entities
            .monsters
            .push(crate::test_support::test_monster(enemy));
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let LearningBoundaryV1::Combat { boundary } =
            LearningEnvV3::from_session(session).observe().unwrap()
        else {
            panic!("expected combat");
        };
        let mut observation = boundary.observation;
        observation.player.hp = hp;
        observation.turn.energy = energy;
        observation.monsters[0].hp = enemy_hp;
        observation.monsters[0].block = 0;
        observation.monsters[0].intent.intent = Some(Intent::Attack {
            damage: attack,
            hits: 1,
        });
        observation.monsters[0].intent.preview_damage_per_hit = Some(attack);
        task_public_combat_decision_v1(
            boundary.public_run_context,
            observation,
            &boundary.public_actions,
        )
        .unwrap()
    }

    #[test]
    fn direct_card_surface_excludes_draw_and_self_damage() {
        assert!(direct_card(CardId::Defend));
        assert!(direct_card(CardId::IronWave));
        assert!(!direct_card(CardId::PommelStrike));
        assert!(!direct_card(CardId::Bloodletting));
    }

    #[test]
    fn direct_search_takes_a_visible_two_strike_kill_over_blocking() {
        let decision = combat_decision(
            &[CardId::Defend, CardId::Strike, CardId::Strike],
            12,
            12,
            18,
            2,
        );
        let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
            panic!()
        };
        let fallback = public_deterministic_task_decision_v1(&decision).unwrap();
        let selected =
            searched_combat_ordinal(observation, &decision, fallback.choice.ordinal_path[0])
                .unwrap();
        assert!(
            matches!(
                &decision.candidates[selected],
                TaskPublicCandidateV1::CombatAtomic {
                    action: PublicCombatAtomicActionV1::PlayCard {
                        hand_index: 1 | 2,
                        ..
                    }
                }
            ),
            "selected {:?}; hand {:?}",
            decision.candidates[selected],
            observation.cards.hand.cards
        );
    }

    #[test]
    fn direct_search_does_not_play_redundant_block_before_a_kill() {
        let decision = combat_decision(&[CardId::Defend, CardId::Strike], 12, 6, 18, 2);
        let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
            panic!()
        };
        let fallback = public_deterministic_task_decision_v1(&decision).unwrap();
        let selected =
            searched_combat_ordinal(observation, &decision, fallback.choice.ordinal_path[0])
                .unwrap();
        assert!(matches!(
            &decision.candidates[selected],
            TaskPublicCandidateV1::CombatAtomic {
                action: PublicCombatAtomicActionV1::PlayCard { hand_index: 1, .. }
            }
        ));
    }

    #[test]
    fn ordinary_turn_keeps_rule_policy_outside_the_search_horizon() {
        let decision = combat_decision(&[CardId::Defend, CardId::Strike], 60, 42, 6, 3);
        let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
            panic!()
        };
        let fallback = public_deterministic_task_decision_v1(&decision).unwrap();
        assert_eq!(
            searched_combat_ordinal(observation, &decision, fallback.choice.ordinal_path[0]),
            Some(fallback.choice.ordinal_path[0])
        );
    }

    #[test]
    fn self_lethal_bloodletting_is_excluded_from_legal_fallback() {
        let decision = combat_decision(&[CardId::Bloodletting], 2, 42, 0, 3);
        let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
            panic!()
        };
        let view = CombatBaselineViewV1::from_task(observation);
        assert!(decision
            .candidates
            .iter()
            .any(|candidate| lethal_self_damage(view, candidate)));
        let fallback = public_deterministic_task_decision_v1(&decision).unwrap();
        let selected =
            searched_combat_ordinal(observation, &decision, fallback.choice.ordinal_path[0])
                .unwrap();
        assert!(!lethal_self_damage(view, &decision.candidates[selected]));
    }

    #[test]
    fn visible_sharp_hide_changes_attack_tradeoff_and_guards_immediate_death() {
        let mut decision = combat_decision_for_enemy(
            &[CardId::Strike, CardId::Defend],
            20,
            206,
            6,
            2,
            EnemyId::TheGuardian,
        );
        let TaskPublicObservationV1::Combat { observation, .. } = &mut decision.observation else {
            panic!("expected combat observation")
        };
        observation.monsters[0].powers.push(
            crate::agent::information::state::CombatLearningPowerV1 {
                power: PowerId::SharpHide,
                amount: 3,
                extra_data: 0,
                just_applied: false,
                payload_card: None,
                source_creature: None,
            },
        );
        let strike = CombatBaselineCandidateV1::PlayCard {
            hand_index: 0,
            target_monster_index: Some(0),
        };
        let defend = CombatBaselineCandidateV1::PlayCard {
            hand_index: 1,
            target_monster_index: None,
        };
        let view = CombatBaselineViewV1::from_task(observation);
        assert!(
            combat_atomic_score(view, defend) > combat_atomic_score(view, strike),
            "defend={} strike={} retaliation={} incoming={} block={}",
            combat_atomic_score(view, defend),
            combat_atomic_score(view, strike),
            visible_sharp_hide_on_attack(view, CardType::Attack),
            visible_incoming_damage(view),
            view.player.block,
        );
        let strike_candidate = decision
            .candidates
            .iter()
            .find(|candidate| {
                matches!(
                    candidate,
                    TaskPublicCandidateV1::CombatAtomic {
                        action: PublicCombatAtomicActionV1::PlayCard {
                            hand_index: 0,
                            target_monster_index: Some(0)
                        }
                    }
                )
            })
            .expect("Strike candidate");
        observation.player.hp = 3;
        assert!(lethal_self_damage(
            CombatBaselineViewV1::from_task(observation),
            strike_candidate
        ));
        observation.player.block = 3;
        assert!(!lethal_self_damage(
            CombatBaselineViewV1::from_task(observation),
            strike_candidate
        ));
        observation.player.block = 0;
        observation.monsters[0].hp = 6;
        let defend_ordinal = decision
            .candidates
            .iter()
            .position(|candidate| {
                matches!(
                    candidate,
                    TaskPublicCandidateV1::CombatAtomic {
                        action: PublicCombatAtomicActionV1::PlayCard {
                            hand_index: 1,
                            target_monster_index: None
                        }
                    }
                )
            })
            .expect("Defend candidate");
        let current_decision = TaskPublicDecisionV1::new(
            decision.site,
            decision.observation.clone(),
            decision.candidates.clone(),
        )
        .expect("current public decision");
        let TaskPublicObservationV1::Combat {
            observation: current_observation,
            ..
        } = &current_decision.observation
        else {
            unreachable!()
        };
        assert_eq!(
            searched_combat_ordinal(current_observation, &current_decision, defend_ordinal),
            Some(defend_ordinal),
            "a projected clear cannot play a publicly lethal Sharp Hide attack first"
        );
    }

    #[test]
    fn visible_sharp_hide_rejects_a_clear_that_dies_on_the_second_attack() {
        let mut decision = combat_decision_for_enemy(
            &[CardId::Strike, CardId::Strike],
            5,
            12,
            0,
            2,
            EnemyId::TheGuardian,
        );
        let TaskPublicObservationV1::Combat { observation, .. } = &mut decision.observation else {
            panic!("expected combat observation")
        };
        observation.monsters[0].powers.push(
            crate::agent::information::state::CombatLearningPowerV1 {
                power: PowerId::SharpHide,
                amount: 3,
                extra_data: 0,
                just_applied: false,
                payload_card: None,
                source_creature: None,
            },
        );
        let end_turn = decision
            .candidates
            .iter()
            .position(|candidate| {
                matches!(
                    candidate,
                    TaskPublicCandidateV1::CombatAtomic {
                        action: PublicCombatAtomicActionV1::EndTurn
                    }
                )
            })
            .expect("EndTurn candidate");
        let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
            unreachable!()
        };
        assert_eq!(
            searched_combat_ordinal(observation, &decision, end_turn),
            Some(end_turn),
            "a two-hit clear that dies before its second hit is not a win"
        );
    }

    #[test]
    fn visible_sharp_hide_search_can_block_before_an_earlier_hand_attack() {
        let mut decision = combat_decision_for_enemy(
            &[CardId::Strike, CardId::Defend],
            3,
            6,
            0,
            2,
            EnemyId::TheGuardian,
        );
        let TaskPublicObservationV1::Combat { observation, .. } = &mut decision.observation else {
            panic!("expected combat observation")
        };
        observation.monsters[0].powers.push(
            crate::agent::information::state::CombatLearningPowerV1 {
                power: PowerId::SharpHide,
                amount: 3,
                extra_data: 0,
                just_applied: false,
                payload_card: None,
                source_creature: None,
            },
        );
        let end_turn = decision
            .candidates
            .iter()
            .position(|candidate| {
                matches!(
                    candidate,
                    TaskPublicCandidateV1::CombatAtomic {
                        action: PublicCombatAtomicActionV1::EndTurn
                    }
                )
            })
            .expect("EndTurn candidate");
        let defend = decision
            .candidates
            .iter()
            .position(|candidate| {
                matches!(
                    candidate,
                    TaskPublicCandidateV1::CombatAtomic {
                        action: PublicCombatAtomicActionV1::PlayCard {
                            hand_index: 1,
                            target_monster_index: None
                        }
                    }
                )
            })
            .expect("Defend candidate");
        let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
            unreachable!()
        };
        assert_eq!(
            searched_combat_ordinal(observation, &decision, end_turn),
            Some(defend),
            "visible block-then-attack kill must not depend on hand order"
        );
    }

    #[test]
    fn no_sharp_hide_does_not_reward_cards_for_spare_block() {
        let mut decision =
            combat_decision_for_enemy(&[CardId::TrueGrit], 20, 20, 0, 1, EnemyId::JawWorm);
        let TaskPublicObservationV1::Combat { observation, .. } = &mut decision.observation else {
            panic!("expected combat observation")
        };
        let action = CombatBaselineCandidateV1::PlayCard {
            hand_index: 0,
            target_monster_index: None,
        };
        observation.player.block = 0;
        let unblocked = combat_atomic_score(CombatBaselineViewV1::from_task(observation), action);
        observation.player.block = 3;
        let spare_block = combat_atomic_score(CombatBaselineViewV1::from_task(observation), action);
        assert_eq!(spare_block, unblocked);
    }
}
