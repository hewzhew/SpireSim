//! A bounded set of effect-order hypotheses at the solver's exact root.
//! These are deliberately different recipes, not a new evaluator or policy.
use super::*;
use crate::observation::consequences::{EffectMilestone, PlanProposal};
use crate::observation::{monster_decision, MonsterFactId};
use sts_core::content::cards::can_upgrade_card_once;
use sts_core::sim::combat_projection::monster_has_visible_attack_in_combat;

#[derive(Clone, Copy)]
enum Purpose {
    Amplify,
    DelayAmplification,
    AvoidRetaliation,
}

impl Purpose {
    fn label(self) -> &'static str {
        match self {
            Self::Amplify => "prepare-amplify",
            Self::DelayAmplification => "prepare-delay-amplification",
            Self::AvoidRetaliation => "prepare-avoid-retaliation",
        }
    }
}

pub(in crate::solver) struct EffectPlan {
    purpose: Purpose,
    target: sts_core::EntityId,
    source_turn: u32,
    milestones: Vec<EffectMilestone>,
}

fn supported_power(power: PowerId) -> bool {
    matches!(
        power,
        PowerId::Strength | PowerId::Dexterity | PowerId::Metallicize | PowerId::Barricade
    )
}

fn active(m: &sts_core::runtime::combat::MonsterEntity) -> bool {
    m.current_hp > 0 && !m.half_dead && !m.is_escaped
}

/// Inspect requests only; hooks and final effects remain engine-owned.
fn preparation(c: &CombatState, card: &sts_core::runtime::combat::CombatCard, a: &Action) -> bool {
    match a {
        Action::UpgradeAllInHand => c
            .zones
            .hand
            .iter()
            .any(|other| other.lineage != card.lineage && can_upgrade_card_once(other)),
        Action::UpgradeAllCardsInCombat => c
            .zones
            .hand
            .iter()
            .chain(c.zones.draw_pile.iter())
            .chain(c.zones.discard_pile.iter())
            .any(can_upgrade_card_once),
        Action::ApplyPower {
            target,
            power_id,
            amount,
            ..
        } => *target == c.entities.player.id && *amount > 0 && supported_power(*power_id),
        Action::SpotWeakness { target, amount } => {
            *amount > 0
                && c.entities.monsters.iter().any(|m| {
                    m.id == *target && active(m) && monster_has_visible_attack_in_combat(c, m)
                })
        }
        _ => false,
    }
}

fn vulnerability(target: sts_core::EntityId, a: &Action) -> bool {
    matches!(a, Action::ApplyPower { target: owner, power_id: PowerId::Vulnerable, amount, .. }
        if *owner == target && *amount > 0)
}

impl EffectPlan {
    pub(in crate::solver) fn reached_horizon(&self, node: &Node) -> bool {
        !actions::unfinished(node)
            && node.state.position().engine == EngineState::CombatPlayerTurn
            && node
                .state
                .position()
                .combat
                .turn
                .turn_count
                .saturating_sub(self.source_turn)
                >= 2
    }

    pub(in crate::solver) fn target_gone(&self, node: &Node) -> bool {
        !node
            .state
            .position()
            .combat
            .entities
            .monsters
            .iter()
            .any(|m| m.id == self.target && active(m))
    }

    pub(super) fn proposal(&self, node: &Node) -> PlanProposal {
        PlanProposal {
            label: format!("{}-target-{}", self.purpose.label(), self.target),
            inputs: node.inputs(),
            player_turns: vec![1, 2],
            milestones: self.milestones.clone(),
        }
    }

