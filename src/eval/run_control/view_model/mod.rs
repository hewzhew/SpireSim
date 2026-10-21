mod candidates;
mod context;
mod labels;
mod resolution;

use serde::{Deserialize, Serialize};

use crate::ai::planner_core::{PlannerMapRoom, PlannerQuestionMarkOutcome};
use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use crate::runtime::combat::CardLineageId;
use crate::sim::combat::{combat_terminal, stable_boundary};
use crate::state::core::{ClientInput, EngineState};
use crate::state::events::{EventActionKind, EventId};
use crate::state::selection::{SelectionReason, SelectionScope};

use super::RunDecisionAction;

pub(super) use super::session::RunControlSession;
use candidates::decision_candidates;
use context::{decision_context, decision_warnings};
use labels::pending_choice_label;
pub(super) use labels::{boss_label, combat_card_label, monster_name, reward_card_label};
pub use resolution::{CandidateResolution, FollowupBoundary};

#[derive(Clone, Debug, PartialEq)]
pub struct RunControlViewModel {
    pub header: RunControlHeader,
    pub decision: DecisionSummary,
    pub candidates: Vec<DecisionCandidate>,
    pub context: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunControlHeader {
    pub step: u64,
    pub title: String,
    pub location: String,
    pub config: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionSummary {
    pub label: String,
    pub status: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecisionCandidate {
    pub id: String,
    pub label: String,
    pub key: Option<DecisionCandidateKey>,
    pub action: CandidateAction,
    pub note: Option<String>,
    pub resolution: Option<CandidateResolution>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum DecisionCandidateKey {
    RouteSelect {
        x: i32,
        y: i32,
        room_type: Option<PlannerMapRoom>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        question_mark_outcomes: Vec<PlannerQuestionMarkOutcome>,
        uses_wing_boots: bool,
        has_emerald_key: bool,
    },
    RouteCancel,
    RunPotionUse {
        slot: usize,
        potion: PotionId,
        uuid: u32,
    },
    RunPotionDiscard {
        slot: usize,
        potion: PotionId,
        uuid: u32,
    },
    RewardPotionClaim {
        reward_item_index: usize,
        potion: PotionId,
    },
    EventOption {
        event_id: EventId,
        screen: usize,
        option_index: usize,
        action: EventActionKind,
    },
    CardRewardPick {
        reward_item_index: usize,
        option_index: usize,
        card: CardId,
        upgrades: u8,
    },
    CardRewardOpen {
        reward_item_index: usize,
    },
    CardRewardSingingBowl {
        reward_item_index: usize,
        option_index: usize,
    },
    CardRewardSkip {
        reward_item_index: usize,
    },
    BossRelicPick {
        option_index: usize,
        relic: RelicId,
    },
    BossRelicSkip,
    CampfireRest,
    CampfireSmith {
        deck_index: usize,
        card_lineage: CardLineageId,
        card: CardId,
        upgrades: u8,
    },
    CampfireDig,
    CampfireLift,
    CampfireToke {
        deck_index: usize,
        card_lineage: CardLineageId,
        card: CardId,
        upgrades: u8,
    },
    CampfireRecall,
    ShopPurgeCard {
        deck_index: usize,
        card: CardId,
        upgrades: u8,
    },
    ShopBuyCard {
        shop_slot: usize,
        card: CardId,
        upgrades: u8,
        price: i32,
    },
    ShopBuyRelic {
        shop_slot: usize,
        relic: RelicId,
        price: i32,
    },
    ShopBuyPotion {
        shop_slot: usize,
        potion: PotionId,
        price: i32,
    },
    ShopOpenRewards,
    SelectionSubmit {
        scope: SelectionScope,
        reason: SelectionReason,
        min_choices: usize,
        max_choices: usize,
        item_count: usize,
    },
    ShopLeave,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum CandidateAction {
    Execute(RunDecisionAction),
    Parameterized { template: String },
    Unavailable { reason: String },
}

impl CandidateAction {
    pub fn summary(&self) -> String {
        match self {
            CandidateAction::Execute(RunDecisionAction::Input(input)) => client_input_hint(input),
            CandidateAction::Execute(RunDecisionAction::SkipCardReward { reward_item_index }) => {
                format!("skip card reward {reward_item_index}")
            }
            CandidateAction::Execute(RunDecisionAction::SingingBowlCardReward {
                reward_item_index,
            }) => format!("singing bowl card reward {reward_item_index}"),
            CandidateAction::Parameterized { template } => template.clone(),
            CandidateAction::Unavailable { reason } => format!("locked: {reason}"),
        }
    }

    pub fn executable_input(&self) -> Option<ClientInput> {
        match self {
            CandidateAction::Execute(action) => action.executable_input(),
            CandidateAction::Parameterized { .. } | CandidateAction::Unavailable { .. } => None,
        }
    }

    pub fn executable_action(&self) -> Option<RunDecisionAction> {
        match self {
            CandidateAction::Execute(action) => Some(action.clone()),
            CandidateAction::Parameterized { .. } | CandidateAction::Unavailable { .. } => None,
        }
    }

    pub fn executable_action_ref(&self) -> Option<&RunDecisionAction> {
        match self {
            CandidateAction::Execute(action) => Some(action),
            CandidateAction::Parameterized { .. } | CandidateAction::Unavailable { .. } => None,
        }
    }
}

impl From<ClientInput> for CandidateAction {
    fn from(value: ClientInput) -> Self {
        CandidateAction::Execute(value.into())
    }
}

impl From<RunDecisionAction> for CandidateAction {
    fn from(value: RunDecisionAction) -> Self {
        CandidateAction::Execute(value)
    }
}

impl From<String> for CandidateAction {
    fn from(value: String) -> Self {
        CandidateAction::Parameterized { template: value }
    }
}

impl From<&str> for CandidateAction {
    fn from(value: &str) -> Self {
        CandidateAction::Parameterized {
            template: value.to_string(),
        }
    }
}

pub fn client_input_hint(input: &ClientInput) -> String {
    match input {
        ClientInput::PlayCard { card_index, target } => match target {
            Some(target) => format!("play {card_index} {target}"),
            None => format!("play {card_index}"),
        },
        ClientInput::UsePotion {
            potion_index,
            target,
        } => match target {
            Some(target) => format!("potion {potion_index} {target}"),
            None => format!("potion {potion_index}"),
        },
        ClientInput::DiscardPotion(slot) => format!("discard-potion {slot}"),
        ClientInput::EndTurn => "end".to_string(),
        ClientInput::SubmitCardChoice(indices) => format_usize_command("card-choice", indices),
        ClientInput::SubmitDiscoverChoice(idx) => format!("choose {idx}"),
        ClientInput::SelectMapNode(x) => format!("go {x}"),
        ClientInput::FlyToNode(x, y) => format!("fly {x} {y}"),
        ClientInput::SelectEventOption(idx) | ClientInput::EventChoice(idx) => {
            format!("event {idx}")
        }
        ClientInput::CampfireOption(choice) => match choice {
            crate::state::core::CampfireChoice::Rest => "rest".to_string(),
            crate::state::core::CampfireChoice::Smith(idx) => format!("smith {idx}"),
            crate::state::core::CampfireChoice::Dig => "dig".to_string(),
            crate::state::core::CampfireChoice::Lift => "lift".to_string(),
            crate::state::core::CampfireChoice::Toke(idx) => format!("toke {idx}"),
            crate::state::core::CampfireChoice::Recall => "recall".to_string(),
        },
        ClientInput::SubmitScryDiscard(indices) => format_usize_command("scry", indices),
        ClientInput::SubmitSelection(resolution) => {
            let lineages = resolution.selected_card_lineages();
            match resolution.scope {
                SelectionScope::Hand => format_lineage_command("hand-select", &lineages),
                SelectionScope::Grid => format_lineage_command("grid-select", &lineages),
                SelectionScope::Deck => format_lineage_command("deck-select", &lineages),
            }
        }
        ClientInput::ClaimReward(idx) => format!("claim {idx}"),
        ClientInput::OpenRewardOverlay => "rewards".to_string(),
        ClientInput::OpenChest => "open".to_string(),
        ClientInput::SelectCard(idx) => format!("pick {idx}"),
        ClientInput::BuyCard(idx) => format!("buy card {idx}"),
        ClientInput::BuyRelic(idx) => format!("buy relic {idx}"),
        ClientInput::BuyPotion(idx) => format!("buy potion {idx}"),
        ClientInput::PurgeCard(idx) => format!("purge {idx}"),
        ClientInput::SubmitRelicChoice(idx) => format!("relic {idx}"),
        ClientInput::Proceed => "proceed".to_string(),
        ClientInput::Cancel => "cancel".to_string(),
    }
}

fn format_usize_command(command: &str, values: &[usize]) -> String {
    format!(
        "{command} {}",
        values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    )
    .trim()
    .to_string()
}

fn format_lineage_command(command: &str, values: &[CardLineageId]) -> String {
    format!(
        "{command} {}",
        values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    )
    .trim()
    .to_string()
}

pub fn build_run_control_view_model(session: &RunControlSession) -> RunControlViewModel {
    let title = decision_title(session);
    let header = RunControlHeader {
        step: session.decision_step,
        title: title.clone(),
        location: header_location(session),
        config: header_config(session),
    };

    RunControlViewModel {
        header,
        decision: decision_summary(session),
        candidates: build_run_control_candidates(session),
        context: decision_context(session),
        warnings: decision_warnings(session),
    }
}

pub(super) fn build_run_control_candidates(session: &RunControlSession) -> Vec<DecisionCandidate> {
    decision_candidates(session)
}

fn decision_title(session: &RunControlSession) -> String {
    match &session.engine_state {
        EngineState::EventRoom => {
            let Some(event) = session.run_state.event_state.as_ref() else {
                return "Event".to_string();
            };
            if event.id == crate::state::events::EventId::Neow {
                if event.current_screen == 0 {
                    "Neow Intro".to_string()
                } else {
                    "Neow Bonus".to_string()
                }
            } else {
                event.id.stable_id().to_string()
            }
        }
        EngineState::MapNavigation => "Map".to_string(),
        EngineState::MapOverlay { .. } => "Map Preview".to_string(),
        EngineState::RewardOverlay { reward_state, .. }
            if reward_state.pending_card_choice.is_some() =>
        {
            "Card Reward".to_string()
        }
        EngineState::RewardOverlay { .. } => "Reward Overlay".to_string(),
        EngineState::RewardScreen(reward) if reward.pending_card_choice.is_some() => {
            "Card Reward".to_string()
        }
        EngineState::RewardScreen(_) => "Reward Screen".to_string(),
        EngineState::TreasureRoom(_) => "Treasure Room".to_string(),
        EngineState::Campfire => "Campfire".to_string(),
        EngineState::Shop(_) => "Shop".to_string(),
        EngineState::CombatStart(request) => format!("Combat Start {:?}", request.encounter_id),
        EngineState::CombatPlayerTurn | EngineState::CombatProcessing => "Combat".to_string(),
        EngineState::PendingChoice(choice) => format!("Combat {}", pending_choice_label(choice)),
        EngineState::RunPendingChoice(choice) => format!("Run Choice {:?}", choice.reason),
        EngineState::BossRelicSelect(_) => "Boss Relic".to_string(),
        EngineState::DoubleBossComplete => "First boss defeated".to_string(),
        EngineState::Act3VictoryComplete => "Act 3 boss defeated".to_string(),
        EngineState::TrueVictoryComplete => "Heart defeated".to_string(),
        EngineState::GameOver(result) => format!("Game Over {:?}", result),
    }
}

fn decision_summary(session: &RunControlSession) -> DecisionSummary {
    match &session.engine_state {
        EngineState::EventRoom => {
            let Some(event) = session.run_state.event_state.as_ref() else {
                return DecisionSummary {
                    label: "Event state is missing.".to_string(),
                    status: None,
                };
            };
            let options = crate::engine::event_handler::get_event_options(&session.run_state);
            let label = if event.id == crate::state::events::EventId::Neow {
                if event.current_screen == 0 {
                    "Neow greets you.".to_string()
                } else {
                    "Choose a starting bonus.".to_string()
                }
            } else if options.iter().all(|option| option.ui.disabled) {
                format!("{:?}: all visible options are locked.", event.id)
            } else {
                event.id.stable_id().to_string()
            };
            DecisionSummary {
                label,
                status: None,
            }
        }
        EngineState::MapNavigation => DecisionSummary {
            label: "Choose the next room.".to_string(),
            status: None,
        },
        EngineState::MapOverlay { .. } => DecisionSummary {
            label: "Preview the map; choose a room to commit or go back to rewards.".to_string(),
            status: None,
        },
        EngineState::RewardOverlay { reward_state, .. }
            if reward_state.pending_card_choice.is_some() =>
        {
            DecisionSummary {
                label: "Choose a card or return to the overlay reward screen.".to_string(),
                status: None,
            }
        }
        EngineState::RewardOverlay { .. } => DecisionSummary {
            label: "Claim overlay rewards or return to the previous screen.".to_string(),
            status: None,
        },
        EngineState::RewardScreen(reward) if reward.pending_card_choice.is_some() => {
            DecisionSummary {
                label: "Choose a card or return to the reward screen.".to_string(),
                status: None,
            }
        }
        EngineState::RewardScreen(_) => DecisionSummary {
            label: "Claim rewards or proceed.".to_string(),
            status: None,
        },
        EngineState::TreasureRoom(_) => DecisionSummary {
            label: "Open the chest.".to_string(),
            status: None,
        },
        EngineState::Campfire => DecisionSummary {
            label: "Choose a campfire action.".to_string(),
            status: None,
        },
        EngineState::Shop(_) => DecisionSummary {
            label: "Buy, remove a card, or leave.".to_string(),
            status: None,
        },
        EngineState::CombatStart(request) => DecisionSummary {
            label: format!("construct combat for {:?}", request.encounter_id),
            status: None,
        },
        EngineState::CombatPlayerTurn
        | EngineState::CombatProcessing
        | EngineState::PendingChoice(_) => combat_decision_summary(session),
        EngineState::RunPendingChoice(choice) => DecisionSummary {
            label: format!(
                "choose {}-{} deck cards for {:?}",
                choice.min_choices, choice.max_choices, choice.reason
            ),
            status: None,
        },
        EngineState::BossRelicSelect(_) => DecisionSummary {
            label: "Choose a boss relic.".to_string(),
            status: None,
        },
        EngineState::Act3VictoryComplete => DecisionSummary {
            label: "Continue to the Spire Heart.".to_owned(),
            status: None,
        },
        EngineState::TrueVictoryComplete => DecisionSummary {
            label: "Continue to the victory screen.".to_owned(),
            status: None,
        },
        EngineState::DoubleBossComplete => DecisionSummary {
            label: "Continue to the second boss.".to_owned(),
            status: None,
        },
        EngineState::GameOver(result) => DecisionSummary {
            label: format!("{result:?}"),
            status: None,
        },
    }
}

fn combat_decision_summary(session: &RunControlSession) -> DecisionSummary {
    let Some(active) = session.active_combat.as_ref() else {
        return DecisionSummary {
            label: "combat state missing".to_string(),
            status: Some("invalid boundary".to_string()),
        };
    };
    let capture_state = session.current_active_combat_position().ok();
    let stable = capture_state
        .as_ref()
        .is_some_and(|position| stable_boundary(&position.engine, &position.combat));
    let terminal = capture_state
        .as_ref()
        .map(|position| combat_terminal(&position.engine, &position.combat));
    DecisionSummary {
        label: format!("Combat turn {}.", active.combat_state.turn.turn_count + 1,),
        status: Some(format!(
            "hp {}/{} | energy {} | stable_capture={stable} terminal={terminal:?}",
            active.combat_state.entities.player.current_hp,
            active.combat_state.entities.player.max_hp,
            active.combat_state.turn.energy,
        )),
    }
}

fn header_location(session: &RunControlSession) -> String {
    let (player_hp, player_max_hp) = session.visible_player_hp();
    format!(
        "Act {} Floor {} | HP {}/{} | Gold {} | Boss {}",
        session.run_state.act_num,
        session.run_state.floor_num,
        player_hp,
        player_max_hp,
        session.run_state.gold,
        boss_label(&session.run_state)
    )
}

fn header_config(session: &RunControlSession) -> String {
    format!(
        "Seed {} | {} A{}",
        session.run_state.seed, session.run_state.player_class, session.run_state.ascension_level,
    )
}
