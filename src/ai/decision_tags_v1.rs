use crate::content::cards::CardId;

pub const TAG_COLLECTOR_ANSWER: &str = "collector_answer";
pub const TAG_AWAKENED_CULTIST_ANSWER: &str = "awakened_cultist_answer";
pub const TAG_ENGINE_CLOSURE: &str = "engine_closure";
pub const TAG_STARTUP_ACCESS: &str = "startup_access";
pub const TAG_DECK_CLEANING: &str = "deck_cleaning";
pub const TAG_COMBAT_SHAPE_ADDS_STATUS: &str = "combat_shape:adds_status";
pub const TAG_COMBAT_SHAPE_ADDS_SELF_COPY: &str = "combat_shape:adds_self_copy";
pub const TAG_COMBAT_SHAPE_RANDOM_EXHAUST: &str = "combat_shape:random_exhaust";
pub const TAG_COMBAT_SHAPE_TOPDECK_SENSITIVE: &str = "combat_shape:topdeck_sensitive";
pub const TAG_COMBAT_SHAPE_MASS_EXHAUST: &str = "combat_shape:mass_exhaust";
pub const TAG_DIGEST_CAPACITY_STATUS: &str = "digest_capacity:status";
pub const TAG_DIGEST_CAPACITY_EXHAUST: &str = "digest_capacity:exhaust";
pub const TAG_DIGEST_CAPACITY_DRAW: &str = "digest_capacity:draw";
pub const TAG_DIGEST_CAPACITY_TOPDECK: &str = "digest_capacity:topdeck";
pub const TAG_BOSS_PRESSURE_ENEMY_STRENGTH_MULTI_HIT_RISK: &str =
    "boss_pressure:enemy_strength_multi_hit_risk";

pub fn strings_have_tag(items: &[String], tag: &str) -> bool {
    items.iter().any(|item| item == tag)
}

pub fn combat_shape_change_tags_for_card_v1(card: CardId) -> Vec<&'static str> {
    match card {
        CardId::RecklessCharge | CardId::WildStrike | CardId::PowerThrough => {
            vec![TAG_COMBAT_SHAPE_ADDS_STATUS]
        }
        CardId::Anger => vec![TAG_COMBAT_SHAPE_ADDS_SELF_COPY],
        CardId::Havoc => vec![
            TAG_COMBAT_SHAPE_RANDOM_EXHAUST,
            TAG_COMBAT_SHAPE_TOPDECK_SENSITIVE,
        ],
        CardId::SecondWind | CardId::SeverSoul | CardId::FiendFire => {
            vec![TAG_COMBAT_SHAPE_MASS_EXHAUST]
        }
        _ => Vec::new(),
    }
}
