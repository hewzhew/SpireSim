//! Short, source-bound attack hypotheses. Static arithmetic proposes a recipe;
//! every card still runs through the normal charged engine transition. Neither
//! a proposal nor its failure is an exhaustive claim about a combat position.
use super::*;
use sts_core::content::cards::{
    can_play_card, effective_target, evaluate_card_for_play, get_card_definition,
    mechanics::{
        card_base_damage_delivery_v1, CardAttackHitsV1, CardAttackTargetsV1,
        CardBaseDamageDeliveryV1,
    },
    resolve_card_play, CardId, CardTarget, CardType,
};
use sts_core::content::{powers::PowerId, relics::RelicId};
use sts_core::runtime::action::{Action, AddTo};
use sts_core::runtime::combat::{CardLineageId, CombatState};
use sts_core::sim::combat_projection::project_monster_attack_request_in_combat;
use sts_core::state::core::EngineState;

mod effects;
mod response;
pub(super) use effects::EffectPlan;
pub(super) use response::attack_debuff_request;
use response::ResponsePlan;

#[derive(Clone)]
pub(super) struct AttackPlan {
    target: sts_core::EntityId,
    turn: u32,
    cards: VecDeque<(CardLineageId, CardId)>,
}

impl AttackPlan {
    pub(super) fn achieved(&self, node: &Node) -> bool {
        let c = &node.state.position().combat;
        c.entities.player.current_hp > 0
            && c.turn.turn_count == self.turn
            && c.entities
                .monsters
                .iter()
                .find(|m| m.id == self.target)
                .is_none_or(|m| m.current_hp <= 0 || m.half_dead || m.is_escaped)
    }

    fn next_input(&mut self, node: &Node) -> Option<ClientInput> {
        let pos = node.state.position();
        if !matches!(pos.engine, EngineState::CombatPlayerTurn)
            || pos.combat.turn.turn_count != self.turn
            || self.achieved(node)
        {
            return None;
        }
        let (lineage, id) = self.cards.pop_front()?;
        let mut matches = pos
            .combat
            .zones
            .hand
            .iter()
            .enumerate()
            .filter(|(_, card)| card.lineage == lineage && card.id == id);
        let (card_index, card) = matches.next()?;
        // Ambiguous occurrences are not rebound by name or a guessed index.
        if matches.next().is_some() {
            return None;
        }
        Some(ClientInput::PlayCard {
            card_index,
            target: (effective_target(card) == CardTarget::Enemy).then_some(self.target),
        })
    }
}

pub(super) enum TacticalPlan {
    Attack(AttackPlan),
    Effect(EffectPlan),
    Response(ResponsePlan),
}

impl TacticalPlan {
    pub(super) fn is_attack(&self) -> bool {
        matches!(self, Self::Attack(_))
    }

    pub(super) fn is_response(&self) -> bool {
        matches!(self, Self::Response(_))
    }

    fn attack_target(&self) -> Option<sts_core::EntityId> {
        match self {
            Self::Attack(plan) => Some(plan.target),
            Self::Effect(_) | Self::Response(_) => None,
        }
    }

    fn next_input(
        &mut self,
        node: &Node,
        choices: &crate::combat::PreparedCombatChoices,
    ) -> Option<ClientInput> {
        match self {
            Self::Attack(plan) => plan.next_input(node),
            Self::Effect(plan) => plan.next_input(node, choices),
            Self::Response(plan) => plan.next_input(node),
        }
    }

    pub(super) fn abort(&self, stats: &mut SolverStats) {
        match self {
            Self::Attack(_) => stats.attack_plans_aborted += 1,
            Self::Effect(_) => stats.effect_plans_aborted += 1,
            Self::Response(_) => stats.response_plans_aborted += 1,
        }
    }
}

pub(super) struct TacticalJob {
    pub node: Node,
    pub plan: TacticalPlan,
}

/// One minimum-energy recipe per currently attacking enemy, then minimum card
/// count. Snapshot damage is a proposal only: draws, triggers and new debuffs
/// are not predicted. No hidden pile is read. Different roots keep exact RNG
/// and pile order; choosing one recipe does not identify alternative orders.
pub(super) fn attack_plans(node: &Node) -> Vec<AttackPlan> {
    attack_plans_excluding(node, &std::collections::HashSet::new())
}

/// A narrow current-hand protection hint using the existing fixed-hit recipe.
/// Only direct packets on this target and an optional last-play draw qualify.
/// Unpriced retaliation, shape changes, repeated plays and packet mitigation
/// stay ordinary search work; this is not a replay certificate or HP bound.
pub(super) struct ProtectionRecipe {
    pub target: sts_core::EntityId,
    pub cards: Vec<(CardLineageId, CardId)>,
    pub payment: u8,
    pub direct_only: bool,
}

