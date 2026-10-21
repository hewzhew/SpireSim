use super::priority::priority_for_node;
use super::*;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::powers::PowerId;
use crate::runtime::combat::CombatCard;
use crate::runtime::combat::{MetaChange, Power, PowerPayload};
use crate::test_support::{blank_test_combat, test_monster};

#[test]
fn frontier_priority_prefers_stronger_visible_next_draw_when_state_ties() {
    let mut strike = test_node();
    strike.combat.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Strike, 11)]).into();

    let mut carnage = test_node();
    carnage.combat.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Carnage, 12)]).into();

    assert!(priority_for_node(&carnage) > priority_for_node(&strike));
}

#[test]
fn frontier_priority_prefers_higher_potion_tactical_role_when_state_ties() {
    let mut sustain = test_node();
    sustain.potion_tactical_priority = 10;

    let mut lethal = test_node();
    lethal.potion_tactical_priority = 50;

    assert!(priority_for_node(&lethal) > priority_for_node(&sustain));
}

#[test]
fn frontier_priority_uses_turn_branch_hint_as_late_tie_break() {
    let neutral = test_node();
    let mut same_turn = test_node();
    same_turn.last_turn_branch_priority = 12;

    assert!(priority_for_node(&same_turn) > priority_for_node(&neutral));
}

#[test]
fn frontier_priority_uses_action_prior_score_when_rollout_ties() {
    let mut low_prior = test_node();
    low_prior.action_prior_score = Some(0.1);
    let mut high_prior = test_node();
    high_prior.action_prior_score = Some(0.9);

    assert!(priority_for_node(&high_prior) > priority_for_node(&low_prior));
}

#[test]
fn frontier_priority_continues_retaliation_protection_before_raw_enemy_progress() {
    let mut protected = test_node();
    protected.action_ordering_frontier_hint = 1;

    let mut raw_progress = test_node();
    raw_progress.combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    protected.combat.entities.monsters = raw_progress.combat.entities.monsters.clone();
    raw_progress.combat.entities.monsters[0].current_hp -= 14;

    assert!(priority_for_node(&protected) > priority_for_node(&raw_progress));
}

#[test]
fn action_ordering_frontier_hint_expires_on_the_next_child_edge() {
    let mut parent = test_node();
    parent.action_ordering_frontier_hint = 1;

    let child = parent.clone_for_child(parent.engine.clone(), parent.combat.clone());

    assert_eq!(child.action_ordering_frontier_hint, 0);
}

#[test]
fn win_candidate_frontier_discards_strictly_dominated_results() {
    let root = CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat());
    let mut candidates = Vec::new();
    let mut low = test_node();
    low.combat.entities.player.current_hp = 30;
    let mut high = test_node();
    high.combat.entities.player.current_hp = 40;

    assert!(remember_win_candidate(&root, &mut candidates, &low));
    assert!(remember_win_candidate(&root, &mut candidates, &high));

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].node.combat.entities.player.current_hp, 40);
}

#[test]
fn win_candidate_frontier_preserves_hp_potion_tradeoffs() {
    use crate::content::potions::{Potion, PotionId};
    let mut candidates = Vec::new();
    let mut conserve = test_node();
    conserve.combat.entities.potions = vec![Some(Potion::new(PotionId::BloodPotion, 101))];
    let root = CombatPosition::new(conserve.engine.clone(), conserve.combat.clone());
    conserve.combat.entities.player.current_hp = 30;
    let mut spend = test_node();
    spend.combat.entities.potions = vec![None];
    spend.combat.entities.player.current_hp = 50;
    spend.potions_used = 1;

    assert!(remember_win_candidate(&root, &mut candidates, &conserve));
    assert!(remember_win_candidate(&root, &mut candidates, &spend));

    assert_eq!(candidates.len(), 2);
}