    pub(super) fn next_input(
        &self,
        node: &Node,
        choices: &crate::combat::PreparedCombatChoices,
    ) -> Option<ClientInput> {
        let p = node.state.position();
        let c = &p.combat;
        if p.engine != EngineState::CombatPlayerTurn
            || self.reached_horizon(node)
            || self.target_gone(node)
        {
            return None;
        }
        let mut nominated = vec![];
        let mut end = None;
        let mut unhandled = false;
        for branch in choices.branches() {
            let CombatBranch::Input(input) = branch else {
                continue;
            };
            let ClientInput::PlayCard { card_index, target } = input else {
                if *input == ClientInput::EndTurn {
                    end = Some(input.clone());
                }
                continue;
            };
            if target.is_some_and(|id| id != self.target) {
                continue;
            }
            let card = &c.zones.hand[*card_index];
            let effects = resolve_card_play(card.id, c, card, *target);
            let attack = get_card_definition(card.id).card_type == CardType::Attack;
            let retaliation = attack
                && c.entities.monsters.iter().filter(|m| active(m)).any(|m| {
                    c.get_power(m.id, PowerId::SharpHide) > 0
                        || (c.get_power(m.id, PowerId::Thorns) > 0
                            && effects.iter().any(|e| match &e.action {
                                Action::Damage(info) => info.target == m.id,
                                Action::Whirlwind { .. } => true,
                                _ => false,
                            }))
                });
            // Holding an attack is a specific retaliation hypothesis. It is
            // not a blanket permission to end while arbitrary cards remain.
            if matches!(self.purpose, Purpose::AvoidRetaliation) && retaliation {
                continue;
            }
            let rank = if effects.iter().any(|e| {
                matches!(
                    e.action,
                    Action::UpgradeAllInHand | Action::UpgradeAllCardsInCombat
                ) && preparation(c, card, &e.action)
            }) {
                0
            } else if effects.iter().any(|e| preparation(c, card, &e.action)) {
                1
            } else if attack {
                if effects
                    .iter()
                    .any(|e| vulnerability(self.target, &e.action))
                {
                    if matches!(self.purpose, Purpose::DelayAmplification) {
                        4
                    } else {
                        2
                    }
                } else {
                    3
                }
            } else if effects.iter().any(|e| {
                matches!(e.action, Action::GainBlock { target, amount }
                if target == c.entities.player.id && amount > 0)
            }) {
                5
            } else {
                unhandled = true;
                continue;
            };
            // Hand order is a concrete recipe tie-break, never equivalence
            // between distinct instances or discard/shuffle orders.
            nominated.push(((rank, *card_index), input.clone()));
        }
        nominated
            .into_iter()
            .min_by_key(|(rank, _)| *rank)
            .map(|(_, input)| input)
            .or_else(|| (!unhandled).then_some(end).flatten())
    }
}

impl Pass {
    pub(in crate::solver) fn offer_root_effect_plans(&mut self, stats: &mut SolverStats) {
        if !std::mem::take(&mut self.effect_root_pending) || !self.tactics_enabled {
            return;
        }
        let Some(node) = self.active.front().cloned() else {
            return;
        };
        let p = node.state.position();
        if p.engine != EngineState::CombatPlayerTurn || node.input_count != 0 {
            return;
        }
        let c = &p.combat;
        self.offer_response_plans(std::slice::from_ref(&node), stats);
        for target in c.entities.monsters.iter().filter(|m| active(m)) {
            let mode_shift = c.get_power(target.id, PowerId::ModeShift) > 0;
            let retaliation = c.get_power(target.id, PowerId::SharpHide) > 0
                || c.get_power(target.id, PowerId::Thorns) > 0;
            // Admit only these reactive-mechanism questions for now. Generic
            // preparation policy remains ordinary search work.
            if !mode_shift && !retaliation {
                continue;
            }
            let mut milestones = vec![];
            let mut can_amplify = false;
            for card in &c.zones.hand {
                if can_play_card(card, c).is_err() {
                    continue;
                }
                let effects = resolve_card_play(card.id, c, card, Some(target.id));
                can_amplify |= effects.iter().any(|e| vulnerability(target.id, &e.action));
                for e in effects {
                    let power = match e.action {
                        Action::ApplyPower {
                            target: owner,
                            power_id,
                            amount,
                            ..
                        } if owner == c.entities.player.id
                            && amount > 0
                            && supported_power(power_id) =>
                        {
                            Some(power_id)
                        }
                        Action::SpotWeakness { .. } => Some(PowerId::Strength),
                        _ => None,
                    };
                    if let Some(power) = power {
                        if !milestones.iter().any(|m| matches!(m, EffectMilestone::EstablishPlayerPower { power: old } if *old == power)) {
                            milestones.push(EffectMilestone::EstablishPlayerPower { power });
                        }
                    }
                }
            }
            if monster_decision(target)
                .is_some_and(|d| d.facts.iter().any(|f| f.fact == MonsterFactId::Open))
            {
                milestones.push(EffectMilestone::ChangeMonsterFact {
                    target: target.id,
                    fact: MonsterFactId::Open,
                });
            }
            let mut purposes = vec![Purpose::Amplify, Purpose::AvoidRetaliation];
            if mode_shift && can_amplify {
                purposes.insert(1, Purpose::DelayAmplification);
            }
            for purpose in purposes {
                if self.tactical.len() == self.width * 8 {
                    break;
                }
                self.tactical.push_back(TacticalJob {
                    node: node.clone(),
                    plan: TacticalPlan::Effect(EffectPlan {
                        purpose,
                        target: target.id,
                        source_turn: c.turn.turn_count,
                        milestones: milestones.clone(),
                    }),
                });
                stats.effect_plans_started += 1;
            }
        }
    }