pub(super) fn protection_recipes(node: &Node) -> Vec<ProtectionRecipe> {
    let c = &node.state.position().combat;
    let player = &c.entities.player;
    let has = |owner, power| sts_core::content::powers::store::has_power(c, owner, power);
    if c.zones.hand.iter().any(|card| card.id == CardId::Pain)
        || [PowerId::DoubleTap, PowerId::DuplicationPower]
            .iter()
            .any(|&power| has(player.id, power))
        || c.entities
            .monsters
            .iter()
            .any(|m| m.current_hp > 0 && has(m.id, PowerId::BeatOfDeath))
    {
        return vec![];
    }
    attack_plans(node)
        .into_iter()
        .filter_map(|plan| {
            if [
                PowerId::Buffer,
                PowerId::Intangible,
                PowerId::Invincible,
                PowerId::ModeShift,
                PowerId::CurlUp,
                PowerId::Flight,
                PowerId::Malleable,
                PowerId::Thorns,
                PowerId::SharpHide,
            ]
            .iter()
            .any(|&power| has(plan.target, power))
                || c.entities.monsters.iter().any(|m| {
                    m.current_hp > 0
                        && has(m.id, PowerId::TimeWarp)
                        && plan.cards.len()
                            >= (12 - c.get_power(m.id, PowerId::TimeWarp)).max(0) as usize
                })
            {
                return None;
            }
            let mut draws = 0;
            let reviewed = plan.cards.iter().all(|&(lineage, id)| {
                let Some(card) = c
                    .zones
                    .hand
                    .iter()
                    .find(|card| card.lineage == lineage && card.id == id)
                else {
                    return false;
                };
                resolve_card_play(card.id, c, card, Some(plan.target))
                    .iter()
                    .all(|effect| {
                        effect.insertion_mode == AddTo::Bottom
                            && match effect.action {
                                Action::Damage(ref info) => info.target == plan.target,
                                Action::DrawCards(_) => {
                                    draws += 1;
                                    true
                                }
                                _ => false,
                            }
                    })
            });
            // The chosen fixed packets can be played before one drawing
            // attack. No drawn card or newly exposed restriction is needed
            // to fund a remaining play. Several draws need their own model.
            if !reviewed || draws > 1 {
                return None;
            }
            let mut payment = 0u8;
            for &(lineage, id) in &plan.cards {
                let mut matches = c
                    .zones
                    .hand
                    .iter()
                    .filter(|card| card.lineage == lineage && card.id == id);
                let card = matches.next()?;
                if matches.next().is_some() {
                    return None;
                }
                // Match the fixed-hit proposer, conservatively reserving its
                // displayed costs. Free-play discounts are not borrowed twice.
                payment = payment.checked_add(u8::try_from(card.get_cost()).ok()?)?;
            }
            Some(ProtectionRecipe {
                target: plan.target,
                cards: plan.cards.into(),
                payment,
                direct_only: draws == 0,
            })
        })
        .collect()
}

fn attack_plans_excluding(
    node: &Node,
    represented: &std::collections::HashSet<sts_core::EntityId>,
) -> Vec<AttackPlan> {
    let pos = node.state.position();
    if !matches!(pos.engine, EngineState::CombatPlayerTurn) {
        return vec![];
    }
    attack_recipes(&pos.combat, represented, 0)
}

fn attack_recipes(
    c: &CombatState,
    represented: &std::collections::HashSet<sts_core::EntityId>,
    preceding_plays: usize,
) -> Vec<AttackPlan> {
    let hand = &c.zones.hand;
    if hand.len() > 64 {
        return vec![];
    }
    let player = &c.entities.player;
    let mut plays = hand.len();
    if hand.iter().any(|card| card.id == CardId::Normality) {
        plays = plays.min(
            3usize
                .saturating_sub(c.turn.counters.cards_played_this_turn as usize)
                .saturating_sub(preceding_plays),
        );
    }
    for relic in &player.relics {
        if relic.id == RelicId::VelvetChoker {
            plays =
                plays.min(((6 - relic.counter).max(0) as usize).saturating_sub(preceding_plays));
        }
    }
    for m in c.entities.monsters.iter().filter(|m| m.current_hp > 0) {
        if sts_core::content::powers::store::has_power(c, m.id, PowerId::TimeWarp) {
            plays = plays.min(
                ((12 - c.get_power(m.id, PowerId::TimeWarp)).max(0) as usize)
                    .saturating_sub(preceding_plays),
            );
        }
    }
    // Do not multiply a one-attack bonus across the whole proposed recipe.
    if c.get_power(player.id, PowerId::Vigor) > 0
        || c.get_power(player.id, PowerId::PenNibPower) > 0
    {
        plays = plays.min(1);
    }
    let energy = usize::from(c.turn.energy);
    let stride = energy + 1;
    let mut plans = vec![];
    for (target_index, m) in c.entities.monsters.iter().enumerate() {
        if represented.contains(&m.id)
            || m.current_hp <= 0
            || m.half_dead
            || m.is_escaped
            || project_monster_attack_request_in_combat(c, m).map_or(0, |r| r.total_damage) <= 0
            || c.get_power(m.id, PowerId::Malleable) > 0
            || (sts_core::content::powers::store::has_power(c, m.id, PowerId::Invincible)
                && c.get_power(m.id, PowerId::Invincible) < m.current_hp)
        {
            continue;
        }
        let required = i64::from(m.current_hp)
            + i64::from(m.block.max(0))
            + i64::from(c.get_power(m.id, PowerId::CurlUp).max(0));
        let mut dp = vec![None::<(i64, u64)>; (plays + 1) * stride];
        dp[0] = Some((0, 0));
        for (i, card) in hand.iter().enumerate() {
            let Some(CardBaseDamageDeliveryV1::Attack {
                hits: CardAttackHitsV1::Fixed { count },
                targets,
                ..
            }) = card_base_damage_delivery_v1(card.id, card.upgrades)
            else {
                continue;
            };
            let cost = card.get_cost();
            if targets == CardAttackTargetsV1::RandomEnemyPerHit
                || card.id == CardId::Conclude
                || cost < 0
                || cost as usize > energy
                || can_play_card(card, c).is_err()
            {
                continue;
            }
            let evaluated = evaluate_card_for_play(card, c, Some(m.id));
            let hit = evaluated
                .multi_damage
                .get(target_index)
                .copied()
                .unwrap_or(evaluated.base_damage_mut);
            let amount = i64::from(hit.max(0)) * i64::from(count.max(0));
            let cost = cost as usize;
            for n in (1..=plays).rev() {
                for e in cost..=energy {
                    if let Some((damage, mask)) = dp[(n - 1) * stride + e - cost] {
                        let next = (damage + amount, mask | (1u64 << i));
                        let slot = &mut dp[n * stride + e];
                        if slot.is_none_or(|old| next.0 > old.0) {
                            *slot = Some(next);
                        }
                    }
                }
            }
        }
        let chosen = (0..=energy).find_map(|e| {
            (1..=plays).find_map(|n| dp[n * stride + e].filter(|&(damage, _)| damage >= required))
        });
        if let Some((_, mask)) = chosen {
            plans.push(AttackPlan {
                target: m.id,
                turn: c.turn.turn_count,
                cards: hand
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & (1u64 << i) != 0)
                    .map(|(_, card)| (card.lineage, card.id))
                    .collect(),
            });
        }
    }
    plans
}