#[test]
fn win_candidate_frontier_preserves_distinct_potions_after_exact_lethals() {
    use crate::content::potions::{Potion, PotionId};

    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::JawWorm);
    monster.current_hp = 5;
    let target_id = monster.id;
    combat.entities.monsters = vec![monster];
    combat.entities.potions = vec![
        Some(Potion::new(PotionId::FirePotion, 101)),
        Some(Potion::new(PotionId::ExplosivePotion, 102)),
    ];
    let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let wins = [Some(target_id), None]
        .into_iter()
        .enumerate()
        .map(|(slot, target)| {
            let input = ClientInput::UsePotion {
                potion_index: slot,
                target,
            };
            exact_win_node(&root, &[input])
        })
        .collect::<Vec<_>>();
    assert_eq!(
        wins[0].combat.entities.player.current_hp,
        wins[1].combat.entities.player.current_hp
    );
    assert_ne!(
        wins[0].combat.entities.potions,
        wins[1].combat.entities.potions
    );
    for order in [[0, 1], [1, 0]] {
        let mut candidates = Vec::new();
        for index in order {
            remember_win_candidate(&root, &mut candidates, &wins[index]);
        }
        assert_eq!(
            candidates.len(),
            2,
            "equal HP and potion count cannot erase which potion remains"
        );
    }
}

#[test]
fn win_candidate_frontier_preserves_exact_pen_nib_cycle_outcomes() {
    use crate::content::relics::{RelicId, RelicState};

    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::JawWorm);
    monster.current_hp = 10;
    let target = Some(monster.id);
    combat.entities.monsters = vec![monster];
    let mut pen_nib = RelicState::new(RelicId::PenNib);
    pen_nib.counter = 8;
    combat.entities.player.add_relic(pen_nib);
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Carnage, 201),
        CombatCard::for_scenario(CardId::Strike, 202),
        CombatCard::for_scenario(CardId::Strike, 203),
    ];
    let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let ready = exact_win_node(
        &root,
        &[ClientInput::PlayCard {
            card_index: 0,
            target,
        }],
    );
    let spent = exact_win_node(
        &root,
        &[
            ClientInput::PlayCard {
                card_index: 1,
                target,
            },
            ClientInput::PlayCard {
                card_index: 1,
                target,
            },
        ],
    );
    assert_eq!(
        ready.combat.entities.player.current_hp,
        spent.combat.entities.player.current_hp
    );
    assert_eq!(ready.combat.entities.player.relics[0].counter, 9);
    assert_eq!(spent.combat.entities.player.relics[0].counter, 0);
    for wins in [[&ready, &spent], [&spent, &ready]] {
        let mut candidates = Vec::new();
        for win in wins {
            remember_win_candidate(&root, &mut candidates, win);
        }
        assert_eq!(
            candidates.len(),
            2,
            "persistent relic state is not an action-count tie-break"
        );
    }
}

fn exact_win_node(root: &CombatPosition, inputs: &[ClientInput]) -> SearchNode {
    let mut node = SearchNode::root(root.engine.clone(), root.combat.clone());
    for (step_index, input) in inputs.iter().enumerate() {
        let position = CombatPosition::new(node.engine.clone(), node.combat.clone());
        let choice = EngineCombatStepper
            .choice_for_legal_input(&position, input)
            .expect("test line must name a currently legal action");
        let step = EngineCombatStepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: 1_000,
                deadline: None,
            },
        );
        assert!(!step.truncated && !step.timed_out);
        node.engine = step.position.engine;
        node.combat = step.position.combat;
        node.note_input(input);
        node.push_action(CombatSearchV2ActionTrace {
            step_index,
            action_id: step_index,
            action_key: choice.action_key,
            action_debug: choice.action_debug,
            input: input.clone(),
        });
    }
    assert_eq!(
        EngineCombatStepper.terminal(&CombatPosition::new(
            node.engine.clone(),
            node.combat.clone()
        )),
        CombatTerminal::Win
    );
    node
}

#[test]
fn win_candidate_frontier_does_not_collapse_distinct_persistent_payoffs() {
    let root = CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat());
    let mut candidates = Vec::new();
    let mut gold = test_node();
    gold.combat.entities.player.gold_delta_this_combat = 50;

    let mut dagger = CombatCard::for_scenario(CardId::RitualDagger, 41);
    dagger.misc_value = 15;
    gold.combat.meta.master_deck_snapshot = vec![dagger.clone()].into();

    let mut ritual = test_node();
    ritual.combat.meta.master_deck_snapshot = vec![dagger].into();
    ritual
        .combat
        .meta
        .meta_changes
        .push(MetaChange::ModifyCardMisc {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(41),
            amount: 5,
        });

    assert!(remember_win_candidate(&root, &mut candidates, &gold));
    assert!(remember_win_candidate(&root, &mut candidates, &ritual));

    assert_eq!(
        candidates.len(),
        2,
        "gold and Ritual Dagger growth are incomparable typed outcomes; an aggregate score must not delete either line"
    );
}

