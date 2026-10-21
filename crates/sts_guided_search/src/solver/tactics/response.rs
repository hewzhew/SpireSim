//! Small current-turn response hypotheses. Requests nominate inputs; only the
//! ordinary charged engine path supplies effects, draws, legality and survival.
use super::*;
use sts_core::runtime::combat::CombatCard;
use sts_core::sim::combat_projection::project_player_turn_end_requests;

#[derive(Clone, PartialEq, Eq)]
struct Play {
    lineage: CardLineageId,
    card: CardId,
    target: Option<sts_core::EntityId>,
}

pub(in crate::solver) struct ResponsePlan {
    turn: u32,
    prefix: VecDeque<Play>,
}

fn needs_response(node: &Node) -> bool {
    let c = &node.state.position().combat;
    let end = project_player_turn_end_requests(c);
    let attack: i64 = c
        .entities
        .monsters
        .iter()
        .filter(|m| m.current_hp > 0 && !m.half_dead && !m.is_escaped)
        .map(|m| {
            i64::from(project_monster_attack_request_in_combat(c, m).map_or(0, |r| r.total_damage))
        })
        .sum();
    attack + end.blockable_self_damage > i64::from(end.block_after_pre_card_powers.max(0))
}

/// Eligibility for an actual defensive play, not an amount of credited
/// protection. Artifact and reactive effects are settled only by the engine.
pub(in crate::solver) fn attack_debuff_request(c: &CombatState, action: &Action) -> bool {
    matches!(*action, Action::ApplyPower { target, power_id, amount, .. }
        if ((power_id == PowerId::Strength && amount < 0) || (power_id == PowerId::Weak && amount > 0))
            && c.entities.monsters.iter().any(|m| m.id == target && m.current_hp > 0 && !m.half_dead && !m.is_escaped
                && project_monster_attack_request_in_combat(c, m).is_some_and(|a| a.total_damage > 0)))
}

fn play(c: &CombatState, index: usize, target: Option<sts_core::EntityId>) -> Play {
    let card = &c.zones.hand[index];
    Play {
        lineage: card.lineage,
        card: card.id,
        target,
    }
}

fn input(c: &CombatState, source: &Play) -> Option<ClientInput> {
    let mut matches = c
        .zones
        .hand
        .iter()
        .enumerate()
        .filter(|(_, card)| card.lineage == source.lineage && card.id == source.card);
    let (card_index, _) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(ClientInput::PlayCard {
        card_index,
        target: source.target,
    })
}

fn protective_play(node: &Node, debuff_only: bool) -> Option<Play> {
    let c = &node.state.position().combat;
    if node.state.position().engine != EngineState::CombatPlayerTurn || !needs_response(node) {
        return None;
    }
    let mut nominated = vec![];
    for action in EngineCombatStepper.atomic_actions(node.state.position()) {
        if let ClientInput::PlayCard {
            card_index: i,
            target,
        } = action
        {
            let card = &c.zones.hand[i];
            let requests = resolve_card_play(card.id, c, card, target);
            let debuff = requests.iter().any(|r| attack_debuff_request(c, &r.action));
            let block = !debuff_only && requests.iter().any(|r| matches!(r.action,
                Action::GainBlock { target, amount } if target == c.entities.player.id && amount > 0));
            if debuff || block {
                nominated.push(((usize::from(!debuff), i, target), play(c, i, target)));
            }
        }
    }
    nominated
        .into_iter()
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, p)| p)
}

fn copies_skill(c: &CombatState, card: &CombatCard) -> bool {
    get_card_definition(card.id).card_type == CardType::Skill
        && matches!(
            effective_target(card),
            CardTarget::SelfTarget | CardTarget::None
        )
        && resolve_card_play(card.id, c, card, None).iter().any(|r| {
            matches!(r.action,
            Action::ApplyPower { target, power_id: PowerId::Burst, amount, .. }
                if target == c.entities.player.id && amount > 0)
        })
}

