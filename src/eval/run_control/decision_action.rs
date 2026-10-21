use crate::state::core::ClientInput;
use serde::{Deserialize, Serialize};

/// A concrete, atomic action accepted by the run execution boundary.
///
/// This contains only state-changing actions that machine callers may execute;
/// retired interactive parsing and display operations are intentionally absent.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum RunDecisionAction {
    Input(ClientInput),
    SkipCardReward { reward_item_index: usize },
    SingingBowlCardReward { reward_item_index: usize },
}

impl RunDecisionAction {
    pub fn executable_input(&self) -> Option<ClientInput> {
        match self {
            Self::Input(input) => Some(input.clone()),
            Self::SkipCardReward { .. } | Self::SingingBowlCardReward { .. } => None,
        }
    }
}

impl From<ClientInput> for RunDecisionAction {
    fn from(input: ClientInput) -> Self {
        Self::Input(input)
    }
}
