use crate::content::relics::{energy_master_delta, get_relic_tier, RelicId, RelicState, RelicTier};
use crate::state::run::RunState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RelicTradeJudgmentV1 {
    Protect { reason: &'static str },
    Allow { reason: &'static str },
}

impl RelicTradeJudgmentV1 {
    pub fn protects(&self) -> bool {
        matches!(self, Self::Protect { .. })
    }

    pub fn reason(&self) -> &'static str {
        match self {
            Self::Protect { reason } | Self::Allow { reason } => reason,
        }
    }
}

pub fn nloth_trade_judgment_v1(relic: &RelicState, run_state: &RunState) -> RelicTradeJudgmentV1 {
    if expired_or_resolved_trade_relic(relic) {
        return RelicTradeJudgmentV1::Allow {
            reason: "relic has already delivered most of its value",
        };
    }

    if energy_master_delta(relic.id) > 0 || matches!(relic.id, RelicId::SlaversCollar) {
        return RelicTradeJudgmentV1::Protect {
            reason: "energy relic protects expensive deck execution",
        };
    }

    if core_defense_or_survival_relic(relic.id) {
        return RelicTradeJudgmentV1::Protect {
            reason: "core defense or survival relic protects future fights",
        };
    }

    if core_card_access_relic(relic.id) {
        return RelicTradeJudgmentV1::Protect {
            reason: "card access relic protects deck consistency",
        };
    }

    if ironclad_core_relic(relic.id, run_state) {
        return RelicTradeJudgmentV1::Protect {
            reason: "Ironclad core relic supports the current class plan",
        };
    }

    if get_relic_tier(relic.id) == RelicTier::Boss {
        return RelicTradeJudgmentV1::Protect {
            reason: "unclassified boss relic should not be traded for future rare chance",
        };
    }

    RelicTradeJudgmentV1::Allow {
        reason: "no protected immediate-power role recognized",
    }
}

fn expired_or_resolved_trade_relic(relic: &RelicState) -> bool {
    matches!(
        relic.id,
        RelicId::OldCoin
            | RelicId::TinyHouse
            | RelicId::EmptyCage
            | RelicId::CallingBell
            | RelicId::Astrolabe
            | RelicId::PandorasBox
            | RelicId::FaceOfCleric
            | RelicId::SsserpentHead
            | RelicId::GoldenIdol
            | RelicId::CultistMask
            | RelicId::GremlinMask
            | RelicId::RedMask
    ) || matches!(
        relic.id,
        RelicId::NeowsLament | RelicId::Omamori | RelicId::Matryoshka
    ) && (relic.used_up || relic.counter <= 0)
}

fn core_defense_or_survival_relic(id: RelicId) -> bool {
    crate::ai::card_semantics_v1::relic_mechanics_profile_v1(id).core_defense_or_survival
}

fn core_card_access_relic(id: RelicId) -> bool {
    crate::ai::card_semantics_v1::relic_mechanics_profile_v1(id).core_card_access
}

fn ironclad_core_relic(id: RelicId, run_state: &RunState) -> bool {
    if run_state.player_class != "Ironclad" {
        return false;
    }
    matches!(
        id,
        RelicId::PaperFrog
            | RelicId::ChampionBelt
            | RelicId::CharonsAshes
            | RelicId::Brimstone
            | RelicId::RunicCube
            | RelicId::SelfFormingClay
            | RelicId::Necronomicon
            | RelicId::DeadBranch
            | RelicId::PenNib
            | RelicId::Kunai
            | RelicId::Shuriken
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nloth_trade_policy_protects_energy_relics() {
        let run_state = RunState::new(1, 0, false, "Ironclad");
        let judgment =
            nloth_trade_judgment_v1(&RelicState::new(RelicId::PhilosopherStone), &run_state);

        assert!(judgment.protects());
        assert!(judgment.reason().contains("energy"));
    }

    #[test]
    fn nloth_trade_policy_allows_expired_neows_lament() {
        let run_state = RunState::new(1, 0, false, "Ironclad");
        let mut relic = RelicState::new(RelicId::NeowsLament);
        relic.counter = -2;
        relic.used_up = true;

        let judgment = nloth_trade_judgment_v1(&relic, &run_state);

        assert!(!judgment.protects());
    }
}