/// Oracle-only admission hint: one simple draw followed by a concrete attack
/// recipe. This layout is never a search node or an engine successor. It reads
/// the draw window and direct requests, using the shared discard-shuffle owner
/// on a private clone when the pile empties. Reactive hooks, draw triggers and
/// new random costs are omitted. Every proposed input still executes in the
/// charged engine; a missing card, choice, death or turn crossing aborts it.
fn draw_attack_plans(
    node: &Node,
    represented: &std::collections::HashSet<sts_core::EntityId>,
) -> Vec<AttackPlan> {
    let pos = node.state.position();
    let c = &pos.combat;
    if !matches!(pos.engine, EngineState::CombatPlayerTurn)
        || sts_core::content::powers::store::has_power(c, c.entities.player.id, PowerId::NoDraw)
        || (c.zones.draw_pile.is_empty() && c.zones.discard_pile.is_empty())
        || c.entities.monsters.iter().all(|monster| {
            represented.contains(&monster.id)
                || monster.current_hp <= 0
                || monster.half_dead
                || monster.is_escaped
        })
    {
        return vec![];
    }
    let mut proposals = std::collections::BTreeMap::new();
    for (i, card) in c.zones.hand.iter().enumerate() {
        let cost = i32::from(card.get_cost());
        if cost < 0 || cost > i32::from(c.turn.energy) || can_play_card(card, c).is_err() {
            continue;
        }
        for monster in &c.entities.monsters {
            if represented.contains(&monster.id) || monster.current_hp <= 0 || monster.half_dead {
                continue;
            }
            let effects = resolve_card_play(card.id, c, card, Some(monster.id));
            let Some(Action::DrawCards(draw)) = effects.last().map(|e| &e.action) else {
                continue;
            };
            if *draw == 0
                || effects.iter().any(|effect| {
                    effect.insertion_mode != AddTo::Bottom
                        || !matches!(
                            effect.action,
                            Action::Damage(_) | Action::GainBlock { .. } | Action::DrawCards(_)
                        )
                })
                || effects[..effects.len() - 1]
                    .iter()
                    .any(|e| matches!(e.action, Action::DrawCards(_)))
            {
                continue;
            }
            let count = (*draw as usize)
                .min(10usize.saturating_sub(c.zones.hand.len() - 1))
                .min(c.zones.draw_pile.len() + c.zones.discard_pile.len());
            if count == 0 {
                continue;
            }
            let mut preview = c.clone();
            let played = preview.zones.hand.remove(i);
            preview.turn.energy -= cost as u8;
            for effect in &effects[..effects.len() - 1] {
                match &effect.action {
                    Action::Damage(info) => {
                        let Some(target) = preview
                            .entities
                            .monsters
                            .iter_mut()
                            .find(|m| m.id == info.target)
                        else {
                            continue;
                        };
                        let damage = info.output.max(0);
                        let absorbed = target.block.max(0).min(damage);
                        target.block -= absorbed;
                        target.current_hp = (target.current_hp - (damage - absorbed)).max(0);
                    }
                    Action::GainBlock { target, amount }
                        if *target == preview.entities.player.id =>
                    {
                        preview.entities.player.block += (*amount).max(0);
                    }
                    _ => {}
                }
            }
            if get_card_definition(card.id).card_type == CardType::Attack {
                for power in [PowerId::Vigor, PowerId::PenNibPower] {
                    sts_core::content::powers::store::remove_power_type(
                        &mut preview,
                        c.entities.player.id,
                        power,
                    );
                }
            }
            for _ in 0..count {
                if preview.zones.draw_pile.is_empty() {
                    preview.shuffle_discard_pile_into_draw_pile();
                }
                let drawn = preview.draw_top_card().unwrap();
                preview.zones.hand.push(drawn);
            }
            // The played card stays in limbo until its own draw finishes. It
            // must not join this shuffle, even though it is discarded later.
            preview.zones.discard_pile.push(played);
            let mut other_targets = represented.clone();
            other_targets.extend(
                c.entities
                    .monsters
                    .iter()
                    .filter(|m| m.id != monster.id)
                    .map(|m| m.id),
            );
            for mut plan in attack_recipes(&preview, &other_targets, 1) {
                let total_cost = cost
                    + plan
                        .cards
                        .iter()
                        .filter_map(|(lineage, id)| {
                            preview
                                .zones
                                .hand
                                .iter()
                                .find(|card| card.lineage == *lineage && card.id == *id)
                        })
                        .map(|card| i32::from(card.get_cost()))
                        .sum::<i32>();
                plan.cards.push_front((card.lineage, card.id));
                let rank = (total_cost, plan.cards.len(), i);
                let entry = proposals
                    .entry(plan.target)
                    .or_insert_with(|| (rank, plan.clone()));
                if rank < entry.0 {
                    *entry = (rank, plan);
                }
            }
        }
    }
    proposals.into_values().map(|(_, plan)| plan).collect()
}

