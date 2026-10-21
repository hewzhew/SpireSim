//! Strategy layer.
//!
//! This is the intended owner of high-level interpretation that is already
//! wired into reward, shop, campfire, boss relic, and deck diagnostics. Scene
//! policies should call into this layer instead of carrying private strategic
//! models.

pub mod acquisition;
pub mod boss_damage_plan;
pub mod boss_encounter_readiness;
pub mod boss_scaling_evidence;
pub mod boss_survival_evidence;
pub mod campfire_upgrade_quality;
pub mod candidate_pressure_response;
pub mod challenger_choice_policy;
pub mod challenger_decision_context;
pub mod challenger_policy_state;
pub mod challenger_signature;
pub mod decision_pipeline;
pub mod deck_admission;
pub mod deck_construction_pressure;
pub mod deck_plan;
pub mod deck_role_inventory;
pub mod deck_strategic_deficit;
pub mod exhaust_corruption_assessment;
pub mod package_state;
pub mod package_transition;
pub mod potion_portfolio;
pub mod power_tempo;
pub mod pressure_assessment;
pub mod relic_expendability;
pub mod reward_admission;
pub mod reward_quality;
pub mod reward_semantic_probe;
pub mod role_saturation;
pub mod run_strategic_facts;
pub mod shop_boss_preview;
pub mod trajectory_comparison;
