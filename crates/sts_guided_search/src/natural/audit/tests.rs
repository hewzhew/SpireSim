use super::*;
use sts_core::content::cards::CardId;
use sts_core::state::core::EngineState;
use sts_core::state::rewards::{RewardItem, RewardState};

#[test]
fn declining_an_offer_does_not_reopen_it_forever() {
    let mut e = NaturalEpisode::new(71, 100).unwrap();
    let cards = vec![sts_core::state::rewards::RewardCard {
        id: CardId::Clash,
        upgrades: 0,
    }];
    let mut rewards = RewardState::new();
    rewards.items.push(RewardItem::Card {
        cards: cards.clone(),
    });
    rewards.pending_card_choice = Some(cards);
    e.engine = EngineState::RewardScreen(rewards);
    // A constructed low-priority offer isolates cancellation behavior. It is
    // not a naturally obtained deck or a card-quality training example.
    e.run
        .master_deck
        .push(sts_core::runtime::combat::CombatCard::for_scenario(
            CardId::Clash,
            78,
        ));
    let mut policy = policy::SetupPolicy::default();
    let options = e.options().unwrap();
    let (i, _, _) = policy.choose(&e, &options).unwrap();
    assert_eq!(options[i].action, NaturalAction::Run(ClientInput::Cancel));
    e.choose(e.source(), &options[i].action).unwrap();
    let options = e.options().unwrap();
    let (i, _, _) = policy.choose(&e, &options).unwrap();
    assert_eq!(options[i].action, NaturalAction::Run(ClientInput::Proceed));
}

#[test]
fn setup_does_not_use_hidden_encounters_rng_or_display_strings() {
    let mut a = NaturalEpisode::new(91_001, 100).unwrap();
    let mut b = NaturalEpisode::new(91_001, 100).unwrap();
    a.choose(a.source(), &NaturalAction::Run(ClientInput::EventChoice(0)))
        .unwrap();
    b.choose(b.source(), &NaturalAction::Run(ClientInput::EventChoice(0)))
        .unwrap();
    b.run.monster_list.reverse();
    b.run.elite_monster_list.reverse();
    b.run.rng_pool.ai_rng.random(100);
    b.run.rng_pool.shuffle_rng.random(100);
    let original = a.options().unwrap();
    let mut renamed = b.options().unwrap();
    for option in &mut renamed {
        option.label = "not an action key".into();
    }
    let (left, _, _) = policy::SetupPolicy::default()
        .choose(&a, &original)
        .unwrap();
    let (right, _, _) = policy::SetupPolicy::default().choose(&b, &renamed).unwrap();
    assert_eq!(original[left].action, renamed[right].action);
}

#[test]
fn budget_stops_and_tampered_trajectories_remain_distinct_from_success() {
    let mut report = run(
        91_001,
        AuditConfig {
            max_actions: 1,
            work_per_combat: 1,
            work_per_run: 1,
            seconds_per_run: 1,
        },
    )
    .unwrap();
    assert_eq!(report.stop, "ActionBudget");
    assert!(report.trajectory_replayed);
    assert_eq!(report.decisions.len(), 1);
    assert_eq!(report.final_resources.hp, 68);
    assert!(report.settled_combats.is_empty());
    report.decisions[0].successor_digest = "tampered".into();
    assert!(replay(&report).unwrap_err().contains("differs after input"));
}

#[test]
fn exhausted_run_work_and_an_attempted_unknown_combat_are_distinct() {
    let config = AuditConfig {
        max_actions: 128,
        work_per_combat: 1_000,
        work_per_run: 1_000,
        seconds_per_run: 30,
    };
    let unstarted = run(96_001, config.clone()).unwrap();
    assert_eq!(unstarted.stop, "RunSearchBudgetBeforeCombat");
    assert_eq!(unstarted.search_work, config.work_per_run);
    assert_eq!(unstarted.combats.len(), 1);
    assert!(unstarted.combats[0].selected.is_some());
    assert!(
        unstarted.final_snapshot["run"][3].as_i64().unwrap()
            > i64::from(unstarted.combats[0].floor)
    );
    assert!(unstarted.trajectory_replayed);

    let attempted = run(
        96_001,
        AuditConfig {
            work_per_combat: 1,
            ..config
        },
    )
    .unwrap();
    assert_eq!(attempted.stop, "CombatSearchUnknown");
    assert_eq!(attempted.combats.len(), 1);
    assert_eq!(attempted.combats[0].stats.work(), 1);
    assert!(attempted.combats[0].selected.is_none());
    assert!(attempted.trajectory_replayed);
}