/// Representation of a kill must retain the preparation that can carry the
/// rest of the fight. This is a scheduling context, never state equivalence:
/// orders, exact HP and all reactive effects still run in their own states.
use super::retention::recovery::{healing_inventory, HealingInventory};

fn same_tactical_preparation(
    a: &Node,
    b: &Node,
    recovery_a: &HealingInventory,
    recovery_b: &HealingInventory,
) -> bool {
    let ac = &a.state.position().combat;
    let bc = &b.state.position().combat;
    sts_core::content::powers::store::powers_for(ac, ac.entities.player.id).unwrap_or(&[])
        == sts_core::content::powers::store::powers_for(bc, bc.entities.player.id).unwrap_or(&[])
        && ac.entities.potions == bc.entities.potions
        && a.expenditures() == b.expenditures()
        && recovery_a == recovery_b
}

impl Pass {
    pub(super) fn offer_attack_plans(
        &mut self,
        nodes: &[Node],
        kept: &[Node],
        stats: &mut SolverStats,
    ) {
        if !self.tactics_enabled {
            return;
        }
        // Challenge loss of an entire tactical opportunity, not every omitted
        // permutation of one already represented. This allocates probes only;
        // ordinary exact states and deferred order variants are not merged.
        let represented = kept
            .iter()
            .map(|node| {
                let direct = attack_plans(node);
                let direct_targets = direct.iter().map(|p| p.target).collect();
                let targets = direct
                    .into_iter()
                    .chain(draw_attack_plans(node, &direct_targets))
                    .map(|plan| plan.target)
                    .chain(
                        node.state
                            .position()
                            .combat
                            .entities
                            .monsters
                            .iter()
                            .filter(|m| m.current_hp <= 0 || m.half_dead || m.is_escaped)
                            .map(|m| m.id),
                    )
                    .collect::<std::collections::HashSet<_>>();
                (
                    node,
                    targets,
                    healing_inventory(&node.state.position().combat),
                )
            })
            .collect::<Vec<_>>();
        for node in nodes {
            if self.tactical.len() >= self.width * 8 {
                break;
            }
            // Each inventory is built once per candidate, rather than once
            // for every candidate/retained-peer comparison.
            let recovery = healing_inventory(&node.state.position().combat);
            let represented = represented
                .iter()
                .filter(|(peer, _, peer_recovery)| {
                    same_tactical_preparation(node, peer, &recovery, peer_recovery)
                })
                .flat_map(|(_, targets, _)| targets.iter().copied())
                .collect::<std::collections::HashSet<_>>();
            let direct = attack_plans_excluding(node, &represented);
            let mut draw_excluded = represented.clone();
            draw_excluded.extend(direct.iter().map(|plan| plan.target));
            let drawn = draw_attack_plans(node, &draw_excluded);
            for (plan, is_draw) in direct
                .into_iter()
                .map(|p| (p, false))
                .chain(drawn.into_iter().map(|p| (p, true)))
            {
                if self.tactical.len() >= self.width * 8 {
                    break;
                }
                if self.tactical_jobs().any(|job| {
                    job.plan.attack_target() == Some(plan.target) && job.node.state == node.state
                }) {
                    continue;
                }
                if let Some(audit) = &mut self.audit {
                    audit.note(node, "attack_plan_queued", stats.work());
                    if is_draw {
                        audit.note(node, "draw_attack_plan_queued", stats.work());
                    }
                }
                self.tactical.push_back(TacticalJob {
                    node: node.clone(),
                    plan: TacticalPlan::Attack(plan),
                });
                stats.attack_plans_started += 1;
            }
        }
    }

    pub(super) fn prepare_tactical_plan(
        &mut self,
        space: &CombatSearchSpace,
        stats: &mut SolverStats,
    ) -> Result<bool, String> {
        while let Some(mut job) = self.tactical.pop_front() {
            let choices = space.prepare_choices(&job.node.state)?;
            let Some(input) = job.plan.next_input(&job.node, &choices) else {
                job.plan.abort(stats);
                self.record_effect_proposal(&job.plan, &job.node);
                self.next.push(job.node);
                continue;
            };
            let Some(index) = choices
                .branches()
                .iter()
                .position(|branch| *branch == CombatBranch::Input(input.clone()))
            else {
                job.plan.abort(stats);
                self.record_effect_proposal(&job.plan, &job.node);
                self.next.push(job.node);
                continue;
            };
            self.expansion = Some(Expansion {
                graph_key: super::graph::Key::new(job.node.state.clone(), job.node.expenditures()),
                node: job.node,
                choices,
                branches: vec![index],
                index: 0,
                tactical_plan: Some(job.plan),
                backed_revision: 0,
            });
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::runtime::combat::{CombatCard, Power};

    fn position() -> CombatPosition {
        let mut c = sts_core::test_support::blank_test_combat();
        let mut enemy = sts_core::test_support::planned_monster(
            sts_core::content::monsters::EnemyId::JawWorm,
            1,
        );
        enemy.current_hp = 18;
        enemy.max_hp = 18;
        c.entities.monsters = vec![enemy];
        c.turn.energy = 3;
        c.zones.hand = [
            CardId::Strike,
            CardId::Defend,
            CardId::Strike,
            CardId::Strike,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 1))
        .collect();
        CombatPosition::new(EngineState::CombatPlayerTurn, c)
    }

    fn draw_position() -> CombatPosition {
        let mut source = position();
        let mut pommel = CombatCard::for_scenario(CardId::PommelStrike, 1);
        pommel.upgrades = 1;
        let mut heavy = CombatCard::for_scenario(CardId::HeavyBlade, 3);
        heavy.upgrades = 1;
        source.combat.zones.hand = vec![pommel, CombatCard::for_scenario(CardId::Defend, 2)];
        source.combat.zones.draw_pile =
            vec![CombatCard::for_scenario(CardId::Defend, 4), heavy].into();
        source
    }

    #[test]
    fn draw_kill_recipe_uses_real_draw_identity_and_charged_resumable_transitions() {
        let source = draw_position();
        let node = Node::root(CombatSearchState::new(source.clone()).unwrap());
        assert!(attack_plans(&node).is_empty());
        let mut plans = draw_attack_plans(&node, &Default::default());
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].cards.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
            vec![CardId::PommelStrike, CardId::HeavyBlade]
        );
        assert_eq!(
            node.state.position(),
            &source,
            "layout admission does not mutate source"
        );
        let mut plan = plans.pop().unwrap();
        let mut current = node;
        let mut inputs = vec![];
        while !plan.achieved(&current) {
            let input = plan.next_input(&current).unwrap();
            assert!(EngineCombatStepper.is_legal_action(current.state.position(), &input));
            let step = EngineCombatStepper.apply_to_stable(
                current.state.position(),
                input.clone(),
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!step.truncated && !step.timed_out);
            inputs.push(input);
            current = Node::root(CombatSearchState::new(step.position).unwrap());
        }
        assert_eq!(inputs.len(), 2);
        assert_eq!(
            crate::shuffle_order::replay(&source, &inputs)
                .unwrap()
                .last(),
            Some(current.state.position())
        );

