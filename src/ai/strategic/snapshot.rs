use super::{StrategicDecisionSite, StrategicJob};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct StrategicDeckFacts {
    pub deck_size: usize,
    pub attacks: u8,
    pub skills: u8,
    pub powers: u8,
    pub curses: u8,
    pub starter_strikes: u8,
    pub starter_defends: u8,
    pub draw_sources: u8,
    pub energy_sources: u8,
    pub strength_sources: u8,
    #[serde(default)]
    pub temporary_strength_bursts: u8,
    #[serde(default)]
    pub strength_converters: u8,
    #[serde(default)]
    pub convertible_strength_sources: u8,
    pub strength_payoffs: u8,
    pub weak_sources: u8,
    pub vulnerable_sources: u8,
    pub exhaust_generators: u8,
    pub exhaust_payoffs: u8,
    pub status_generators: u8,
    pub status_payoffs: u8,
    pub total_attack_damage: i32,
    pub total_block: i32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct StrategicRouteFacts {
    pub need_card_rewards: f32,
    pub need_upgrade: f32,
    pub need_heal: f32,
    pub can_take_elite: f32,
    pub avoid_damage: f32,
    pub min_elites: usize,
    pub max_elites: usize,
    pub min_fires: usize,
    pub max_fires: usize,
    pub first_fire_floor: Option<i32>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StrategicSnapshot {
    pub site: StrategicDecisionSite,
    pub act: u8,
    pub floor: i32,
    pub boss: Option<String>,
    pub hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub deck: StrategicDeckFacts,
    pub route: Option<StrategicRouteFacts>,
    pub formation_needs: Vec<StrategicJob>,
}
