use super::*;
use crate::{SearchBudget, SearchConfig, SearchSession};
use sts_core::content::cards::CardId;

fn select(e: &mut NaturalEpisode, input: ClientInput) {
    e.choose(e.source(), &NaturalAction::Run(input)).unwrap();
}
fn enter_first(e: &mut NaturalEpisode) {
    select(e, ClientInput::EventChoice(0));
    // The ordinary first Neow choice is exposed by the game, with any follow-up
    // selected from its actual surface. This is plumbing, not demonstration data.
    while !e.engine.is_map_surface() {
        let a = e
            .options()
            .unwrap()
            .into_iter()
            .find(|o| !matches!(o.action, NaturalAction::Run(ClientInput::Cancel)))
            .unwrap()
            .action;
        e.choose(e.source(), &a).unwrap();
    }
    let a = e
        .options()
        .unwrap()
        .into_iter()
        .find(|o| matches!(o.action, NaturalAction::Run(ClientInput::SelectMapNode(_))))
        .unwrap()
        .action;
    e.choose(e.source(), &a).unwrap();
}

#[test]
fn fresh_sources_reject_cross_episode_stale_and_invalid_inputs() {
    let mut a = NaturalEpisode::new(501, 100).unwrap();
    let b = NaturalEpisode::new(501, 100).unwrap();
    let original = a.snapshot();
    let input = a.options().unwrap()[0].action.clone();
    assert!(a.choose(b.source(), &input).is_err());
    assert!(a
        .choose(a.source(), &NaturalAction::Run(ClientInput::BuyCard(999)))
        .is_err());
    assert_eq!(a.snapshot(), original);
    let old = a.source();
    a.choose(old, &input).unwrap();
    let next = a.snapshot();
    assert!(a.choose(old, &input).is_err());
    assert_eq!(a.snapshot(), next);
}

#[test]
fn materialized_combat_input_checks_source_and_executes_symbolic_choices() {
    use sts_core::runtime::combat::CombatCard;
    let mut e = constructed_elite();
    let combat = &mut e.active.as_mut().unwrap().combat_state;
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::BurningPact, 101),
        CombatCard::for_scenario(CardId::Strike, 102),
        CombatCard::for_scenario(CardId::Defend, 103),
    ];
    refresh_cursor(&mut e);
    let before = e.snapshot();
    let source = e.source();
    assert!(e
        .choose_combat_input(source, &ClientInput::BuyCard(0))
        .is_err());
    assert_eq!(e.snapshot(), before);
    assert_eq!(e.source(), source);
    let play = ClientInput::PlayCard {
        card_index: 0,
        target: None,
    };
    e.choose_combat_input(source, &play).unwrap();
    let pending = e.snapshot();
    assert!(e.choose_combat_input(source, &play).is_err());
    assert_eq!(e.snapshot(), pending);
    let space = CombatSearchSpace::default();
    let state = e.cursor.as_ref().unwrap();
    let family = space
        .decision_choices(state)
        .unwrap()
        .into_iter()
        .find(|c| matches!(c.action, CombatBranch::BeginSelection(_)))
        .unwrap()
        .action;
    let selecting = space.virtual_successor(state, &family).unwrap();
    let member = space
        .decision_choices(&selecting)
        .unwrap()
        .into_iter()
        .find(|c| matches!(c.action, CombatBranch::SelectMember(_)))
        .unwrap()
        .action;
    let selected = space.virtual_successor(&selecting, &member).unwrap();
    let input = space
        .input(&selected, &CombatBranch::FinishSelection)
        .unwrap()
        .unwrap();
    e.choose_combat_input(e.source(), &input).unwrap();
    assert!(matches!(e.engine, EngineState::CombatPlayerTurn));
    assert!(e
        .active
        .as_ref()
        .unwrap()
        .combat_state
        .zones
        .exhaust_pile
        .iter()
        .any(|c| c.id == CardId::Strike));
}

