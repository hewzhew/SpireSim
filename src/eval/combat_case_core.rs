use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::runtime::combat::{CardLineageId, CombatCard, CombatState};
use crate::runtime::rng::RngPool;
use crate::sim::combat::CombatPosition;
pub use crate::sim::combat_case_contract::COMBAT_CASE_SCHEMA;

/// Production-independent, exact combat-case payload.
///
/// Run-control diagnostics and production restoration envelopes may surround
/// this core, but combat evaluation and search need only this typed boundary.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatCaseCoreV2 {
    pub schema: String,
    pub source: CombatCaseSource,
    pub gap: CombatCaseGap,
    pub run: CombatCaseRunSummary,
    pub combat: CombatCaseCombatSummary,
    pub run_rng: CombatCaseRngSummary,
    pub position: CombatPosition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseSource {
    pub seed: u64,
    pub ascension: u8,
    pub generation: usize,
    pub branch_id: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseGap {
    pub boundary: String,
    pub reason: String,
    pub witness_budget: CombatCaseWitnessBudgetV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "witness_engine")]
pub enum CombatCaseWitnessBudgetV1 {
    NotRun,
    AtomicExactV2 {
        primary_nodes: usize,
        primary_wall_ms: u64,
        rescue_nodes: usize,
        rescue_wall_ms: u64,
    },
    TurnGraphPortfolioV1 {
        generation_work: usize,
        wall_ms: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseRunSummary {
    pub act: u8,
    pub floor: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub deck_size: usize,
    pub relic_count: usize,
    pub potion_slots: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseCombatSummary {
    pub engine_state: String,
    pub turn: u32,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub energy: u8,
    pub enemies: Vec<String>,
    pub hand: Vec<CombatCaseCardSummary>,
    pub draw_count: usize,
    pub discard_count: usize,
    pub exhaust_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseCardSummary {
    pub id: String,
    pub lineage: CardLineageId,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub upgrades: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseRngSummary {
    pub monster_rng: u32,
    pub event_rng: u32,
    pub merchant_rng: u32,
    pub card_rng: u32,
    pub treasure_rng: u32,
    pub relic_rng: u32,
    pub potion_rng: u32,
    pub monster_hp_rng: u32,
    pub ai_rng: u32,
    pub shuffle_rng: u32,
    pub card_random_rng: u32,
    pub misc_rng: u32,
    pub math_rng: u32,
}

impl CombatCaseCoreV2 {
    pub fn new(
        source: CombatCaseSource,
        gap: CombatCaseGap,
        run: CombatCaseRunSummary,
        run_rng: CombatCaseRngSummary,
        position: CombatPosition,
    ) -> Self {
        Self {
            schema: COMBAT_CASE_SCHEMA.to_string(),
            source,
            gap,
            run,
            combat: combat_summary(&position),
            run_rng,
            position,
        }
    }

    pub fn validate_schema(&self) -> Result<(), String> {
        if self.schema != COMBAT_CASE_SCHEMA {
            return Err(format!("expected combat case, got {}", self.schema));
        }
        Ok(())
    }

    pub fn refresh_derived_summaries(&mut self) {
        self.combat = combat_summary(&self.position);
        self.run.hp = self.position.combat.entities.player.current_hp;
        self.run.max_hp = self.position.combat.entities.player.max_hp;
    }
}

impl CombatCaseRngSummary {
    pub fn from_pool(rng: &RngPool) -> Self {
        Self {
            monster_rng: rng.monster_rng.counter,
            event_rng: rng.event_rng.counter,
            merchant_rng: rng.merchant_rng.counter,
            card_rng: rng.card_rng.counter,
            treasure_rng: rng.treasure_rng.counter,
            relic_rng: rng.relic_rng.counter,
            potion_rng: rng.potion_rng.counter,
            monster_hp_rng: rng.monster_hp_rng.counter,
            ai_rng: rng.ai_rng.counter,
            shuffle_rng: rng.shuffle_rng.counter,
            card_random_rng: rng.card_random_rng.counter,
            misc_rng: rng.misc_rng.counter,
            math_rng: rng.math_rng.counter,
        }
    }
}

pub fn load_combat_case_core_v2(path: &Path) -> Result<CombatCaseCoreV2, String> {
    let payload = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let core: CombatCaseCoreV2 = serde_json::from_str(&payload).map_err(|err| err.to_string())?;
    core.validate_schema()?;
    Ok(core)
}

pub fn combat_summary(position: &CombatPosition) -> CombatCaseCombatSummary {
    let combat = &position.combat;
    CombatCaseCombatSummary {
        engine_state: position.engine.kind_v1().to_string(),
        turn: combat.turn.turn_count,
        hp: combat.entities.player.current_hp,
        max_hp: combat.entities.player.max_hp,
        block: combat.entities.player.block,
        energy: combat.turn.energy,
        enemies: living_enemy_names(combat),
        hand: combat.zones.hand.iter().map(card_summary).collect(),
        draw_count: combat.zones.draw_pile.len(),
        discard_count: combat.zones.discard_pile.len(),
        exhaust_count: combat.zones.exhaust_pile.len(),
    }
}

pub fn living_enemy_names(combat: &CombatState) -> Vec<String> {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .take(3)
        .map(|monster| monster.enemy_id.stable_id().to_owned())
        .collect()
}

pub fn card_summary(card: &CombatCard) -> CombatCaseCardSummary {
    CombatCaseCardSummary {
        id: card.id.stable_id().to_string(),
        lineage: card.lineage,
        upgrades: card.upgrades,
    }
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}