        let make = || {
            let mut solver = CombatSolver::new(
                source.clone(),
                SolverConfig {
                    max_width: 4,
                    max_potion_uses: Some(0),
                    refine_potion_plans: false,
                    refine_card_order: false,
                    ..Default::default()
                },
            )
            .unwrap();
            let pass = &mut solver.beams[0].passes[0];
            pass.tactics_enabled = true;
            let root = pass.active[0].clone();
            pass.offer_attack_plans(&[root], &[], &mut solver.stats);
            assert_eq!(
                solver.stats.work(),
                0,
                "admission performs no engine transitions"
            );
            assert_eq!(solver.stats.attack_plans_started, 1);
            solver
        };
        let mut chunks = make();
        chunks.advance(0, Duration::from_secs(5)).unwrap();
        chunks.advance(10, Duration::ZERO).unwrap();
        assert_eq!(chunks.stats.work(), 0);
        for _ in 0..100 {
            let before = chunks.stats.work();
            chunks.advance(1, Duration::from_secs(5)).unwrap();
            assert!(chunks.stats.work() <= before + 1);
        }
        let mut whole = make();
        whole.advance(100, Duration::from_secs(5)).unwrap();
        assert_eq!(
            serde_json::to_value(chunks.stats()).unwrap(),
            serde_json::to_value(whole.stats()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(chunks.plans()).unwrap(),
            serde_json::to_value(whole.plans()).unwrap()
        );
        assert!(chunks.stats.attack_plan_transitions >= 2);
        assert!(chunks.stats.attack_plans_achieved > 0);
        assert!(!chunks.plans().is_empty());
        for plan in chunks.plans() {
            assert_eq!(
                crate::shuffle_order::replay(&source, &plan.inputs)
                    .unwrap()
                    .last(),
                Some(&plan.successor)
            );
        }
    }