    pub(in crate::solver) fn record_effect_proposal(&mut self, plan: &TacticalPlan, node: &Node) {
        if let TacticalPlan::Effect(plan) = plan {
            if node.input_count > 0 {
                self.effect_proposals.push(plan.proposal(node));
            }
        }
    }
}

impl CombatSolver {
    /// Executed root hypotheses, including incomplete prefixes stopped at a
    /// choice or quota. Reading does no engine work; these are not victory plans.
    pub fn effect_plan_proposals(&self) -> Vec<PlanProposal> {
        let mut result: Vec<PlanProposal> = vec![];
        for pass in self.beams.iter().flat_map(|b| &b.passes) {
            let suspended = pass.deferred_cursors.iter().filter_map(|cursor| {
                if let actions::DeferredCursor::Tactical(TacticalJob {
                    node,
                    plan: TacticalPlan::Effect(plan),
                }) = cursor
                {
                    (node.input_count > 0).then(|| plan.proposal(node))
                } else {
                    None
                }
            });
            for proposal in pass.effect_proposals.iter().cloned().chain(suspended) {
                if !result
                    .iter()
                    .any(|old| old.label == proposal.label && old.inputs == proposal.inputs)
                {
                    result.push(proposal);
                }
            }
        }
        for (i, proposal) in result.iter_mut().enumerate() {
            proposal.label.push_str(&format!("-{i}"));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::monsters::factory::EncounterId;
    use sts_core::runtime::combat::{CombatCard, Power};
    use sts_core::state::map::node::RoomType;

    fn cards(ids: &[(CardId, u8)], first: u32) -> Vec<CombatCard> {
        ids.iter()
            .enumerate()
            .map(|(i, &(id, upgrades))| {
                let mut card = CombatCard::for_scenario(id, first + i as u32);
                card.upgrades = upgrades;
                card
            })
            .collect()
    }

    fn root(encounter: EncounterId) -> CombatPosition {
        let mut p = crate::local::ConstructedCombat {
            seed: 42,
            encounter,
            act: 1,
            room: RoomType::MonsterRoomBoss,
            hp: 75,
            deck: vec![(CardId::Strike, 0); 12],
            relics: vec![],
            potions: vec![],
            max_potion_uses: Some(0),
        }
        .build()
        .unwrap();
        p.combat.turn.energy = 6;
        p.combat.zones.hand = cards(
            &[
                (CardId::Metallicize, 1),
                (CardId::Armaments, 1),
                (CardId::Strike, 0),
                (CardId::Bash, 1),
                (CardId::TwinStrike, 0),
            ],
            100,
        );
        p.combat.zones.draw_pile = cards(
            &[
                (CardId::SpotWeakness, 1),
                (CardId::Strike, 0),
                (CardId::Strike, 0),
                (CardId::Wound, 0),
                (CardId::Wound, 0),
                (CardId::Defend, 0),
                (CardId::Strike, 0),
                (CardId::Defend, 0),
                (CardId::Defend, 0),
                (CardId::Whirlwind, 0),
            ],
            200,
        )
        .into();
        p
    }

    fn solver(p: CombatPosition, follow_preparations: bool) -> CombatSolver {
        CombatSolver::new(
            p,
            SolverConfig {
                max_width: 4,
                max_potion_uses: Some(0),
                refine_potion_plans: false,
                refine_card_order: false,
                follow_preparations,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn power(p: &mut CombatPosition, owner: sts_core::EntityId, id: PowerId, amount: i32) {
        p.combat
            .entities
            .power_db
            .entry(owner)
            .or_default()
            .push(Power {
                power_type: id,
                amount,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            });
    }

    #[test]
    fn automatic_guardian_recipes_keep_two_turns_exact_and_resume_within_budget() {
        let p = root(EncounterId::TheGuardian);
        for follow in [false, true] {
            let mut chunks = solver(p.clone(), follow);
            chunks.advance(0, Duration::from_secs(5)).unwrap();
            chunks.advance(1, Duration::ZERO).unwrap();
            assert_eq!(chunks.stats.work(), 0);
            assert_eq!(chunks.stats.effect_plans_started, 0);
            for _ in 0..200 {
                let before = chunks.stats.work();
                chunks.advance(1, Duration::from_secs(5)).unwrap();
                assert!(chunks.stats.work() <= before + 1);
                let stats = chunks.stats();
                assert_eq!(
                    stats.effect_plans_started,
                    stats.effect_plans_completed
                        + stats.effect_plans_aborted
                        + stats.effect_plans_quota_stopped
                        + chunks
                            .progress()
                            .iter()
                            .map(|p| p.effect_plans)
                            .sum::<usize>()
                );
            }
            let mut whole = solver(p.clone(), follow);
            whole.advance(200, Duration::from_secs(5)).unwrap();
            assert_eq!(
                serde_json::to_value(chunks.stats()).unwrap(),
                serde_json::to_value(whole.stats()).unwrap()
            );
            assert_eq!(
                serde_json::to_value(chunks.effect_plan_proposals()).unwrap(),
                serde_json::to_value(whole.effect_plan_proposals()).unwrap()
            );
            assert_eq!(chunks.root(), &p);
            assert_eq!(chunks.stats.effect_plans_started, 3);
            assert_eq!(chunks.stats.effect_plans_completed, 3);
            let proposals = chunks.effect_plan_proposals();
            assert_eq!(proposals.len(), 3);
            let results = proposals
                .iter()
                .map(|proposal| {
                    crate::observation::consequences::plan_consequences(&p, proposal).unwrap()
                })
                .collect::<Vec<_>>();
            for result in &results {
                assert_eq!(result.player_turn_frames.len(), 2);
                assert!(result.unreached_player_turns.is_empty());
            }
            let last = |purpose: &str| {
                results
                    .iter()
                    .find(|r| r.label.starts_with(purpose))
                    .unwrap()
                    .frames
                    .last()
                    .unwrap()
            };
            assert!(
                last("prepare-delay").observation.hp[0] > last("prepare-amplify").observation.hp[0]
            );
            assert!(
                last("prepare-avoid").observation.hp[0] > last("prepare-amplify").observation.hp[0]
            );
            assert!(
                last("prepare-amplify").observation.monsters[0].hp[0]
                    < last("prepare-avoid").observation.monsters[0].hp[0]
            );
            // Recipes include actual dynamic indices and both end turns,
            // rather than copying the analyst's concrete input list.
            assert_eq!(
                proposals[0]
                    .inputs
                    .iter()
                    .filter(|i| **i == ClientInput::EndTurn)
                    .count(),
                2
            );
        }
    }

    #[test]
    fn retaliation_holding_uses_power_effects_and_unhandled_cards_handoff() {
        let mut p = root(EncounterId::JawWorm);
        p.combat.zones.hand = cards(&[(CardId::Strike, 0)], 100);
        let mut ordinary = solver(p.clone(), false);
        ordinary.advance(50, Duration::from_secs(5)).unwrap();
        assert_eq!(ordinary.stats.effect_plans_started, 0);
        let target = p.combat.entities.monsters[0].id;
        power(&mut p, target, PowerId::Thorns, 3);
        let mut s = solver(p.clone(), false);
        let pass = &mut s.beams[0].passes[0];
        pass.tactics_enabled = true;
        pass.offer_root_effect_plans(&mut s.stats);
        assert_eq!(pass.tactical.len(), 2);
        let avoid = &pass.tactical[1].plan;
        let TacticalPlan::Effect(avoid) = avoid else {
            panic!("missing effect recipe")
        };
        let node = pass.active[0].clone();
        let choices = s.space.prepare_choices(&node.state).unwrap();
        assert_eq!(
            avoid.next_input(&node, &choices),
            Some(ClientInput::EndTurn)
        );
        let mut unknown = p.clone();
        unknown
            .combat
            .zones
            .hand
            .push(cards(&[(CardId::Offering, 0)], 300).remove(0));
        let unknown = Node::root(CombatSearchState::new(unknown).unwrap());
        assert_eq!(
            avoid.next_input(&unknown, &s.space.prepare_choices(&unknown.state).unwrap()),
            None
        );
        let mut block = p;
        block
            .combat
            .zones
            .hand
            .push(cards(&[(CardId::Defend, 0)], 301).remove(0));
        let block = Node::root(CombatSearchState::new(block).unwrap());
        assert!(matches!(
            avoid.next_input(&block, &s.space.prepare_choices(&block.state).unwrap()),
            Some(ClientInput::PlayCard { card_index: 1, .. })
        ));
    }

    #[test]
    fn pending_choice_and_turn_quota_preserve_exact_effect_prefix() {
        let mut p = root(EncounterId::TheGuardian);
        p.combat.zones.hand = cards(&[(CardId::Headbutt, 0)], 100);
        p.combat.zones.discard_pile = cards(&[(CardId::Bash, 0), (CardId::Defend, 0)], 300).into();
        let mut s = solver(p.clone(), false);
        s.advance(50, Duration::from_secs(5)).unwrap();
        assert_eq!(s.stats.effect_plans_started, 2);
        assert_eq!(s.stats.effect_plans_aborted, 2);
        for proposal in s.effect_plan_proposals() {
            let result =
                crate::observation::consequences::plan_consequences(&p, &proposal).unwrap();
            assert!(matches!(
                result.frames.last().unwrap().boundary,
                crate::observation::consequences::ConsequenceBoundary::PendingChoice
            ));
            assert_eq!(result.unreached_player_turns, [1, 2]);
        }
        let p = root(EncounterId::TheGuardian);
        let mut s = solver(p.clone(), false);
        s.advance(1, Duration::from_secs(5)).unwrap();
        let sampled = s.sample_prefixes(10);
        let prefix = sampled
            .iter()
            .find(|p| p.pool == prefixes::SolverPrefixPool::EffectPlan)
            .unwrap();
        let saved = prefix.inputs.clone();
        s.beams[0].passes[0].defer_turn(&mut s.stats);
        assert!(s.beams[0].passes[0]
            .deferred_cursors
            .iter()
            .flat_map(actions::DeferredCursor::nodes)
            .any(|node| node.inputs() == saved));
        assert_eq!(s.stats.effect_plans_quota_stopped, 0);
        assert_eq!(s.stats.quota_cursors_saved, 3);
        assert_eq!(s.effect_plan_proposals()[0].inputs, saved);
        assert_eq!(s.stats.work(), 1);
    }

    #[test]
    fn forced_card_turn_end_is_a_real_plan_boundary_and_wider_passes_do_not_repeat_recipes() {
        let mut p = root(EncounterId::TimeEater);
        let target = p.combat.entities.monsters[0].id;
        power(&mut p, target, PowerId::Thorns, 3);
        p.combat
            .entities
            .power_db
            .values_mut()
            .flatten()
            .find(|power| power.power_type == PowerId::TimeWarp)
            .unwrap()
            .amount = 11;
        p.combat.zones.hand = cards(&[(CardId::Strike, 0)], 100);
        p.combat.zones.draw_pile = cards(&[(CardId::Wound, 0); 10], 200).into();
        let mut s = CombatSolver::new(
            p.clone(),
            SolverConfig {
                max_width: 8,
                transitions_per_width: 8,
                max_potion_uses: Some(0),
                refine_potion_plans: false,
                refine_card_order: false,
                ..Default::default()
            },
        )
        .unwrap();
        s.advance(300, Duration::from_secs(5)).unwrap();
        assert!(s.stats.passes_started >= 2);
        assert_eq!(s.stats.effect_plans_started, 2);
        let proposals = s.effect_plan_proposals();
        let amplify = proposals
            .iter()
            .find(|p| p.label.starts_with("prepare-amplify"))
            .unwrap();
        let result = crate::observation::consequences::plan_consequences(&p, amplify).unwrap();
        assert_eq!(result.player_turn_frames, [(1, 1), (2, 2)]);
        assert_eq!(amplify.inputs.len(), 2);
        assert!(matches!(amplify.inputs[0], ClientInput::PlayCard { .. }));
        assert_eq!(amplify.inputs[1], ClientInput::EndTurn);
    }

    #[test]
    fn future_recipe_boundary_waits_outside_an_earlier_turns_score_cohort() {
        let p = root(EncounterId::TheGuardian);
        let states =
            crate::shuffle_order::replay(&p, &[ClientInput::EndTurn, ClientInput::EndTurn])
                .unwrap();
        let mut pass = Pass::new(Node::root(CombatSearchState::new(p).unwrap()), 4, true);
        pass.active.clear();
        let earlier = Node::root(CombatSearchState::new(states[1].clone()).unwrap());
        let later = Node::root(CombatSearchState::new(states[2].clone()).unwrap());
        pass.ended = vec![later.clone(), earlier.clone()];
        let config = SolverConfig {
            tactical_attacks: false,
            max_potion_uses: Some(0),
            ..Default::default()
        };
        let mut stats = SolverStats::default();
        assert!(pass
            .prepare(
                &CombatSearchSpace::with_potion_limit(0),
                &config,
                &mut stats,
                None
            )
            .unwrap());
        assert_eq!(pass.expansion.as_ref().unwrap().node.state, earlier.state);
        assert_eq!(pass.ended.len(), 1);
        assert_eq!(pass.ended[0].state, later.state);
        assert_eq!(stats.turn_layers, 1);
        assert_eq!(stats.beam_dropped, 0);
    }
}
