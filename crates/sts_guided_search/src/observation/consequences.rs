//! Exact consequences of supplied plans, at source-relative control boundaries.
//! This projection owns no plan generator, ranking, continuation policy or model.
use super::{combat_observation, monster, CombatObservation, MonsterFactId};
use crate::{shuffle_order, CombatSearchState};
use serde::{Deserialize, Serialize};
use sts_combat_outcome::combat_witness_materialization::{
    project_observed_combat_outcome_with_expenditures_v1, CombatInputExpendituresV1,
    CombatWitnessMaterializedOutcomeV1,
};
use sts_core::{
    content::powers::{store::has_power, PowerId},
    sim::combat::{CombatPosition, CombatStepper, CombatTerminal, EngineCombatStepper},
    state::core::{ClientInput, EngineState},
    EntityId,
};

/// A purpose to observe along a plan, not an action restriction or reward.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectMilestone {
    EstablishPlayerPower {
        power: PowerId,
    },
    ChangeMonsterFact {
        target: EntityId,
        fact: MonsterFactId,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanProposal {
    pub label: String,
    pub inputs: Vec<ClientInput>,
    /// Positive, increasing offsets from the source's player turn.
    pub player_turns: Vec<u32>,
    #[serde(default)]
    pub milestones: Vec<EffectMilestone>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConsequenceBoundary {
    PlayerControl { turn_offset: u32 },
    PendingChoice,
    Victory,
    Loss,
    Escape,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConsequenceFrame {
    pub after_inputs: usize,
    pub boundary: ConsequenceBoundary,
    pub observation: CombatObservation,
    /// Resources observed at this frame. Only a Victory frame is a combat win.
    pub resources: CombatWitnessMaterializedOutcomeV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct MilestoneObservation {
    pub milestone: EffectMilestone,
    /// None means absent from the supplied trace, not unreachable from its root.
    pub first_observed_after_inputs: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlanConsequences {
    pub label: String,
    /// One-based display turn, matching CombatObservation::turn.
    pub source_turn: u32,
    /// Every frame belongs to this one exact replay. Values are never mixed.
    pub frames: Vec<ConsequenceFrame>,
    /// Frame indices of the first complete player control at each requested turn.
    pub player_turn_frames: Vec<(u32, usize)>,
    pub unreached_player_turns: Vec<u32>,
    pub milestones: Vec<MilestoneObservation>,
    pub replay_inputs: usize,
}

fn monster_fact(p: &CombatPosition, target: EntityId, fact: MonsterFactId) -> Option<i32> {
    monster::decision(p.combat.entities.monsters.iter().find(|m| m.id == target)?)?
        .facts
        .into_iter()
        .find(|entry| entry.fact == fact)
        .map(|entry| entry.value)
}

/// Replay already concrete actions; do not fill missing actions or end a turn.
/// A caller comparing hypotheses must bind this output to the same exact root.
/// This is Oracle evidence, not a distribution under unobserved draw orders.
pub fn plan_consequences(
    source: &CombatPosition,
    proposal: &PlanProposal,
) -> Result<PlanConsequences, String> {
    if proposal.label.trim().is_empty()
        || proposal.player_turns.iter().any(|&turn| turn == 0)
        || proposal.player_turns.windows(2).any(|w| w[0] >= w[1])
    {
        return Err("plan needs a label and positive increasing player-turn offsets".into());
    }
    CombatSearchState::new(source.clone())?;
    if source.engine != EngineState::CombatPlayerTurn
        || source.combat.runtime.combat_smoked
        || EngineCombatStepper.terminal(source) != CombatTerminal::Unresolved
    {
        return Err("plan source must be a complete player-control boundary".into());
    }
    for milestone in &proposal.milestones {
        if let EffectMilestone::ChangeMonsterFact { target, fact } = milestone {
            if monster_fact(source, *target, *fact).is_none() {
                return Err("milestone needs an observable fact on its exact source target".into());
            }
        }
    }
    let states = shuffle_order::replay(source, &proposal.inputs)?;
    let source_turn = source.combat.turn.turn_count;
    let mut frames = Vec::with_capacity(states.len());
    let mut expenditures = CombatInputExpendituresV1::default();
    let mut potion_uses = 0;
    let mut player_turn_frames = vec![];
    let mut milestones: Vec<_> = proposal
        .milestones
        .iter()
        .cloned()
        .map(|milestone| MilestoneObservation {
            milestone,
            first_observed_after_inputs: None,
        })
        .collect();
    for (after_inputs, position) in states.into_iter().enumerate() {
        if after_inputs > 0 {
            let input = &proposal.inputs[after_inputs - 1];
            expenditures.record(input);
            potion_uses += usize::from(matches!(input, ClientInput::UsePotion { .. }));
        }
        let boundary = if position.combat.runtime.combat_smoked {
            ConsequenceBoundary::Escape
        } else {
            match EngineCombatStepper.terminal(&position) {
                CombatTerminal::Win => ConsequenceBoundary::Victory,
                CombatTerminal::Loss => ConsequenceBoundary::Loss,
                CombatTerminal::Unresolved => match position.engine {
                    EngineState::CombatPlayerTurn => ConsequenceBoundary::PlayerControl {
                        turn_offset: position
                            .combat
                            .turn
                            .turn_count
                            .checked_sub(source_turn)
                            .ok_or("plan replay moved backwards in player turns")?,
                    },
                    EngineState::PendingChoice(_) => ConsequenceBoundary::PendingChoice,
                    _ => return Err("plan replay ended outside a stable combat decision".into()),
                },
            }
        };
        if let ConsequenceBoundary::PlayerControl { turn_offset } = boundary {
            if proposal.player_turns.contains(&turn_offset)
                && !player_turn_frames
                    .iter()
                    .any(|&(turn, _)| turn == turn_offset)
            {
                player_turn_frames.push((turn_offset, frames.len()));
            }
        }
        for observed in &mut milestones {
            let reached = match observed.milestone {
                EffectMilestone::EstablishPlayerPower { power } => {
                    has_power(&position.combat, position.combat.entities.player.id, power)
                }
                EffectMilestone::ChangeMonsterFact { target, fact } => {
                    monster_fact(&position, target, fact)
                        .is_some_and(|value| Some(value) != monster_fact(source, target, fact))
                }
            };
            if reached && observed.first_observed_after_inputs.is_none() {
                observed.first_observed_after_inputs = Some(after_inputs);
            }
        }
        let mut state = CombatSearchState::new(position.clone())?;
        state.potion_uses = potion_uses;
        frames.push(ConsequenceFrame {
            after_inputs,
            boundary,
            observation: combat_observation(&state)?,
            resources: project_observed_combat_outcome_with_expenditures_v1(
                source,
                expenditures,
                &position,
            ),
        });
    }
    Ok(PlanConsequences {
        label: proposal.label.clone(),
        source_turn: source_turn.saturating_add(1),
        frames,
        player_turn_frames: player_turn_frames.clone(),
        unreached_player_turns: proposal
            .player_turns
            .iter()
            .copied()
            .filter(|turn| {
                !player_turn_frames
                    .iter()
                    .any(|(reached, _)| reached == turn)
            })
            .collect(),
        milestones,
        replay_inputs: proposal.inputs.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::SelectionDomain;
    use sts_core::{
        content::{
            cards::CardId,
            monsters::EnemyId,
            potions::{Potion, PotionId},
        },
        runtime::combat::{CombatCard, Power, PowerPayload},
    };

    fn root(cards: &[CardId]) -> CombatPosition {
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::Cultist, 1)];
        c.entities.monsters[0].current_hp = 200;
        c.entities.monsters[0].max_hp = 200;
        c.zones.hand = cards
            .iter()
            .enumerate()
            .map(|(i, &id)| CombatCard::for_scenario(id, i as u32 + 1))
            .collect();
        c.zones.draw_pile = (10..18)
            .map(|i| CombatCard::for_scenario(CardId::Wound, i))
            .collect::<Vec<_>>()
            .into();
        CombatPosition::new(EngineState::CombatPlayerTurn, c)
    }

    fn power(p: &mut CombatPosition, owner: EntityId, id: PowerId, amount: i32) {
        p.combat
            .entities
            .power_db
            .entry(owner)
            .or_default()
            .push(Power {
                power_type: id,
                instance_id: None,
                amount,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            });
    }

    fn proposal(inputs: Vec<ClientInput>) -> PlanProposal {
        PlanProposal {
            label: "test plan".into(),
            inputs,
            player_turns: vec![1, 2],
            milestones: vec![],
        }
    }

    fn play(index: usize, target: Option<EntityId>) -> ClientInput {
        ClientInput::PlayCard {
            card_index: index,
            target,
        }
    }

    #[test]
    fn joint_frames_follow_one_exact_trace_without_mutating_the_root() {
        let r = root(&[CardId::Strike, CardId::Defend]);
        let original = r.clone();
        let attack = proposal(vec![play(0, Some(1)), ClientInput::EndTurn]);
        let defend = proposal(vec![play(1, None), ClientInput::EndTurn]);
        let a = plan_consequences(&r, &attack).unwrap();
        let d = plan_consequences(&r, &defend).unwrap();
        assert_eq!(a.player_turn_frames, [(1, 2)]);
        assert_eq!(a.unreached_player_turns, [2]);
        assert_eq!(a.source_turn, a.frames[0].observation.turn);
        for (plan, result) in [(&attack, &a), (&defend, &d)] {
            let states = shuffle_order::replay(&r, &plan.inputs).unwrap();
            for (frame, state) in result.frames.iter().zip(states) {
                assert_eq!(
                    frame.observation,
                    combat_observation(&CombatSearchState::new(state).unwrap()).unwrap()
                );
                assert_eq!(frame.resources.final_hp, frame.observation.hp[0]);
            }
        }
        assert_ne!(a.frames[2].observation.hp, d.frames[2].observation.hp);
        assert_ne!(
            a.frames[2].observation.monsters[0].hp,
            d.frames[2].observation.monsters[0].hp
        );
        assert_eq!(r, original);
        assert!(plan_consequences(&r, &proposal(vec![play(99, None)])).is_err());
        assert_eq!(r, original);
    }

    #[test]
    fn next_turn_choice_is_not_complete_player_control() {
        let mut r = root(&[]);
        power(&mut r, 0, PowerId::ToolsOfTheTrade, 1);
        let next = shuffle_order::step(&r, ClientInput::EndTurn).unwrap();
        assert_eq!(next.combat.turn.turn_count, r.combat.turn.turn_count + 1);
        assert!(matches!(next.engine, EngineState::PendingChoice(_)));
        let incomplete = plan_consequences(&r, &proposal(vec![ClientInput::EndTurn])).unwrap();
        assert_eq!(
            incomplete.frames[1].boundary,
            ConsequenceBoundary::PendingChoice
        );
        assert!(incomplete.player_turn_frames.is_empty());
        assert_eq!(incomplete.unreached_player_turns, [1, 2]);
        let surface = EngineCombatStepper.legal_action_surface(&next);
        let choice = SelectionDomain::new(&surface.selection_families[0])
            .unwrap()
            .compile(&[0])
            .unwrap();
        let complete =
            plan_consequences(&r, &proposal(vec![ClientInput::EndTurn, choice])).unwrap();
        assert_eq!(complete.player_turn_frames, [(1, 2)]);
        assert_eq!(complete.unreached_player_turns, [2]);
        assert!(plan_consequences(&next, &proposal(vec![])).is_err());
    }

    #[test]
    fn a_card_forced_turn_end_reaches_the_same_control_boundary() {
        let mut r = crate::local::ConstructedCombat {
            seed: 42,
            encounter: sts_core::content::monsters::factory::EncounterId::TimeEater,
            act: 3,
            room: sts_core::state::map::node::RoomType::MonsterRoomBoss,
            hp: 68,
            deck: vec![(CardId::Defend, 0); 10],
            relics: vec![],
            potions: vec![],
            max_potion_uses: None,
        }
        .build()
        .unwrap();
        r.combat
            .entities
            .power_db
            .values_mut()
            .flatten()
            .find(|p| p.power_type == PowerId::TimeWarp)
            .unwrap()
            .amount = 11;
        let result = plan_consequences(&r, &proposal(vec![play(0, None)])).unwrap();
        assert_eq!(result.player_turn_frames, [(1, 1)]);
        assert_eq!(
            result.frames[1].boundary,
            ConsequenceBoundary::PlayerControl { turn_offset: 1 }
        );
    }

    #[test]
    fn victory_loss_and_escape_do_not_invent_later_turns() {
        let mut win = root(&[CardId::Strike]);
        win.combat.entities.monsters[0].current_hp = 6;
        let mut loss = root(&[]);
        loss.combat.entities.player.current_hp = 1;
        let mut escape = root(&[]);
        escape.combat.entities.potions = vec![Some(Potion::new(PotionId::SmokeBomb, 1))];
        for (r, input, expected) in [
            (&win, play(0, Some(1)), ConsequenceBoundary::Victory),
            (&loss, ClientInput::EndTurn, ConsequenceBoundary::Loss),
            (
                &escape,
                ClientInput::UsePotion {
                    potion_index: 0,
                    target: None,
                },
                ConsequenceBoundary::Escape,
            ),
        ] {
            let result = plan_consequences(r, &proposal(vec![input])).unwrap();
            assert_eq!(result.frames[1].boundary, expected);
            assert!(result.player_turn_frames.is_empty());
            assert_eq!(result.unreached_player_turns, [1, 2]);
        }
    }

    #[test]
    fn typed_milestones_record_presence_and_controller_changes() {
        let mut r = root(&[CardId::Inflame, CardId::Strike]);
        r.combat.entities.monsters = vec![sts_core::test_support::planned_monster(
            EnemyId::TheGuardian,
            1,
        )];
        r.combat.entities.monsters[0].current_hp = 200;
        r.combat.entities.monsters[0].guardian.is_open = true;
        r.combat.entities.monsters[0].guardian.damage_threshold = 40;
        power(&mut r, 0, PowerId::Metallicize, 4);
        let mut plan = proposal(vec![play(0, None), play(0, Some(1))]);
        plan.milestones = vec![
            EffectMilestone::EstablishPlayerPower {
                power: PowerId::Metallicize,
            },
            EffectMilestone::EstablishPlayerPower {
                power: PowerId::Strength,
            },
            EffectMilestone::ChangeMonsterFact {
                target: 1,
                fact: MonsterFactId::DamageTaken,
            },
            EffectMilestone::ChangeMonsterFact {
                target: 1,
                fact: MonsterFactId::Open,
            },
        ];
        let result = plan_consequences(&r, &plan).unwrap();
        let hits = result
            .milestones
            .iter()
            .map(|m| m.first_observed_after_inputs)
            .collect::<Vec<_>>();
        assert_eq!(hits, [Some(0), Some(1), Some(2), None]);
        plan.milestones.push(EffectMilestone::ChangeMonsterFact {
            target: 999,
            fact: MonsterFactId::Open,
        });
        assert!(plan_consequences(&r, &plan).is_err());
        for turns in [vec![0], vec![2, 1], vec![1, 1]] {
            let mut invalid = proposal(vec![]);
            invalid.player_turns = turns;
            assert!(plan_consequences(&r, &invalid).is_err());
        }
    }

    #[test]
    fn potion_uses_and_discards_preserve_separate_resource_facts() {
        let mut r = root(&[]);
        r.combat.entities.potions = vec![
            Some(Potion::new(PotionId::BlockPotion, 1)),
            Some(Potion::new(PotionId::StrengthPotion, 2)),
        ];
        let result = plan_consequences(
            &r,
            &proposal(vec![
                ClientInput::UsePotion {
                    potion_index: 0,
                    target: None,
                },
                ClientInput::DiscardPotion(1),
            ]),
        )
        .unwrap();
        assert_eq!(result.frames[1].observation.potion_uses, 1);
        assert_eq!(result.frames[2].observation.potion_uses, 1);
        assert_eq!(result.frames[2].resources.total_potion_expenditures, 2);
        assert_eq!(
            result.frames[2].resources.starting_potion_expenditure_slots,
            0b11
        );
        assert!(result.frames[2]
            .observation
            .potions
            .iter()
            .all(Option::is_none));
    }
}