impl ResponsePlan {
    fn propose(node: &Node) -> Vec<Self> {
        let c = &node.state.position().combat;
        if node.state.position().engine != EngineState::CombatPlayerTurn || !needs_response(node) {
            return vec![];
        }
        let Some(debuff) = protective_play(node, true) else {
            return vec![];
        };
        let Some((i, _)) = c
            .zones
            .hand
            .iter()
            .enumerate()
            .find(|(_, card)| can_play_card(card, c).is_ok() && copies_skill(c, card))
        else {
            return vec![];
        };
        let amplifier = play(c, i, None);
        let Some((j, _)) = c.zones.hand.iter().enumerate().find(|(j, card)| {
            if *j == i
                || can_play_card(card, c).is_err()
                || get_card_definition(card.id).card_type != CardType::Skill
                || !matches!(
                    effective_target(card),
                    CardTarget::SelfTarget | CardTarget::None
                )
            {
                return false;
            }
            let effects = resolve_card_play(card.id, c, card, None);
            effects
                .iter()
                .any(|r| matches!(r.action, Action::GainEnergy { amount } if amount > 0))
                && effects
                    .iter()
                    .any(|r| matches!(r.action, Action::DrawCards(amount) if amount > 0))
        }) else {
            return vec![];
        };
        let refill = play(c, j, None);
        // Two concrete hypotheses: copy the refill before responding, or use
        // one debuff first (e.g. consume Artifact) before copying the refill.
        // Neither reads a hidden draw window or asserts a protection amount.
        vec![
            Self {
                turn: c.turn.turn_count,
                prefix: VecDeque::from([amplifier.clone(), refill.clone()]),
            },
            Self {
                turn: c.turn.turn_count,
                prefix: VecDeque::from([debuff, amplifier, refill]),
            },
        ]
    }

    pub(in crate::solver) fn finished(&self, node: &Node) -> bool {
        node.state.position().engine == EngineState::CombatPlayerTurn
            && node.state.position().combat.turn.turn_count == self.turn
            && self.prefix.is_empty()
            && protective_play(node, false).is_none()
    }

    pub(super) fn next_input(&mut self, node: &Node) -> Option<ClientInput> {
        let c = &node.state.position().combat;
        if node.state.position().engine != EngineState::CombatPlayerTurn
            || c.turn.turn_count != self.turn
        {
            return None;
        }
        let source = self
            .prefix
            .pop_front()
            .or_else(|| protective_play(node, false))?;
        input(c, &source)
    }
}

