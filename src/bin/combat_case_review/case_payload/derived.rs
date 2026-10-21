use sts_oracle_runtime::ai::strategy::deck_strategic_deficit::{
    assess_deck_strategic_deficit, DeckStrategicDeficit,
};
use sts_oracle_runtime::ai::strategy::run_strategic_facts::RunStrategicFacts;
use sts_oracle_runtime::content::cards::{get_card_definition, is_starter_basic, CardType};
use sts_oracle_runtime::content::relics::{energy_master_delta, RelicId};
use sts_oracle_runtime::eval::combat_case::{
    card_summary, CombatCase, CombatCaseCardSummary, CombatCasePathStep,
};

pub(super) struct CombatCaseDerivedPayload {
    pub(super) static_strategic_deficit: DeckStrategicDeficit,
    pub(super) deck: Vec<CombatCaseCardSummary>,
    pub(super) relics: Vec<String>,
    pub(super) potions: Vec<Option<String>>,
    pub(super) path_tail: Vec<CombatCasePathStep>,
}

pub(super) fn derived_payload_from_case(case: &CombatCase) -> CombatCaseDerivedPayload {
    CombatCaseDerivedPayload {
        static_strategic_deficit: assess_deck_strategic_deficit(
            &case.core.position.combat.meta.master_deck_snapshot,
            strategic_facts_from_case(case),
        ),
        deck: case
            .core
            .position
            .combat
            .meta
            .master_deck_snapshot
            .iter()
            .map(card_summary)
            .collect(),
        relics: case
            .core
            .position
            .combat
            .entities
            .player
            .relics
            .iter()
            .map(|relic| relic.id.stable_id().to_string())
            .collect(),
        potions: case
            .core
            .position
            .combat
            .entities
            .potions
            .iter()
            .map(|potion| {
                potion
                    .as_ref()
                    .map(|potion| potion.id.stable_id().to_string())
            })
            .collect(),
        path_tail: case
            .path
            .iter()
            .skip(case.path.len().saturating_sub(12))
            .cloned()
            .collect(),
    }
}

fn strategic_facts_from_case(case: &CombatCase) -> RunStrategicFacts {
    let deck = &case.core.position.combat.meta.master_deck_snapshot;
    RunStrategicFacts {
        entering_act: case.core.run.act,
        starter_basic_count: deck.iter().filter(|card| is_starter_basic(card.id)).count(),
        curse_count: deck
            .iter()
            .filter(|card| get_card_definition(card.id).card_type == CardType::Curse)
            .count(),
        has_energy_relic: case
            .core
            .position
            .combat
            .entities
            .player
            .relics
            .iter()
            .any(|relic| energy_master_delta(relic.id) > 0),
        has_runic_pyramid: case
            .core
            .position
            .combat
            .entities
            .player
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::RunicPyramid),
    }
}
