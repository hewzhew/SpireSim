use std::collections::{BTreeMap, BTreeSet};

use crate::ai::card_semantics_v1::{card_mechanics_profile_v1, relic_mechanics_profile_v1};
use crate::content::cards::{get_card_definition, CardId};
use crate::content::relics::RelicId;
use crate::state::run::RunState;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunDebtContractKindV1 {
    CurseDebt,
    ChestCurseOrRelicSkipDebt,
    SmithLock,
    RewardWidthDebt,
    RestLock,
    GoldIncomeLock,
    WoundDeckDebt,
    EnemyStrengthDebt,
    IntentVisibilityDebt,
    RandomCostDeckShapeDebt,
    PotionLock,
    CardPlayCapDebt,
    HealingDisabled,
}

pub fn has_curse_mitigation_or_synergy_v1(run_state: &RunState) -> bool {
    run_state.relics.iter().any(|relic| {
        !relic.used_up
            && ((relic.id == RelicId::Omamori && relic.counter > 0)
                || matches!(
                    relic.id,
                    RelicId::BlueCandle | RelicId::DuVuDoll | RelicId::DarkstonePeriapt
                ))
    })
}

impl RunDebtContractKindV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::CurseDebt => "curse_debt",
            Self::ChestCurseOrRelicSkipDebt => "chest_curse_or_relic_skip_debt",
            Self::SmithLock => "smith_lock",
            Self::RewardWidthDebt => "reward_width_debt",
            Self::RestLock => "rest_lock",
            Self::GoldIncomeLock => "gold_income_lock",
            Self::WoundDeckDebt => "wound_deck_debt",
            Self::EnemyStrengthDebt => "enemy_strength_debt",
            Self::IntentVisibilityDebt => "intent_visibility_debt",
            Self::RandomCostDeckShapeDebt => "random_cost_deck_shape_debt",
            Self::PotionLock => "potion_lock",
            Self::CardPlayCapDebt => "card_play_cap_debt",
            Self::HealingDisabled => "healing_disabled",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunDebtContractV1 {
    pub source: String,
    pub kind: RunDebtContractKindV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mitigators: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aggravators: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl RunDebtContractV1 {
    pub fn compact_label(&self) -> String {
        let mut label = format!("{}={}", self.source, self.kind.label());
        if !self.unresolved.is_empty() {
            label.push_str(" unresolved=");
            label.push_str(&self.unresolved.join(","));
        }
        if !self.mitigators.is_empty() {
            label.push_str(" mitigated=");
            label.push_str(&self.mitigators.join(","));
        }
        if !self.aggravators.is_empty() {
            label.push_str(" aggravated=");
            label.push_str(&self.aggravators.join(","));
        }
        label
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunDebtLedgerV1 {
    pub contracts: Vec<RunDebtContractV1>,
}

impl RunDebtLedgerV1 {
    pub fn is_empty(&self) -> bool {
        self.contracts.is_empty()
    }

    pub fn compact_labels(&self) -> Vec<String> {
        self.contracts
            .iter()
            .map(RunDebtContractV1::compact_label)
            .collect()
    }

    pub fn strategic_debt_tags(&self) -> Vec<String> {
        self.contracts
            .iter()
            .flat_map(|contract| contract.tags.iter().cloned())
            .chain(run_debt_compounding_tags_v1(self))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn compounding_tags(&self) -> Vec<String> {
        run_debt_compounding_tags_v1(self)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunDebtProjectionV1 {
    pub current: RunDebtLedgerV1,
    pub projected: RunDebtLedgerV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_contracts: Vec<RunDebtContractV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compounding_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compounding_labels: Vec<String>,
}

pub fn run_debt_ledger_v1(run_state: &RunState) -> RunDebtLedgerV1 {
    run_debt_ledger_for_relics_v1(run_state, &[])
}

pub fn run_debt_ledger_for_relics_v1(
    run_state: &RunState,
    extra_relics: &[RelicId],
) -> RunDebtLedgerV1 {
    let mut relics = run_state
        .relics
        .iter()
        .map(|relic| relic.id)
        .collect::<Vec<_>>();
    relics.extend(extra_relics.iter().copied());
    relics.sort_by_key(|relic| relic.stable_ordinal());
    relics.dedup();

    let contracts = relics
        .into_iter()
        .filter_map(|relic| run_debt_contract_for_relic_v1(run_state, relic))
        .collect();
    RunDebtLedgerV1 { contracts }
}

pub fn run_debt_projection_for_relic_v1(
    run_state: &RunState,
    relic: RelicId,
) -> RunDebtProjectionV1 {
    let current = run_debt_ledger_v1(run_state);
    let projected = run_debt_ledger_for_relics_v1(run_state, &[relic]);
    let current_keys = current
        .contracts
        .iter()
        .map(run_debt_contract_key_v1)
        .collect::<BTreeSet<_>>();
    let added_contracts = projected
        .contracts
        .iter()
        .filter(|contract| !current_keys.contains(&run_debt_contract_key_v1(contract)))
        .cloned()
        .collect::<Vec<_>>();
    let compounding_tags = projected.compounding_tags();
    let compounding_labels = compounding_tags
        .iter()
        .map(|tag| run_debt_compounding_label_v1(tag).to_string())
        .collect();
    RunDebtProjectionV1 {
        current,
        projected,
        added_contracts,
        compounding_tags,
        compounding_labels,
    }
}

pub fn run_debt_tag_rank_adjustment_v1(tag: &str) -> i32 {
    match tag {
        "relic_constraint:sozu_potion_lock" => -700,
        "relic_constraint:velvet_choker_action_cap" => -900,
        "relic_constraint:runic_dome_hidden_intents" => -800,
        "relic_constraint:coffee_dripper_rest_lock" => -600,
        "run_debt:coffee_dripper:no_recovery_source" => -1_400,
        "run_debt:coffee_dripper:low_hp" => -1_000,
        "run_debt:coffee_dripper:self_damage_aggravator" => -800,
        "run_debt:coffee_dripper:low_survival_support" => -700,
        "run_debt_compound:rest_lock+reward_width" => -1_500,
        "run_debt_compound:rest_lock+smith_lock" => -1_500,
        "run_debt_compound:reward_width+smith_lock" => -1_100,
        "run_debt_compound:rest_lock+card_play_cap" => -1_200,
        "run_debt_compound:random_cost+card_play_cap" => -1_200,
        _ => 0,
    }
}

fn run_debt_contract_key_v1(contract: &RunDebtContractV1) -> (String, RunDebtContractKindV1) {
    (contract.source.clone(), contract.kind)
}

fn run_debt_contract_for_relic_v1(
    run_state: &RunState,
    relic: RelicId,
) -> Option<RunDebtContractV1> {
    let kind = relic_debt_kind_v1(relic)?;
    let mut contract = RunDebtContractV1 {
        source: relic.stable_id().to_string(),
        kind,
        requirements: Vec::new(),
        unresolved: Vec::new(),
        mitigators: Vec::new(),
        aggravators: Vec::new(),
        tags: vec![format!(
            "relic_contract:{}:{}",
            relic_label_v1(relic),
            kind.label()
        )],
    };

    match relic {
        RelicId::Sozu => contract
            .tags
            .push("relic_constraint:sozu_potion_lock".to_string()),
        RelicId::VelvetChoker => contract
            .tags
            .push("relic_constraint:velvet_choker_action_cap".to_string()),
        RelicId::RunicDome => contract
            .tags
            .push("relic_constraint:runic_dome_hidden_intents".to_string()),
        RelicId::CoffeeDripper => add_coffee_dripper_rest_lock_terms(run_state, &mut contract),
        _ => {}
    }

    Some(contract)
}

fn relic_debt_kind_v1(relic: RelicId) -> Option<RunDebtContractKindV1> {
    match relic {
        RelicId::CallingBell => Some(RunDebtContractKindV1::CurseDebt),
        RelicId::CursedKey => Some(RunDebtContractKindV1::ChestCurseOrRelicSkipDebt),
        RelicId::FusionHammer => Some(RunDebtContractKindV1::SmithLock),
        RelicId::BustedCrown => Some(RunDebtContractKindV1::RewardWidthDebt),
        RelicId::CoffeeDripper => Some(RunDebtContractKindV1::RestLock),
        RelicId::Ectoplasm => Some(RunDebtContractKindV1::GoldIncomeLock),
        RelicId::MarkOfPain => Some(RunDebtContractKindV1::WoundDeckDebt),
        RelicId::PhilosopherStone => Some(RunDebtContractKindV1::EnemyStrengthDebt),
        RelicId::RunicDome => Some(RunDebtContractKindV1::IntentVisibilityDebt),
        RelicId::SneckoEye => Some(RunDebtContractKindV1::RandomCostDeckShapeDebt),
        RelicId::Sozu => Some(RunDebtContractKindV1::PotionLock),
        RelicId::VelvetChoker => Some(RunDebtContractKindV1::CardPlayCapDebt),
        RelicId::MarkOfTheBloom => Some(RunDebtContractKindV1::HealingDisabled),
        _ => None,
    }
}

fn run_debt_compounding_tags_v1(ledger: &RunDebtLedgerV1) -> Vec<String> {
    let kinds = ledger
        .contracts
        .iter()
        .map(|contract| contract.kind)
        .collect::<BTreeSet<_>>();
    let mut tags = Vec::new();
    if kinds.contains(&RunDebtContractKindV1::RestLock)
        && kinds.contains(&RunDebtContractKindV1::RewardWidthDebt)
    {
        tags.push("run_debt_compound:rest_lock+reward_width".to_string());
    }
    if kinds.contains(&RunDebtContractKindV1::RestLock)
        && kinds.contains(&RunDebtContractKindV1::SmithLock)
    {
        tags.push("run_debt_compound:rest_lock+smith_lock".to_string());
    }
    if kinds.contains(&RunDebtContractKindV1::RewardWidthDebt)
        && kinds.contains(&RunDebtContractKindV1::SmithLock)
    {
        tags.push("run_debt_compound:reward_width+smith_lock".to_string());
    }
    if kinds.contains(&RunDebtContractKindV1::RestLock)
        && kinds.contains(&RunDebtContractKindV1::CardPlayCapDebt)
    {
        tags.push("run_debt_compound:rest_lock+card_play_cap".to_string());
    }
    if kinds.contains(&RunDebtContractKindV1::RandomCostDeckShapeDebt)
        && kinds.contains(&RunDebtContractKindV1::CardPlayCapDebt)
    {
        tags.push("run_debt_compound:random_cost+card_play_cap".to_string());
    }
    tags
}

fn run_debt_compounding_label_v1(tag: &str) -> &'static str {
    match tag {
        "run_debt_compound:rest_lock+reward_width" => {
            "rest is locked while future card reward width is reduced"
        }
        "run_debt_compound:rest_lock+smith_lock" => {
            "rest is locked while campfire upgrades are also locked"
        }
        "run_debt_compound:reward_width+smith_lock" => {
            "future card reward width and upgrade conversion are both reduced"
        }
        "run_debt_compound:rest_lock+card_play_cap" => {
            "rest is locked while combat recovery through many small actions is capped"
        }
        "run_debt_compound:random_cost+card_play_cap" => {
            "random card costs are combined with a hard card-play cap"
        }
        _ => "multiple boss relic constraints compound",
    }
}

fn add_coffee_dripper_rest_lock_terms(run_state: &RunState, contract: &mut RunDebtContractV1) {
    contract
        .tags
        .push("relic_constraint:coffee_dripper_rest_lock".to_string());
    contract
        .requirements
        .extend(["recovery_source", "hp_loss_control", "self_damage_control"].map(str::to_string));

    let recovery = recovery_sources_v1(run_state);
    if recovery.is_empty() {
        contract.unresolved.push("recovery_source".to_string());
        contract
            .tags
            .push("run_debt:coffee_dripper:no_recovery_source".to_string());
    } else {
        contract.mitigators.extend(recovery);
    }

    let hp_percent = hp_percent_v1(run_state);
    if hp_percent < 60 {
        contract
            .unresolved
            .push(format!("hp_loss_control:{hp_percent}%"));
        contract.aggravators.push(format!("low_hp:{hp_percent}%"));
        contract
            .tags
            .push("run_debt:coffee_dripper:low_hp".to_string());
    }

    let self_damage = self_damage_cards_v1(run_state);
    if !self_damage.is_empty() {
        contract
            .aggravators
            .extend(render_card_counts(&self_damage));
        contract
            .tags
            .push("run_debt:coffee_dripper:self_damage_aggravator".to_string());
        if !has_combat_healing_plan_v1(run_state) {
            contract.unresolved.push("self_damage_control".to_string());
        }
    }

    if low_survival_support_v1(run_state) {
        contract.unresolved.push("survival_buffer".to_string());
        contract
            .tags
            .push("run_debt:coffee_dripper:low_survival_support".to_string());
    }

    contract.unresolved.sort();
    contract.unresolved.dedup();
    contract.mitigators.sort();
    contract.mitigators.dedup();
    contract.aggravators.sort();
    contract.aggravators.dedup();
    contract.tags.sort();
    contract.tags.dedup();
}

fn recovery_sources_v1(run_state: &RunState) -> Vec<String> {
    let mut sources = Vec::new();
    for relic in &run_state.relics {
        if relic_mechanics_profile_v1(relic.id).core_defense_or_survival {
            match relic.id {
                RelicId::BurningBlood => sources.push("BurningBlood(+6 after combat)".to_string()),
                RelicId::BlackBlood => sources.push("BlackBlood(+12 after combat)".to_string()),
                RelicId::BloodVial => sources.push("BloodVial(+2 combat start)".to_string()),
                RelicId::MeatOnTheBone => {
                    sources.push("MeatOnTheBone(low HP recovery)".to_string())
                }
                RelicId::MagicFlower => sources.push("MagicFlower(healing multiplier)".to_string()),
                RelicId::Pantograph => sources.push("Pantograph(boss recovery)".to_string()),
                RelicId::ToyOrnithopter => {
                    sources.push("ToyOrnithopter(potion healing)".to_string())
                }
                RelicId::EternalFeather => {
                    sources.push("EternalFeather(rest-room recovery)".to_string())
                }
                _ => {}
            }
        }
    }
    if run_state
        .master_deck
        .iter()
        .any(|card| card.id == CardId::Reaper)
    {
        sources.push("Reaper(combat healing)".to_string());
    }
    if run_state
        .master_deck
        .iter()
        .any(|card| card.id == CardId::Feed)
    {
        sources.push("Feed(max HP growth)".to_string());
    }
    sources
}

fn has_combat_healing_plan_v1(run_state: &RunState) -> bool {
    run_state
        .master_deck
        .iter()
        .any(|card| matches!(card.id, CardId::Reaper))
        || run_state
            .relics
            .iter()
            .any(|relic| matches!(relic.id, RelicId::BlackBlood | RelicId::MagicFlower))
}

fn hp_percent_v1(run_state: &RunState) -> i32 {
    if run_state.max_hp <= 0 {
        return 0;
    }
    run_state.current_hp.max(0).saturating_mul(100) / run_state.max_hp
}

fn self_damage_cards_v1(run_state: &RunState) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for card in &run_state.master_deck {
        if card_mechanics_profile_v1(card.id).self_damage_source {
            *counts
                .entry(get_card_definition(card.id).name.to_string())
                .or_default() += 1;
        }
    }
    counts
}

fn render_card_counts(counts: &BTreeMap<String, usize>) -> Vec<String> {
    counts
        .iter()
        .map(|(card, count)| {
            if *count <= 1 {
                card.clone()
            } else {
                format!("{card}x{count}")
            }
        })
        .collect()
}

fn low_survival_support_v1(run_state: &RunState) -> bool {
    let survival_cards = run_state
        .master_deck
        .iter()
        .filter(|card| {
            matches!(
                card.id,
                CardId::ShrugItOff
                    | CardId::FlameBarrier
                    | CardId::PowerThrough
                    | CardId::Impervious
                    | CardId::SecondWind
                    | CardId::TrueGrit
                    | CardId::Disarm
                    | CardId::Shockwave
                    | CardId::Uppercut
                    | CardId::Clothesline
                    | CardId::Intimidate
                    | CardId::DarkShackles
                    | CardId::GhostlyArmor
            )
        })
        .count();
    survival_cards <= 1 && run_state.master_deck.len() >= 18
}

fn relic_label_v1(relic: RelicId) -> &'static str {
    match relic {
        RelicId::CallingBell => "calling_bell",
        RelicId::CursedKey => "cursed_key",
        RelicId::FusionHammer => "fusion_hammer",
        RelicId::BustedCrown => "busted_crown",
        RelicId::CoffeeDripper => "coffee_dripper",
        RelicId::Ectoplasm => "ectoplasm",
        RelicId::MarkOfPain => "mark_of_pain",
        RelicId::PhilosopherStone => "philosopher_stone",
        RelicId::RunicDome => "runic_dome",
        RelicId::SneckoEye => "snecko_eye",
        RelicId::Sozu => "sozu",
        RelicId::VelvetChoker => "velvet_choker",
        RelicId::MarkOfTheBloom => "mark_of_the_bloom",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::RelicState;
    use crate::runtime::combat::CombatCard;

    #[test]
    fn coffee_dripper_contract_reports_low_hp_and_self_damage() {
        let mut run_state = RunState::new(7, 0, false, "Ironclad");
        run_state.current_hp = 30;
        run_state.max_hp = 80;
        run_state
            .relics
            .push(RelicState::new(RelicId::CoffeeDripper));
        run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::Hemokinesis, 100));

        let ledger = run_debt_ledger_v1(&run_state);
        let contract = ledger
            .contracts
            .iter()
            .find(|contract| contract.source == "CoffeeDripper")
            .expect("Coffee Dripper should emit a run debt contract");

        assert_eq!(contract.kind, RunDebtContractKindV1::RestLock);
        assert!(contract
            .tags
            .contains(&"relic_constraint:coffee_dripper_rest_lock".to_string()));
        assert!(contract
            .tags
            .contains(&"run_debt:coffee_dripper:low_hp".to_string()));
        assert!(contract
            .tags
            .contains(&"run_debt:coffee_dripper:self_damage_aggravator".to_string()));
        assert!(contract
            .unresolved
            .iter()
            .any(|item| item.starts_with("hp_loss_control")));
        assert!(contract
            .unresolved
            .contains(&"self_damage_control".to_string()));
        assert!(contract
            .aggravators
            .iter()
            .any(|item| item.contains("Hemokinesis")));
    }

    #[test]
    fn coffee_dripper_contract_keeps_mitigators_report_only() {
        let mut run_state = RunState::new(7, 0, false, "Ironclad");
        run_state.current_hp = 70;
        run_state.max_hp = 80;
        run_state
            .relics
            .push(RelicState::new(RelicId::CoffeeDripper));
        run_state
            .relics
            .push(RelicState::new(RelicId::MeatOnTheBone));

        let ledger = run_debt_ledger_v1(&run_state);
        let contract = ledger
            .contracts
            .iter()
            .find(|contract| contract.source == "CoffeeDripper")
            .expect("Coffee Dripper should emit a run debt contract");

        assert!(contract
            .mitigators
            .iter()
            .any(|item| item.contains("MeatOnTheBone")));
        assert!(!contract.unresolved.contains(&"recovery_source".to_string()));
    }

    #[test]
    fn boss_relic_projection_reports_compounded_reward_width_after_rest_lock() {
        let mut run_state = RunState::new(7, 0, false, "Ironclad");
        run_state
            .relics
            .push(RelicState::new(RelicId::CoffeeDripper));

        let projection = run_debt_projection_for_relic_v1(&run_state, RelicId::BustedCrown);

        assert!(projection.added_contracts.iter().any(|contract| {
            contract.source == "BustedCrown"
                && contract.kind == RunDebtContractKindV1::RewardWidthDebt
        }));
        assert!(projection
            .compounding_tags
            .contains(&"run_debt_compound:rest_lock+reward_width".to_string()));
        assert!(run_debt_tag_rank_adjustment_v1("run_debt_compound:rest_lock+reward_width") < 0);
    }

    #[test]
    fn curse_support_is_shared_across_charge_mitigation_and_relic_synergy() {
        for relic in [
            RelicId::BlueCandle,
            RelicId::DuVuDoll,
            RelicId::DarkstonePeriapt,
        ] {
            let mut run_state = RunState::new(8, 0, false, "Ironclad");
            run_state.relics.push(RelicState::new(relic));
            assert!(has_curse_mitigation_or_synergy_v1(&run_state));
        }

        let mut omamori = RunState::new(9, 0, false, "Ironclad");
        omamori.relics.push(RelicState::new(RelicId::Omamori));
        assert!(has_curse_mitigation_or_synergy_v1(&omamori));
        omamori
            .relics
            .iter_mut()
            .find(|relic| relic.id == RelicId::Omamori)
            .expect("Omamori fixture")
            .counter = 0;
        assert!(!has_curse_mitigation_or_synergy_v1(&omamori));
    }
}