#[test]
fn fresh_natural_combat_encodes_identically_without_a_search_tree() {
    let mut e = NaturalEpisode::new(502, 200).unwrap();
    enter_first(&mut e);
    let state = e.cursor.as_ref().unwrap().clone();
    let budget = SearchBudget {
        max_services: 0,
        max_engine_steps: 0,
        max_nodes: 1,
        max_depth: 0,
        deadline: None,
        guidance: Default::default(),
    };
    let session = SearchSession::new(
        CombatSearchSpace::default(),
        state,
        SearchConfig::default(),
        budget,
    )
    .unwrap();
    assert_eq!(
        e.combat_entities().unwrap().unwrap(),
        entities::encode(&session, session.root()).unwrap()
    );
}

#[test]
fn natural_rooms_keep_deck_resources_rng_and_replay_every_boundary() {
    let mut e = NaturalEpisode::new(503, 400).unwrap();
    let mut replay = NaturalEpisode::new(503, 400).unwrap();
    let mut inputs = vec![];
    while e.results.len() < 2 && e.stop.is_none() && e.steps < 350 {
        let options = e.options().unwrap();
        let a = if e.cursor.is_some() {
            options.iter().find(|o| matches!(&o.action, NaturalAction::Combat(CombatBranch::Input(ClientInput::PlayCard {card_index, ..})) if e.active.as_ref().unwrap().combat_state.zones.hand[*card_index].id != CardId::Defend))
                .or_else(|| options.iter().find(|o| matches!(o.action, NaturalAction::Combat(CombatBranch::Input(ClientInput::PlayCard { .. })))))
                .or_else(|| options.iter().find(|o| matches!(o.action, NaturalAction::Combat(CombatBranch::Input(ClientInput::EndTurn)))))
                .unwrap()
        } else {
            options.iter().find(|o| matches!(o.action, NaturalAction::Run(ClientInput::ClaimReward(_))))
                .or_else(|| options.iter().find(|o| matches!(o.action, NaturalAction::Run(ClientInput::SelectCard(_)))))
                .or_else(|| options.iter().find(|o| matches!(o.action, NaturalAction::Run(ClientInput::SelectMapNode(_)))))
                .or_else(|| options.iter().find(|o| matches!(o.action, NaturalAction::Run(ClientInput::Proceed))))
                .unwrap_or(&options[0])
        }.action.clone();
        assert_eq!(
            e.combat_entities().unwrap(),
            replay.combat_entities().unwrap()
        );
        if let Some(encoded) = e.combat_entities().unwrap() {
            let value = serde_json::to_value(&encoded).unwrap();
            let persisted: serde_json::Value =
                serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
            let restored: entities::EntityPosition = serde_json::from_value(persisted).unwrap();
            assert_eq!(encoded, restored);
        }
        let actual = e.choose(e.source(), &a).unwrap();
        assert_eq!(actual, replay.choose(replay.source(), &a).unwrap());
        assert_eq!(e.snapshot(), replay.snapshot());
        inputs.push(actual);
    }
    assert_eq!(e.results.len(), 2);
    assert!(e.run.master_deck.len() > 12);
    assert!(e.run.gold > 99);
    assert!(e.results.iter().all(|r| r.defeated));
    assert!(e.active.is_none());
    assert!(matches!(e.engine, EngineState::RewardScreen(_)));
    assert!(inputs.len() > 20);
}

#[test]
fn action_budget_is_not_a_loss() {
    let mut e = NaturalEpisode::new(1, 1).unwrap();
    select(&mut e, ClientInput::EventChoice(0));
    assert_eq!(e.stop(), Some(&NaturalStop::ActionBudget));
    assert!(e.options().unwrap().is_empty());
    assert_eq!(e.run.current_hp, 68);
}