    #[test]
    fn draw_recipe_tracks_real_shuffle_mapping_without_drawing_its_source_card() {
        let mut admitted = 0;
        for seed in 0..8 {
            let mut source = draw_position();
            let heavy = source
                .combat
                .zones
                .draw_pile
                .iter()
                .find(|card| card.id == CardId::HeavyBlade)
                .unwrap()
                .clone();
            source.combat.zones.draw_pile =
                vec![CombatCard::for_scenario(CardId::Defend, 4)].into();
            source.combat.zones.discard_pile =
                vec![heavy, CombatCard::for_scenario(CardId::Strike, 5)].into();
            source.combat.rng.shuffle_rng = sts_core::runtime::rng::StsRng::new(seed);
            let node = Node::root(CombatSearchState::new(source.clone()).unwrap());
            let plans = draw_attack_plans(&node, &Default::default());
            assert_eq!(node.state.position(), &source);
            let input = ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            };
            let after = crate::shuffle_order::replay(&source, &[input.clone()])
                .unwrap()
                .pop()
                .unwrap();
            let mapping = crate::shuffle_order::mapping(&source, &input, &after).unwrap();
            assert_eq!(mapping.input.len(), 2);
            assert!(mapping
                .input
                .iter()
                .all(|lineage| *lineage != source.combat.zones.hand[0].lineage));
            let drew_heavy = after
                .combat
                .zones
                .hand
                .iter()
                .any(|card| card.id == CardId::HeavyBlade);
            assert_eq!(!plans.is_empty(), drew_heavy);
            if let Some(mut plan) = plans.into_iter().next() {
                admitted += 1;
                assert_eq!(plan.next_input(&node), Some(input));
                let actual = Node::root(CombatSearchState::new(after.clone()).unwrap());
                let follow = plan.next_input(&actual).unwrap();
                let end = crate::shuffle_order::replay(&after, &[follow])
                    .unwrap()
                    .pop()
                    .unwrap();
                assert!(plan.achieved(&Node::root(CombatSearchState::new(end).unwrap())));
            }
        }
        assert!(
            admitted > 0 && admitted < 8,
            "the concrete shuffle window matters"
        );
    }

    #[test]
    fn draw_admission_respects_window_energy_play_limits_and_real_reactive_failure() {
        let source = draw_position();
        let admits = |p| {
            !draw_attack_plans(
                &Node::root(CombatSearchState::new(p).unwrap()),
                &Default::default(),
            )
            .is_empty()
        };
        assert!(admits(source.clone()));
        let mut unavailable = source.clone();
        unavailable.combat.zones.draw_pile.clear();
        assert!(!admits(unavailable));
        let mut too_small = source.clone();
        too_small.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Defend, 4)].into();
        assert!(!admits(too_small), "cannot draw a missing attack");
        let mut too_expensive = source.clone();
        too_expensive.combat.turn.energy = 2;
        assert!(!admits(too_expensive));
        let mut normality = source.clone();
        normality
            .combat
            .zones
            .hand
            .push(CombatCard::for_scenario(CardId::Normality, 10));
        normality.combat.turn.counters.cards_played_this_turn = 2;
        assert!(!admits(normality), "draw play uses the remaining play slot");
        let mut full_hand = source.clone();
        for lineage in 10..18 {
            full_hand
                .combat
                .zones
                .hand
                .push(CombatCard::for_scenario(CardId::Defend, lineage));
        }
        assert!(
            !admits(full_hand),
            "only first draw fits; Heavy Blade stays in pile"
        );
        let mut no_draw = source.clone();
        sts_core::content::powers::store::set_powers_for(
            &mut no_draw.combat,
            0,
            vec![Power {
                power_type: PowerId::NoDraw,
                amount: 1,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        assert!(!admits(no_draw));

        let mut reactive = source;
        reactive.combat.entities.player.current_hp = 1;
        sts_core::content::powers::store::set_powers_for(
            &mut reactive.combat,
            1,
            vec![Power {
                power_type: PowerId::Thorns,
                amount: 3,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        let node = Node::root(CombatSearchState::new(reactive).unwrap());
        let mut plan = draw_attack_plans(&node, &Default::default()).pop().unwrap();
        let input = plan.next_input(&node).unwrap();
        let step = EngineCombatStepper.apply_to_stable(
            node.state.position(),
            input,
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        let after = Node::root(CombatSearchState::new(step.position).unwrap());
        assert!(after.state.position().combat.entities.player.current_hp <= 0);
        assert!(!plan.achieved(&after));
        assert!(plan.next_input(&after).is_none());
    }

    #[test]
    fn kept_kill_with_lost_scaling_or_healing_does_not_cover_a_draw_kill() {
        let mut source = draw_position();
        let mut other = source.combat.entities.monsters[0].clone();
        other.id = 2;
        other.current_hp = 100;
        other.max_hp = 100;
        source.combat.entities.monsters.push(other);
        source
            .combat
            .zones
            .hand
            .push(CombatCard::for_scenario(CardId::Reaper, 5));
        sts_core::content::powers::store::set_powers_for(
            &mut source.combat,
            0,
            vec![Power {
                power_type: PowerId::DemonForm,
                amount: 3,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        let root = Node::root(CombatSearchState::new(source.clone()).unwrap());
        let mut kept = source.clone();
        kept.combat.entities.monsters[0].current_hp = 0;
        let make = |kept: CombatPosition| {
            let mut solver = CombatSolver::new(
                source.clone(),
                SolverConfig {
                    max_width: 4,
                    ..Default::default()
                },
            )
            .unwrap();
            let pass = &mut solver.beams[0].passes[0];
            pass.tactics_enabled = true;
            pass.offer_attack_plans(
                std::slice::from_ref(&root),
                &[Node::root(CombatSearchState::new(kept).unwrap())],
                &mut solver.stats,
            );
            pass.tactical
                .iter()
                .filter(|job| job.plan.attack_target() == Some(1))
                .count()
        };
        assert_eq!(
            make(source.clone()),
            0,
            "a retained draw-kill represents the same prepared opportunity"
        );
        assert_eq!(
            make(kept.clone()),
            0,
            "same preparation already represents the kill"
        );
        let mut no_scaling = kept.clone();
        sts_core::content::powers::store::remove_power_type(
            &mut no_scaling.combat,
            0,
            PowerId::DemonForm,
        );
        assert_eq!(
            make(no_scaling),
            1,
            "same enemy removal without Demon Form is a different opportunity"
        );
        let reaper = kept.combat.zones.hand.pop().unwrap();
        assert_eq!(reaper.id, CardId::Reaper);
        kept.combat.zones.exhaust_pile.push(reaper);
        assert_eq!(
            make(kept),
            1,
            "spent recovery must not cover a retained-Reaper kill"
        );
    }

    #[test]
    fn recipe_rebinds_three_attacks_and_respects_real_play_limits() {
        let source = position();
        let node = Node::root(CombatSearchState::new(source.clone()).unwrap());
        let mut plan = attack_plans(&node).pop().unwrap();
        let mut current = node.clone();
        let mut inputs = vec![];
        while !plan.achieved(&current) {
            let input = plan.next_input(&current).unwrap();
            assert!(EngineCombatStepper.is_legal_action(current.state.position(), &input));
            let step = EngineCombatStepper.apply_to_stable(
                current.state.position(),
                input.clone(),
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!step.truncated && !step.timed_out);
            inputs.push(input);
            current = Node::root(CombatSearchState::new(step.position).unwrap());
        }
        assert_eq!(inputs.len(), 3);
        assert_eq!(
            crate::shuffle_order::replay(&source, &inputs)
                .unwrap()
                .last(),
            Some(current.state.position())
        );
        assert_eq!(node.state.position(), &source);

        let mut limited = source.clone();
        limited.combat.turn.energy = 2;
        assert!(attack_plans(&Node::root(CombatSearchState::new(limited).unwrap())).is_empty());
        let mut limited = source;
        limited
            .combat
            .zones
            .hand
            .push(CombatCard::for_scenario(CardId::Normality, 9));
        limited.combat.turn.counters.cards_played_this_turn = 1;
        assert!(attack_plans(&Node::root(CombatSearchState::new(limited).unwrap())).is_empty());
    }

    #[test]
    fn first_draw_recipe_waits_for_real_hand_and_keeps_ordinary_search() {
        let mut source = position();
        let mut sentry = sts_core::test_support::planned_monster(
            sts_core::content::monsters::EnemyId::Sentry,
            3,
        );
        sentry.current_hp = 12;
        sentry.max_hp = 12;
        sentry.sentry.first_move = false;
        sentry.sentry.protocol_seeded = true;
        source.combat.entities.monsters = vec![sentry];
        source.combat.turn.energy = 0;
        source.combat.zones.discard_pile = std::mem::take(&mut source.combat.zones.hand).into();
        let target_turn = source.combat.turn.turn_count + 1;
        for (complete_actions, follow_preparations) in
            [(false, false), (true, false), (false, true)]
        {
            let make = |enabled| {
                let mut solver = CombatSolver::new(
                    source.clone(),
                    SolverConfig {
                        max_width: 4,
                        max_potion_uses: Some(0),
                        refine_potion_plans: false,
                        refine_card_order: false,
                        complete_actions,
                        follow_preparations,
                        tactical_attacks: enabled,
                        ..Default::default()
                    },
                )
                .unwrap();
                solver.beams[0].passes[0].first_draw_attack_turn = Some(target_turn);
                solver
            };
            let mut chunks = make(true);
            chunks.advance(1, Duration::from_secs(5)).unwrap();
            assert_eq!(chunks.stats.work(), 1, "the EndTurn/draw is charged");
            assert_eq!(chunks.stats.attack_plans_started, 0);
            // Charged boundary backups can intervene before the next actual
            // transition. The recipe must still wait for that real draw.
            while chunks.stats.search_transitions < 2 {
                chunks.advance(1, Duration::from_secs(5)).unwrap();
                assert!(chunks.stats.work() < 16);
            }
            assert_eq!(chunks.stats.attack_plans_started, 1);
            assert_eq!(chunks.stats.attack_plan_transitions, 1);
            let pass = &chunks.beams[0].passes[0];
            assert!(pass.first_draw_attack_turn.is_none());
            let ordinary = &pass.active[0];
            assert_eq!(ordinary.inputs(), vec![ClientInput::EndTurn]);
            assert_eq!(
                ordinary.state.position().combat.turn.turn_count,
                target_turn
            );
            assert_eq!(
                ordinary.state.position(),
                crate::shuffle_order::replay(&source, &[ClientInput::EndTurn])
                    .unwrap()
                    .last()
                    .unwrap()
            );
            let before = serde_json::to_value(chunks.stats()).unwrap();
            chunks.advance(0, Duration::from_secs(5)).unwrap();
            chunks.advance(1, Duration::ZERO).unwrap();
            assert_eq!(serde_json::to_value(chunks.stats()).unwrap(), before);
            for _ in chunks.stats.work()..200 {
                let before = chunks.stats.work();
                chunks.advance(1, Duration::from_secs(5)).unwrap();
                assert!(chunks.stats.work() <= before + 1);
            }
            let mut whole = make(true);
            whole.advance(200, Duration::from_secs(5)).unwrap();
            assert_eq!(
                serde_json::to_value(chunks.stats()).unwrap(),
                serde_json::to_value(whole.stats()).unwrap()
            );
            assert_eq!(
                serde_json::to_value(chunks.plans()).unwrap(),
                serde_json::to_value(whole.plans()).unwrap()
            );
            assert_eq!(chunks.stats.attack_plans_started, 1);
            assert_eq!(chunks.stats.attack_plans_achieved, 1);
            assert!(!chunks.plans().is_empty());
            for plan in chunks.plans() {
                assert_eq!(
                    crate::shuffle_order::replay(&source, &plan.inputs)
                        .unwrap()
                        .last(),
                    Some(&plan.successor)
                );
            }
            let mut disabled = make(false);
            disabled.advance(200, Duration::from_secs(5)).unwrap();
            assert_eq!(disabled.stats.attack_plans_started, 0);
            assert!(!disabled.plans().is_empty());
        }
    }

    #[test]
    fn attack_jobs_share_budget_resume_and_require_whole_combat_replay() {
        let source = position();
        let make = |complete_actions, follow_preparations| {
            let mut solver = CombatSolver::new(
                source.clone(),
                SolverConfig {
                    tactical_attacks: true,
                    max_width: 4,
                    max_potion_uses: Some(0),
                    refine_potion_plans: false,
                    refine_card_order: false,
                    complete_actions,
                    follow_preparations,
                    ..Default::default()
                },
            )
            .unwrap();
            let pass = &mut solver.beams[0].passes[0];
            pass.tactics_enabled = true;
            let node = pass.active[0].clone();
            pass.offer_attack_plans(&[node], &[], &mut solver.stats);
            solver
        };
        for (complete_actions, follow_preparations) in
            [(false, false), (true, false), (false, true)]
        {
            let mut chunks = make(complete_actions, follow_preparations);
            let mut whole = make(complete_actions, follow_preparations);
            chunks.advance(0, Duration::from_secs(5)).unwrap();
            chunks.advance(100, Duration::ZERO).unwrap();
            assert_eq!(chunks.stats.work(), 0);
            for _ in 0..200 {
                let before = chunks.stats.work();
                chunks.advance(1, Duration::from_secs(5)).unwrap();
                assert!(chunks.stats.work() - before <= 1);
                let stats = chunks.stats();
                assert_eq!(
                    stats.attack_plans_started,
                    stats.attack_plans_achieved
                        + stats.attack_plans_aborted
                        + stats.attack_plans_quota_stopped
                        + chunks
                            .progress()
                            .iter()
                            .map(|p| p.attack_plans)
                            .sum::<usize>()
                );
            }
            whole.advance(200, Duration::from_secs(5)).unwrap();
            assert!(chunks.stats.attack_plan_transitions >= 3);
            assert!(chunks.stats.attack_plans_achieved > 0);
            assert_eq!(
                serde_json::to_value(&chunks.stats).unwrap(),
                serde_json::to_value(&whole.stats).unwrap()
            );
            assert_eq!(
                serde_json::to_value(chunks.plans()).unwrap(),
                serde_json::to_value(whole.plans()).unwrap()
            );
            assert!(!chunks.plans().is_empty());
            for plan in chunks.plans() {
                assert_eq!(
                    crate::shuffle_order::replay(&source, &plan.inputs)
                        .unwrap()
                        .last(),
                    Some(&plan.successor)
                );
            }
        }
    }

    #[test]
    fn tactical_service_preserves_ordinary_work_and_quota_preserves_exact_prefix() {
        let source = position();
        let config = SolverConfig {
            tactical_attacks: true,
            complete_actions: true,
            ..Default::default()
        };
        let mut solver = CombatSolver::new(source.clone(), config.clone()).unwrap();
        let pass = &mut solver.beams[0].passes[0];
        pass.tactics_enabled = true;
        let node = pass.active[0].clone();
        let ordinary = pass
            .prepare_node(node.clone(), &solver.space, &config, &mut solver.stats)
            .unwrap();
        let ordinary_branches = ordinary.branches.clone();
        pass.ordinary = Some(ordinary);
        pass.offer_attack_plans(&[node], &[], &mut solver.stats);
        assert!(pass
            .prepare_tactical_plan(&solver.space, &mut solver.stats)
            .unwrap());
        // A completed tactical expansion must leave the suspended ordinary cursor intact.
        pass.expansion.as_mut().unwrap().index = 1;
        pass.park_expansion(false);
        assert_eq!(pass.ordinary.as_ref().unwrap().branches, ordinary_branches);

        // Retain a real intermediate recipe prefix for sampling and quota recovery.
        let node = pass.active[0].clone();
        let mut plan = attack_plans(&node).pop().unwrap();
        let input = plan.next_input(&node).unwrap();
        let step = EngineCombatStepper.apply_to_stable(
            &source,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        let current = Node {
            state: CombatSearchState::new(step.position).unwrap(),
            trace: Some(Arc::new(Trace {
                previous: None,
                input: input.clone(),
                expenditures: node.expenditures(),
            })),
            input_count: 1,
            card_baseline: node.card_baseline.clone(),
            potion_budget: node.potion_budget.clone(),
        };
        pass.tactical.push_back(TacticalJob {
            node: current.clone(),
            plan: TacticalPlan::Attack(plan),
        });
        let before = solver.stats.work();
        let sampled = solver.sample_prefixes(8);
        assert!(sampled
            .iter()
            .any(|p| p.pool == prefixes::SolverPrefixPool::AttackPlan
                && p.inputs == vec![input.clone()]
                && p.successor == *current.state.position()));
        assert_eq!(solver.stats.work(), before);
        assert_eq!(solver.progress()[0].attack_plans, 1);
        let pass = &mut solver.beams[0].passes[0];
        pass.defer_turn(&mut solver.stats);
        assert!(pass
            .deferred_cursors
            .iter()
            .flat_map(actions::DeferredCursor::nodes)
            .any(|n| n.state == current.state && n.inputs() == current.inputs()));
        assert_eq!(solver.stats.attack_plans_quota_stopped, 0);
        assert_eq!(solver.stats.quota_cursors_saved, 2);
        assert_eq!(solver.progress()[0].attack_plans, 1);
        let started = solver.stats.attack_plans_started;
        solver.beams[0].passes[0].offer_attack_plans(&[current], &[], &mut solver.stats);
        assert_eq!(solver.stats.attack_plans_started, started);
    }

    #[test]
    fn predicted_kill_never_overrides_reactive_damage_or_pending_choice() {
        let mut source = position();
        source.combat.entities.player.current_hp = 1;
        sts_core::content::powers::store::set_powers_for(
            &mut source.combat,
            1,
            vec![Power {
                power_type: PowerId::Thorns,
                amount: 3,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        let node = Node::root(CombatSearchState::new(source).unwrap());
        let mut plan = attack_plans(&node).pop().unwrap();
        let input = plan.next_input(&node).unwrap();
        let step = EngineCombatStepper.apply_to_stable(
            node.state.position(),
            input,
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        assert!(step.position.combat.entities.player.current_hp <= 0);
        let after = Node::root(CombatSearchState::new(step.position).unwrap());
        assert!(!plan.achieved(&after));
        assert!(plan.next_input(&after).is_none());

        let mut source = position();
        source.combat.zones.hand[0] = CombatCard::for_scenario(CardId::Headbutt, 1);
        source
            .combat
            .zones
            .discard_pile
            .push(CombatCard::for_scenario(CardId::Defend, 9));
        source
            .combat
            .zones
            .discard_pile
            .push(CombatCard::for_scenario(CardId::Bash, 10));
        let node = Node::root(CombatSearchState::new(source).unwrap());
        let mut plan = attack_plans(&node).pop().unwrap();
        let input = plan.next_input(&node).unwrap();
        let step = EngineCombatStepper.apply_to_stable(
            node.state.position(),
            input,
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        assert!(matches!(
            step.position.engine,
            EngineState::PendingChoice(_)
        ));
        let after = Node::root(CombatSearchState::new(step.position).unwrap());
        assert!(!plan.achieved(&after));
        assert!(plan.next_input(&after).is_none());
    }
}