#[test]
fn win_candidate_frontier_preserves_hp_recoverable_gold_tradeoff() {
    let root = CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat());
    let mut candidates = Vec::new();
    let mut escaped = test_node();
    escaped.combat.entities.player.current_hp = 82;
    escaped.combat.entities.player.gold_delta_this_combat = -75;

    let mut killed = test_node();
    killed.combat.entities.player.current_hp = 71;
    killed.combat.entities.player.gold_delta_this_combat = -75;
    killed
        .combat
        .runtime
        .pending_rewards
        .push(crate::state::rewards::RewardItem::StolenGold { amount: 75 });

    assert!(remember_win_candidate(&root, &mut candidates, &escaped));
    assert!(remember_win_candidate(&root, &mut candidates, &killed));
    assert_eq!(candidates.len(), 2);
}

#[test]
fn frontier_priority_uses_sustained_mitigation_after_raw_enemy_progress() {
    let mut better_progress = test_node();
    let mut monster = test_monster(EnemyId::TheGuardian);
    monster.id = 1;
    monster.current_hp = 220;
    monster.max_hp = 240;
    better_progress.combat.entities.monsters = vec![monster.clone()];

    let mut disarmed = test_node();
    monster.current_hp = 240;
    disarmed.combat.entities.monsters = vec![monster];
    disarmed.combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: None,
            amount: -3,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    assert!(priority_for_node(&better_progress) > priority_for_node(&disarmed));

    better_progress.combat.entities.monsters[0].current_hp = 240;
    assert!(priority_for_node(&disarmed) > priority_for_node(&better_progress));
}

#[test]
fn frontier_queue_preserves_priority_order() {
    let mut queue = FrontierQueue::new();

    let low_priority = test_node();
    let mut high_priority = test_node();
    high_priority.potion_tactical_priority = 50;

    queue.push_node(low_priority);
    queue.push_node(high_priority);

    assert_eq!(queue.len(), 2);
    assert_eq!(queue.pop().unwrap().node.potion_tactical_priority, 50);
    assert_eq!(queue.pop().unwrap().node.potion_tactical_priority, 0);
    assert!(queue.is_empty());
}

#[test]
fn frontier_priority_preserves_rollout_value_order() {
    let low = test_node_with_final_hp(10);
    let high = test_node_with_final_hp(20);

    assert!(priority_for_node(&high) > priority_for_node(&low));
}

#[test]
fn frontier_queue_preserves_fifo_order_for_exact_priority_ties() {
    let mut queue = FrontierQueue::new();
    let mut first = test_node();
    first.root_lineage = RootLineage::Action(RootLineageId(1));
    let mut second = test_node();
    second.root_lineage = RootLineage::Action(RootLineageId(2));

    queue.push_node(first);
    queue.push_node(second);

    assert_eq!(
        queue.pop().unwrap().node.root_lineage,
        RootLineage::Action(RootLineageId(1))
    );
    assert_eq!(
        queue.pop().unwrap().node.root_lineage,
        RootLineage::Action(RootLineageId(2))
    );
}

fn test_node_with_final_hp(final_hp: i32) -> SearchNode {
    let mut node = test_node();
    node.combat.entities.player.current_hp = final_hp;
    node.rollout_estimate = RolloutNodeEstimate::from_node(
        &node,
        0,
        RolloutStopReason::MaxActions,
        None,
        Default::default(),
    );
    node
}

fn test_node() -> SearchNode {
    SearchNode {
        engine: EngineState::CombatPlayerTurn,
        combat: blank_test_combat(),
        actions: Vec::new(),
        turn_prefix: TurnPrefixState::default(),
        initial_hp: 80,
        potions_used: 0,
        potions_discarded: 0,
        cards_played: 0,
        potion_tactical_priority: 0,
        last_turn_branch_priority: 0,
        action_prior_score: None,
        action_ordering_frontier_hint: 0,
        rollout_estimate: RolloutNodeEstimate::unevaluated(),
        root_lineage: Default::default(),
    }
}