#[test]
fn blocked_potion_reward_is_visible_without_an_executable_claim() {
    use sts_core::content::potions::{Potion, PotionId};
    use sts_core::state::rewards::{RewardItem, RewardState};
    let mut e = NaturalEpisode::new(601, 20).unwrap();
    // Constructed rendering case, not natural training material.
    e.run.potions = vec![Some(Potion::new(PotionId::FirePotion, 1)); 2];
    let mut rewards = RewardState::new();
    rewards.items.push(RewardItem::Potion {
        potion_id: PotionId::FruitJuice,
    });
    e.engine = EngineState::RewardScreen(rewards);
    assert!(e.view().unwrap().contains("Fruit Juice: potion slots full"));
    assert!(!e
        .options()
        .unwrap()
        .iter()
        .any(|o| o.action == NaturalAction::Run(ClientInput::ClaimReward(0))));
}

#[test]
fn stopped_reward_boundary_does_not_mislabel_unclaimed_potions_as_full() {
    use sts_core::content::potions::PotionId;
    use sts_core::state::rewards::{RewardItem, RewardState};
    let mut e = NaturalEpisode::new(601, 20).unwrap();
    let mut rewards = RewardState::new();
    rewards.items.push(RewardItem::Potion {
        potion_id: PotionId::Elixir,
    });
    e.engine = EngineState::RewardScreen(rewards);
    e.stop = Some(NaturalStop::EliteDefeated);
    assert!(e.run.find_empty_potion_slot().is_some());
    let text = e.view().unwrap();
    assert!(text.contains("Stopped: EliteDefeated"));
    assert!(!text.contains("Elixir"));
    assert!(!text.contains("slots full"));
}

fn constructed_elite() -> NaturalEpisode {
    use sts_core::content::monsters::factory::EncounterId;
    use sts_core::state::core::CombatStartRequest;
    let mut e = NaturalEpisode::new(601, 100).unwrap();
    e.run.floor_num = 6;
    e.engine = EngineState::CombatStart(CombatStartRequest::room(
        EncounterId::GremlinNob,
        RoomType::MonsterRoomElite,
    ));
    e.tick(None).unwrap();
    e
}
fn refresh_cursor(e: &mut NaturalEpisode) {
    e.cursor = Some(
        CombatSearchState::new(CombatPosition::new(
            e.engine.clone(),
            e.active.as_ref().unwrap().combat_state.clone(),
        ))
        .unwrap(),
    );
}

#[test]
fn combat_choices_select_exact_cards_and_targets_without_display_labels() {
    use crate::observation::ModelChoice;
    use sts_core::content::monsters::factory::EncounterId;
    use sts_core::runtime::combat::CombatCard;
    use sts_core::state::core::CombatStartRequest;

    // Constructed interface regression, not natural demonstration material.
    let mut e = NaturalEpisode::new(602, 100).unwrap();
    e.run.floor_num = 6;
    e.engine = EngineState::CombatStart(CombatStartRequest::room(
        EncounterId::ThreeSentries,
        RoomType::MonsterRoomElite,
    ));
    e.tick(None).unwrap();
    let combat = &mut e.active.as_mut().unwrap().combat_state;
    combat.zones.hand = [CardId::PommelStrike, CardId::Strike, CardId::Strike]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, 1000 + i as u32))
        .collect();
    combat.entities.monsters[0].current_hp = 0;
    combat.entities.monsters[2].current_hp = 20;
    refresh_cursor(&mut e);

    for remaining_hp in [14, 8] {
        let observation = e.combat_observation().unwrap().unwrap();
        let mut options = e.options().unwrap();
        for option in &mut options {
            option.label = "presentation is not an action key".into();
        }
        let action = options
            .iter()
            .find(|option| {
                matches!(
                    option.combat_choice,
                    Some(ModelChoice::Play { hand, target: Some(2) })
                        if observation.hand[hand].card == CardId::Strike
                )
            })
            .unwrap()
            .action
            .clone();
        let source = e.source();
        e.choose(source, &action).unwrap();
        let after = e.combat_observation().unwrap().unwrap();
        assert_eq!(after.monsters[2].hp[0], remaining_hp);
        assert_eq!(after.monsters[1].hp, observation.monsters[1].hp);
        assert_eq!(after.hand.len(), observation.hand.len() - 1);
        assert_eq!(after.hand[0].card, CardId::PommelStrike);
        let snapshot = e.snapshot();
        assert!(e.choose(source, &action).is_err());
        assert_eq!(e.snapshot(), snapshot);
    }
}

