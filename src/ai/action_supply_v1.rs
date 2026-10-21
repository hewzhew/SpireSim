use crate::ai::card_analysis_v1::{
    card_analysis_profile_v1, CardAnalysisAoeSupportV1, CardAnalysisAttackChunkV1,
    CardAnalysisBossTransitionBurstV1,
};
use crate::ai::card_semantics_v1::{
    card_mechanics_profile_v1, relic_mechanics_profile_v1, ActionSupplyTraitsV1,
    AdditionalPlayScopeV1,
};
use crate::content::cards::{CardId, CardType};
use crate::content::relics::RelicId;
use crate::state::run::RunState;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionSupplySourceV1 {
    Card(CardId),
    Relic(RelicId),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionSupplySourceFactV1 {
    pub source: ActionSupplySourceV1,
    pub traits: ActionSupplyTraitsV1,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ActionSupplyProfileV1 {
    pub opening_once_options: u8,
    pub delayed_per_turn_sources: u8,
    pub same_turn_burst_sources: u8,
    pub triggered_repeatable_sources: u8,
    pub additional_play_sources: u8,
    pub cost_or_resource_compression_sources: u8,
    pub potentially_recursive_sources: u8,
    pub sources: Vec<ActionSupplySourceFactV1>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSupplyDeltaV1 {
    pub opening_once_options: i16,
    pub delayed_per_turn_sources: i16,
    pub same_turn_burst_sources: i16,
    pub triggered_repeatable_sources: i16,
    pub additional_play_sources: i16,
    pub cost_or_resource_compression_sources: i16,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdditionalPlayTargetSupportV1 {
    pub eligible_targets: usize,
    pub high_leverage_targets: usize,
}

impl AdditionalPlayTargetSupportV1 {
    pub fn has_high_leverage_target(self) -> bool {
        self.high_leverage_targets > 0
    }
}

impl ActionSupplyDeltaV1 {
    pub fn has_positive_supply(self) -> bool {
        self.opening_once_options > 0
            || self.delayed_per_turn_sources > 0
            || self.same_turn_burst_sources > 0
            || self.triggered_repeatable_sources > 0
            || self.additional_play_sources > 0
            || self.cost_or_resource_compression_sources > 0
    }
}

impl ActionSupplyProfileV1 {
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    fn record(&mut self, source: ActionSupplySourceV1, traits: ActionSupplyTraitsV1) {
        if traits.is_empty() {
            return;
        }
        self.opening_once_options = self
            .opening_once_options
            .saturating_add(traits.opening_once_options);
        self.delayed_per_turn_sources = self
            .delayed_per_turn_sources
            .saturating_add(u8::from(traits.delayed_per_turn));
        self.same_turn_burst_sources = self
            .same_turn_burst_sources
            .saturating_add(u8::from(traits.same_turn_burst()));
        self.triggered_repeatable_sources = self
            .triggered_repeatable_sources
            .saturating_add(u8::from(traits.triggered_repeatable));
        self.additional_play_sources = self
            .additional_play_sources
            .saturating_add(u8::from(traits.additional_play_scope.is_some()));
        self.cost_or_resource_compression_sources = self
            .cost_or_resource_compression_sources
            .saturating_add(u8::from(traits.cost_or_resource_compression));
        self.potentially_recursive_sources = self
            .potentially_recursive_sources
            .saturating_add(u8::from(traits.potentially_recursive));
        self.sources
            .push(ActionSupplySourceFactV1 { source, traits });
    }
}

pub fn action_supply_profile_v1(run_state: &RunState) -> ActionSupplyProfileV1 {
    let mut profile = ActionSupplyProfileV1::default();
    for relic in &run_state.relics {
        let traits = relic_mechanics_profile_v1(relic.id).action_supply;
        profile.record(ActionSupplySourceV1::Relic(relic.id), traits);
    }
    for card in &run_state.master_deck {
        let traits = card_mechanics_profile_v1(card.id).action_supply;
        profile.record(ActionSupplySourceV1::Card(card.id), traits);
    }
    profile
}

pub fn action_supply_delta_v1(
    before: &ActionSupplyProfileV1,
    after: &ActionSupplyProfileV1,
) -> ActionSupplyDeltaV1 {
    ActionSupplyDeltaV1 {
        opening_once_options: delta_u8(after.opening_once_options, before.opening_once_options),
        delayed_per_turn_sources: delta_u8(
            after.delayed_per_turn_sources,
            before.delayed_per_turn_sources,
        ),
        same_turn_burst_sources: delta_u8(
            after.same_turn_burst_sources,
            before.same_turn_burst_sources,
        ),
        triggered_repeatable_sources: delta_u8(
            after.triggered_repeatable_sources,
            before.triggered_repeatable_sources,
        ),
        additional_play_sources: delta_u8(
            after.additional_play_sources,
            before.additional_play_sources,
        ),
        cost_or_resource_compression_sources: delta_u8(
            after.cost_or_resource_compression_sources,
            before.cost_or_resource_compression_sources,
        ),
    }
}

pub fn additional_play_target_support_v1(
    run_state: &RunState,
    scope: AdditionalPlayScopeV1,
) -> AdditionalPlayTargetSupportV1 {
    let mut support = AdditionalPlayTargetSupportV1::default();
    for card in &run_state.master_deck {
        let analysis = card_analysis_profile_v1(card.id, card.upgrades);
        let eligible = match scope {
            AdditionalPlayScopeV1::Attack => analysis.card_type == CardType::Attack,
        };
        if !eligible {
            continue;
        }
        support.eligible_targets += 1;
        let high_leverage = analysis.attack_chunk >= CardAnalysisAttackChunkV1::Solid
            || analysis.aoe_support == CardAnalysisAoeSupportV1::Strong
            || analysis.has_damage_scaling
            || analysis.has_combat_sustain
            || analysis.boss_transition_burst != CardAnalysisBossTransitionBurstV1::None;
        support.high_leverage_targets += usize::from(high_leverage);
    }
    support
}

fn delta_u8(after: u8, before: u8) -> i16 {
    i16::from(after) - i16::from(before)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::RelicState;
    use crate::test_support::natural_test_run;

    #[test]
    fn run_profile_keeps_opening_supply_neutral_and_repeatable_supply_separate() {
        let mut run = natural_test_run(20260711002, 0, false, "Ironclad");
        run.relics = vec![
            RelicState::new(RelicId::Enchiridion),
            RelicState::new(RelicId::DeadBranch),
        ];
        run.add_card_to_deck(CardId::BladeDance);
        run.add_card_to_deck(CardId::DoubleTap);
        run.add_card_to_deck(CardId::Corruption);

        let profile = action_supply_profile_v1(&run);

        assert_eq!(profile.opening_once_options, 1);
        assert_eq!(profile.delayed_per_turn_sources, 0);
        assert_eq!(profile.same_turn_burst_sources, 1);
        assert_eq!(profile.triggered_repeatable_sources, 1);
        assert_eq!(profile.additional_play_sources, 1);
        assert_eq!(profile.cost_or_resource_compression_sources, 1);
        assert_eq!(profile.potentially_recursive_sources, 1);
        assert_eq!(profile.sources.len(), 5);
    }

    #[test]
    fn unknown_or_ordinary_mechanics_do_not_create_action_supply() {
        let mut run = natural_test_run(20260711003, 0, false, "Ironclad");
        run.add_card_to_deck(CardId::Strike);
        run.relics.push(RelicState::new(RelicId::Vajra));

        assert!(action_supply_profile_v1(&run).is_empty());
    }

    #[test]
    fn delta_reports_only_new_action_supply_sources() {
        let mut before = natural_test_run(20260816001, 0, false, "Ironclad");
        before.add_card_to_deck(CardId::DoubleTap);
        let mut after = before.clone();
        after.relics.push(RelicState::new(RelicId::Nunchaku));

        let delta = action_supply_delta_v1(
            &action_supply_profile_v1(&before),
            &action_supply_profile_v1(&after),
        );

        assert_eq!(delta.additional_play_sources, 0);
        assert_eq!(delta.triggered_repeatable_sources, 1);
        assert_eq!(delta.cost_or_resource_compression_sources, 1);
        assert!(delta.has_positive_supply());
    }

    #[test]
    fn attack_additional_play_requires_a_high_leverage_owned_target() {
        let mut starter_only = natural_test_run(20260816002, 0, false, "Ironclad");
        let starter_support =
            additional_play_target_support_v1(&starter_only, AdditionalPlayScopeV1::Attack);
        assert_eq!(starter_support.eligible_targets, 6);
        assert_eq!(starter_support.high_leverage_targets, 0);

        starter_only.add_card_to_deck(CardId::Carnage);
        starter_only.add_card_to_deck(CardId::Whirlwind);
        let supported =
            additional_play_target_support_v1(&starter_only, AdditionalPlayScopeV1::Attack);
        assert_eq!(supported.eligible_targets, 8);
        assert_eq!(supported.high_leverage_targets, 2);
    }
}