impl Pass {
    pub(in crate::solver) fn offer_response_plans(
        &mut self,
        nodes: &[Node],
        stats: &mut SolverStats,
    ) {
        if !self.tactics_enabled {
            return;
        }
        for node in nodes {
            if self.tactical.len() >= self.width * 8 {
                break;
            }
            if self.tactical_jobs().any(|job| {
                matches!(job.plan, TacticalPlan::Response(_)) && job.node.state == node.state
            }) {
                continue;
            }
            for plan in ResponsePlan::propose(node) {
                if self.tactical.len() >= self.width * 8 {
                    break;
                }
                if let Some(audit) = &mut self.audit {
                    audit.note(node, "response_plan_queued", stats.work());
                }
                self.tactical.push_back(TacticalJob {
                    node: node.clone(),
                    plan: TacticalPlan::Response(plan),
                });
                stats.response_plans_started += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::runtime::combat::Power;

    fn source() -> CombatPosition {
        let mut p = super::super::super::tests::root(
            &[
                CardId::Burst,
                CardId::Adrenaline,
                CardId::PiercingWail,
                CardId::DefendG,
            ],
            18,
        );
        p.combat.zones.hand[0].upgrades = 1;
        p.combat.zones.hand[1].upgrades = 1;
        p.combat.zones.draw_pile = [
            CardId::PiercingWail,
            CardId::DefendG,
            CardId::StrikeG,
            CardId::StrikeG,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 101))
        .collect::<Vec<_>>()
        .into();
        sts_core::content::powers::store::set_powers_for(
            &mut p.combat,
            1,
            vec![Power {
                power_type: PowerId::Artifact,
                amount: 2,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        p
    }

    #[test]
    fn copy_response_executes_real_draws_artifact_and_new_debuffs() {
        let root = source();
        let node = Node::root(CombatSearchState::new(root.clone()).unwrap());
        let plans = ResponsePlan::propose(&node);
        assert_eq!(plans.len(), 2);
        for mut plan in plans {
            let mut current = node.clone();
            let mut inputs = vec![];
            for _ in 0..16 {
                if plan.finished(&current) {
                    break;
                }
                let input = plan
                    .next_input(&current)
                    .expect("a real response must remain bound to its hand source");
                assert!(EngineCombatStepper.is_legal_action(current.state.position(), &input));
                let next = EngineCombatStepper.apply_to_stable(
                    current.state.position(),
                    input.clone(),
                    CombatStepLimits {
                        max_engine_steps: 512,
                        deadline: None,
                    },
                );
                assert!(!next.truncated && !next.timed_out);
                current.state = CombatSearchState::new(next.position).unwrap();
                inputs.push(input);
            }
            assert!(plan.finished(&current));
            let result = current.state.position();
            assert_eq!(result.combat.get_power(1, PowerId::Artifact), 0);
            assert!(result.combat.get_power(1, PowerId::Strength) < 0);
            assert!(
                result
                    .combat
                    .zones
                    .exhaust_pile
                    .iter()
                    .filter(|c| c.id == CardId::PiercingWail)
                    .count()
                    >= 2
            );
            inputs.push(ClientInput::EndTurn);
            let replayed = crate::shuffle_order::replay(&root, &inputs)
                .unwrap()
                .pop()
                .unwrap();
            assert_eq!(
                replayed.combat.entities.player.current_hp,
                root.combat.entities.player.current_hp
            );
        }
        assert_eq!(node.state.position(), &root);
    }

    #[test]
    fn copy_response_does_not_rebind_sources_or_cross_turns() {
        let root = source();
        let mut targeted = root.clone();
        targeted
            .combat
            .zones
            .hand
            .insert(0, CombatCard::for_scenario(CardId::LegSweep, 800));
        targeted
            .combat
            .zones
            .hand
            .insert(0, CombatCard::for_scenario(CardId::SpotWeakness, 801));
        assert_eq!(
            ResponsePlan::propose(&Node::root(CombatSearchState::new(targeted).unwrap())).len(),
            2,
            "classifying self-target preparation must not resolve enemy cards without a target"
        );
        let original = Node::root(CombatSearchState::new(root.clone()).unwrap());
        let mut plan = ResponsePlan::propose(&original).remove(0);
        let mut replaced = root.clone();
        replaced.combat.zones.hand[0].lineage =
            sts_core::runtime::combat::CardLineageId::constructed_scenario(888);
        let replaced = Node::root(CombatSearchState::new(replaced).unwrap());
        assert!(
            plan.next_input(&replaced).is_none(),
            "the same card name is not its source"
        );
        let mut plan = ResponsePlan::propose(&original).remove(0);
        let mut later = root.clone();
        later.combat.turn.turn_count += 1;
        let later = Node::root(CombatSearchState::new(later).unwrap());
        assert!(plan.next_input(&later).is_none());
        assert!(!plan.finished(&later));
        let mut pending = root;
        pending
            .combat
            .zones
            .hand
            .retain(|c| c.id != CardId::Adrenaline);
        assert!(
            ResponsePlan::propose(&Node::root(CombatSearchState::new(pending).unwrap())).is_empty()
        );
    }

    #[test]
    fn copy_response_shares_budget_and_resumes_one_work_at_a_time() {
        let root = source();
        let config = SolverConfig {
            max_width: 4,
            max_potion_uses: Some(0),
            ..Default::default()
        };
        let mut whole = CombatSolver::new(root.clone(), config.clone()).unwrap();
        let mut chunks = CombatSolver::new(root.clone(), config.clone()).unwrap();
        whole.advance(2000, Duration::from_secs(30)).unwrap();
        for _ in 0..2000 {
            let before = chunks.stats.work();
            chunks.advance(1, Duration::from_secs(30)).unwrap();
            assert!(chunks.stats.work() <= before + 1);
            assert!(chunks
                .beams
                .iter()
                .flat_map(|b| &b.passes)
                .all(|p| p.tactical.len() <= p.width * 8));
        }
        assert!(whole.stats.response_plans_started > 0);
        assert!(whole.stats.response_plan_transitions > 0);
        assert!(whole.stats.response_plans_completed > 0);
        assert_eq!(
            serde_json::to_value(&whole.stats).unwrap(),
            serde_json::to_value(&chunks.stats).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&whole.plans).unwrap(),
            serde_json::to_value(&chunks.plans).unwrap()
        );
        for plan in whole.plans() {
            assert_eq!(
                crate::shuffle_order::replay(&root, &plan.inputs)
                    .unwrap()
                    .last(),
                Some(&plan.successor)
            );
        }
        let mut disabled = CombatSolver::new(
            root,
            SolverConfig {
                tactical_attacks: false,
                ..config
            },
        )
        .unwrap();
        disabled.advance(2000, Duration::from_secs(30)).unwrap();
        assert_eq!(disabled.stats.response_plans_started, 0);
    }

    #[test]
    fn paused_responses_remain_queued_for_nomination_deduplication() {
        let node = Node::root(CombatSearchState::new(source()).unwrap());
        let mut pass = Pass::new(node.clone(), 1, false);
        pass.tactics_enabled = true;
        let mut stats = SolverStats::default();
        pass.offer_response_plans(std::slice::from_ref(&node), &mut stats);
        assert_eq!(stats.response_plans_started, 2);
        pass.defer_turn(&mut stats);
        assert_eq!(pass.tactical_jobs().count(), 2);
        pass.offer_response_plans(&[node], &mut stats);
        assert!(pass.tactical.is_empty());
        assert_eq!(stats.response_plans_started, 2);
    }
}