#[test]
fn current_resources_use_active_combat_instead_of_unsettled_run_values() {
    use sts_core::content::potions::{Potion, PotionId};
    let mut e = constructed_elite();
    let entities = &mut e.active.as_mut().unwrap().combat_state.entities;
    entities.player.current_hp = 27;
    entities.player.max_hp = 77;
    entities.player.gold = 41;
    entities.player.relics[0].counter = 3;
    entities.potions[0] = Some(Potion::new(PotionId::FirePotion, 72));
    refresh_cursor(&mut e);
    let live = e.resources();
    assert_eq!((live.hp, live.max_hp, live.gold), (27, 77, 41));
    assert_eq!(live.relics[0].counter, 3);
    assert_eq!(live.potions[0].as_ref().unwrap().id, PotionId::FirePotion);
    assert_eq!((e.run.current_hp, e.run.max_hp, e.run.gold), (68, 75, 99));
    assert!(e
        .view()
        .unwrap()
        .starts_with("Act 1 F6 | HP 27/77 | Gold 41"));
}

#[test]
fn elite_defeat_stops_after_shared_settlement_before_any_reward_claim() {
    let mut e = constructed_elite();
    e.active.as_mut().unwrap().combat_state.entities.monsters[0].current_hp = 1;
    refresh_cursor(&mut e);
    let action = e
        .options()
        .unwrap()
        .into_iter()
        .find(|o| {
            matches!(
                o.action,
                NaturalAction::Combat(CombatBranch::Input(ClientInput::PlayCard {
                    target: Some(_),
                    ..
                }))
            )
        })
        .unwrap()
        .action;
    e.choose(e.source(), &action).unwrap();
    assert_eq!(e.stop(), Some(&NaturalStop::EliteDefeated));
    assert!(e.active.is_none());
    let EngineState::RewardScreen(reward) = &e.engine else {
        panic!("elite rewards not settled")
    };
    assert!(reward.pending_card_choice.is_none());
    assert!(!reward.items.is_empty());
    assert_eq!(e.run.current_hp, 74); // Burning Blood, no reward auto-claim.
    assert_eq!(e.resources().hp, 74);
    assert_eq!(e.run.gold, 99);
    assert_eq!(e.results.len(), 1);
    assert!(e.results[0].defeated);
}

#[test]
fn escaping_an_elite_does_not_count_as_defeating_it() {
    use sts_core::content::potions::{Potion, PotionId};
    let mut e = constructed_elite();
    e.active.as_mut().unwrap().combat_state.entities.potions[0] =
        Some(Potion::new(PotionId::SmokeBomb, 22));
    refresh_cursor(&mut e);
    let action = e
        .options()
        .unwrap()
        .into_iter()
        .find(|o| {
            matches!(
                o.action,
                NaturalAction::Combat(CombatBranch::Input(ClientInput::UsePotion {
                    potion_index: 0,
                    ..
                }))
            )
        })
        .unwrap()
        .action;
    e.choose(e.source(), &action).unwrap();
    assert_ne!(e.stop(), Some(&NaturalStop::EliteDefeated));
    assert_ne!(e.stop(), Some(&NaturalStop::EliteDefeated));
    assert!(e.results[0].escaped);
    assert!(!e.results[0].defeated);
    assert_eq!(e.results[0].potion_uses, 1);
}
