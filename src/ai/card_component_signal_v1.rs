use crate::ai::block_plan_profile_v1::{BlockPlanProfileV1, BlockPlanReadinessV1};
use crate::ai::card_semantics_v1::card_mechanics_profile_v1;
use crate::ai::card_semantics_v1::{CardRewardSemanticProfileV1, CardRewardSemanticRoleV1};
use crate::ai::card_tactical_delivery_v1::card_tactical_delivery_profile_v1;
use crate::ai::deck_startup_profile_v1::{
    startup_energy_candidate_discounted_by_snecko_v1, DeckStartupProfileV1, StatusPayoffAccessV1,
};
use crate::ai::noncombat_strategy_v1::StrategyDeckFormationNeedV1;
use crate::content::cards::CardId;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardComponentRoleV1 {
    Enabler,
    Payoff,
    Lubricant,
    Transition,
    Mitigation,
    Liability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardComponentSignalKindV1 {
    FormationNeedProgress,
    FormationNeedRoleWithoutTacticalDensity,
    MultiAxisTacticalDelivery,
    DamageMitigation,
    DrawEnergyAccess,
    ExhaustAccess,
    ExhaustConversionSupported,
    ExhaustReuseSupported,
    ExhaustReuseUnsupported,
    ExhaustPayoffSupported,
    ExhaustPayoffUnsupported,
    StatusPayoffSupported,
    StatusPayoffUnsupported,
    BlockPayoffSupported,
    BlockPayoffUnsupported,
    DuplicateNoDrawAccessDebt,
    DuplicateAccessRequiresTurnPlanning,
    PayoffWithoutVisibleGapFill,
    SneckoEnergyDiscountDebt,
    OfferingEnergyGainLessReliableUnderSnecko,
    EnergyGainLessReliableUnderSnecko,
    CorruptionDuplicateWithoutPayoffDebt,
    ExhaustEngineEnabler,
    FnpEngineUnlock,
    SelfDamagePayoffSupported,
    SelfDamagePayoffUnsupported,
    StrengthConvertiblePackageUnlock,
    StrengthPayoffConvertibleBurstSupported,
    ConvertibleStrengthRequiresDrawTiming,
    StrengthPayoffTemporaryBurstOnly,
    StrengthPayoffWithoutStableGenerator,
    StrengthPayoffUnsupported,
    StrengthPayoffSupported,
    AdditionalPlayTargetSupported,
    AdditionalPlayTargetUnsupported,
}

impl CardComponentSignalKindV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::FormationNeedProgress => "advances_current_formation_need",
            Self::FormationNeedRoleWithoutTacticalDensity => {
                "formation_need_role_without_tactical_density"
            }
            Self::MultiAxisTacticalDelivery => "multi_axis_tactical_delivery",
            Self::DamageMitigation => "mitigates_enemy_damage",
            Self::DrawEnergyAccess => "improves_access_or_conversion",
            Self::ExhaustAccess => "improves_exhaust_access",
            Self::ExhaustConversionSupported => "exhaust_conversion_has_visible_output_and_payoff",
            Self::ExhaustReuseSupported => "exhaust_reuse_has_owned_self_exhausting_target",
            Self::ExhaustReuseUnsupported => "exhaust_reuse_without_owned_self_exhausting_target",
            Self::ExhaustPayoffSupported => "exhaust_payoff_has_generator",
            Self::ExhaustPayoffUnsupported => "exhaust_payoff_without_generator",
            Self::StatusPayoffSupported => "status_payoff_has_owned_generator",
            Self::StatusPayoffUnsupported => "status_payoff_without_owned_generator",
            Self::BlockPayoffSupported => "block_payoff_has_supported_block_plan",
            Self::BlockPayoffUnsupported => "block_payoff_without_supported_block_plan",
            Self::DuplicateNoDrawAccessDebt => "duplicate_draw_access_applies_no_draw_debuff",
            Self::DuplicateAccessRequiresTurnPlanning => "duplicate_access_requires_turn_planning",
            Self::PayoffWithoutVisibleGapFill => "payoff_without_visible_gap_fill",
            Self::SneckoEnergyDiscountDebt => "snecko_random_cost_discounts_energy_startup",
            Self::OfferingEnergyGainLessReliableUnderSnecko => {
                "offering_energy_gain_is_less_reliable_under_snecko"
            }
            Self::EnergyGainLessReliableUnderSnecko => "energy_gain_is_less_reliable_under_snecko",
            Self::CorruptionDuplicateWithoutPayoffDebt => {
                "deck_shape_nonstacking_power_duplicate_without_payoff"
            }
            Self::ExhaustEngineEnabler => "exhaust_engine_enabler",
            Self::FnpEngineUnlock => "unlocks_fnp_engine",
            Self::SelfDamagePayoffSupported => "self_damage_payoff_has_enabler",
            Self::SelfDamagePayoffUnsupported => "self_damage_payoff_without_enabler",
            Self::StrengthConvertiblePackageUnlock => {
                "candidate_unlocks_convertible_strength_for_existing_payoffs"
            }
            Self::StrengthPayoffConvertibleBurstSupported => {
                "strength_payoff_has_convertible_burst_source"
            }
            Self::ConvertibleStrengthRequiresDrawTiming => {
                "convertible_strength_requires_draw_timing"
            }
            Self::StrengthPayoffTemporaryBurstOnly => "strength_payoff_has_temporary_burst_support",
            Self::StrengthPayoffWithoutStableGenerator => {
                "strength_payoff_without_stable_generator"
            }
            Self::StrengthPayoffUnsupported => "strength_payoff_without_generator",
            Self::StrengthPayoffSupported => "strength_payoff_has_generator",
            Self::AdditionalPlayTargetSupported => "additional_play_has_high_leverage_owned_target",
            Self::AdditionalPlayTargetUnsupported => {
                "additional_play_without_high_leverage_owned_target"
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardComponentSignalContextV1 {
    pub same_card_count: usize,
    pub candidate_upgrades: u8,
    pub formation_needs: Vec<StrategyDeckFormationNeedV1>,
    pub startup: DeckStartupProfileV1,
    pub block_plan: BlockPlanProfileV1,
    pub candidate_unlocks_convertible_strength_payoff: bool,
    pub additional_play_target_supported: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardComponentSignalReportV1 {
    pub card: CardId,
    pub roles: Vec<CardComponentRoleV1>,
    pub positive_signals: Vec<CardComponentSignalKindV1>,
    pub debt_signals: Vec<CardComponentSignalKindV1>,
    pub note_signals: Vec<CardComponentSignalKindV1>,
}

pub fn evaluate_card_component_signals_v1(
    context: &CardComponentSignalContextV1,
    profile: &CardRewardSemanticProfileV1,
) -> CardComponentSignalReportV1 {
    let mut report = CardComponentSignalReportV1 {
        card: profile.card,
        roles: component_roles(profile),
        positive_signals: Vec::new(),
        debt_signals: Vec::new(),
        note_signals: Vec::new(),
    };

    add_generic_components(context, profile, &mut report);
    add_card_specific_components(context, profile.card, &mut report);
    add_formation_need_component(context, profile, &mut report);
    add_unresolved_package_payoff_debts(profile, &mut report);
    report
}

fn add_generic_components(
    context: &CardComponentSignalContextV1,
    profile: &CardRewardSemanticProfileV1,
    report: &mut CardComponentSignalReportV1,
) {
    if card_tactical_delivery_profile_v1(profile.card, context.candidate_upgrades)
        .multi_axis_draw_slot_density
    {
        push_signal(
            &mut report.positive_signals,
            CardComponentSignalKindV1::MultiAxisTacticalDelivery,
        );
    }
    if profile
        .roles
        .contains(&CardRewardSemanticRoleV1::EnemyStrengthDown)
        || profile.roles.contains(&CardRewardSemanticRoleV1::Weak)
    {
        push_signal(
            &mut report.positive_signals,
            CardComponentSignalKindV1::DamageMitigation,
        );
    }
    if effective_draw_energy_access_component(context, profile) {
        push_signal(
            &mut report.positive_signals,
            CardComponentSignalKindV1::DrawEnergyAccess,
        );
    }
    if effective_exhaust_access_component(context, profile) {
        push_signal(
            &mut report.positive_signals,
            CardComponentSignalKindV1::ExhaustAccess,
        );
        if has_immediate_exhaust_conversion_output(profile) {
            push_signal(
                &mut report.positive_signals,
                CardComponentSignalKindV1::ExhaustConversionSupported,
            );
        }
    }
    if context.candidate_unlocks_convertible_strength_payoff {
        push_signal(
            &mut report.positive_signals,
            CardComponentSignalKindV1::StrengthConvertiblePackageUnlock,
        );
        push_signal(
            &mut report.note_signals,
            CardComponentSignalKindV1::ConvertibleStrengthRequiresDrawTiming,
        );
    }
    if profile
        .roles
        .contains(&CardRewardSemanticRoleV1::ExhaustPayoff)
    {
        if context.startup.exhaust_engine_count > 0 {
            push_signal(
                &mut report.positive_signals,
                CardComponentSignalKindV1::ExhaustPayoffSupported,
            );
        } else {
            push_signal(
                &mut report.debt_signals,
                CardComponentSignalKindV1::ExhaustPayoffUnsupported,
            );
        }
    }
    if profile
        .roles
        .contains(&CardRewardSemanticRoleV1::ExhaustReuse)
    {
        if context.startup.exhaust_reuse_target_count > 0 {
            push_signal(
                &mut report.positive_signals,
                CardComponentSignalKindV1::ExhaustReuseSupported,
            );
        } else {
            push_signal(
                &mut report.debt_signals,
                CardComponentSignalKindV1::ExhaustReuseUnsupported,
            );
        }
    }
    if profile
        .roles
        .contains(&CardRewardSemanticRoleV1::StatusPayoff)
    {
        if context.startup.status_payoff_access == StatusPayoffAccessV1::Connected {
            push_signal(
                &mut report.positive_signals,
                CardComponentSignalKindV1::StatusPayoffSupported,
            );
        } else {
            push_signal(
                &mut report.debt_signals,
                CardComponentSignalKindV1::StatusPayoffUnsupported,
            );
        }
    }
    if profile
        .roles
        .contains(&CardRewardSemanticRoleV1::BlockPayoff)
    {
        if context.block_plan.readiness >= BlockPlanReadinessV1::Supported {
            push_signal(
                &mut report.positive_signals,
                CardComponentSignalKindV1::BlockPayoffSupported,
            );
        } else {
            push_signal(
                &mut report.debt_signals,
                CardComponentSignalKindV1::BlockPayoffUnsupported,
            );
        }
    }
    if profile
        .roles
        .contains(&CardRewardSemanticRoleV1::StrengthPayoff)
    {
        add_strength_payoff_component(context, report);
    }
    if profile
        .roles
        .contains(&CardRewardSemanticRoleV1::ActionAmplifier)
    {
        push_signal(
            if context.additional_play_target_supported {
                &mut report.positive_signals
            } else {
                &mut report.debt_signals
            },
            if context.additional_play_target_supported {
                CardComponentSignalKindV1::AdditionalPlayTargetSupported
            } else {
                CardComponentSignalKindV1::AdditionalPlayTargetUnsupported
            },
        );
    }
    if context.same_card_count > 0
        && profile.roles.contains(&CardRewardSemanticRoleV1::CardDraw)
        && card_mechanics_profile_v1(profile.card).applies_no_draw_debuff
    {
        push_signal(
            &mut report.debt_signals,
            CardComponentSignalKindV1::DuplicateNoDrawAccessDebt,
        );
        push_signal(
            &mut report.note_signals,
            CardComponentSignalKindV1::DuplicateAccessRequiresTurnPlanning,
        );
    }
}

fn add_formation_need_component(
    context: &CardComponentSignalContextV1,
    profile: &CardRewardSemanticProfileV1,
    report: &mut CardComponentSignalReportV1,
) {
    if advances_current_need(context, profile, report) {
        push_signal(
            &mut report.positive_signals,
            CardComponentSignalKindV1::FormationNeedProgress,
        );
    }
    if touches_tactical_need_without_density(context, profile) {
        push_signal(
            &mut report.note_signals,
            CardComponentSignalKindV1::FormationNeedRoleWithoutTacticalDensity,
        );
    }
}

fn add_unresolved_package_payoff_debts(
    profile: &CardRewardSemanticProfileV1,
    report: &mut CardComponentSignalReportV1,
) {
    if !profile
        .roles
        .contains(&CardRewardSemanticRoleV1::PackagePayoff)
    {
        return;
    }
    if report
        .positive_signals
        .iter()
        .any(|component| is_package_payoff_or_gap_progress_signal_v1(*component))
    {
        return;
    }
    push_signal(
        &mut report.debt_signals,
        CardComponentSignalKindV1::PayoffWithoutVisibleGapFill,
    );
}

/// Evidence that a package payoff either has concrete support or fills a
/// currently visible formation need.
///
/// This is deliberately broader than `is_concrete_package_support_signal_v1`:
/// it controls the generic "no visible gap fill" debt, not the stronger claim
/// that the payoff's required enabler is already owned.
fn is_package_payoff_or_gap_progress_signal_v1(signal: CardComponentSignalKindV1) -> bool {
    matches!(
        signal,
        CardComponentSignalKindV1::FormationNeedProgress
            | CardComponentSignalKindV1::ExhaustConversionSupported
            | CardComponentSignalKindV1::ExhaustReuseSupported
            | CardComponentSignalKindV1::ExhaustPayoffSupported
            | CardComponentSignalKindV1::StatusPayoffSupported
            | CardComponentSignalKindV1::BlockPayoffSupported
            | CardComponentSignalKindV1::ExhaustEngineEnabler
            | CardComponentSignalKindV1::FnpEngineUnlock
            | CardComponentSignalKindV1::SelfDamagePayoffSupported
            | CardComponentSignalKindV1::StrengthConvertiblePackageUnlock
            | CardComponentSignalKindV1::StrengthPayoffConvertibleBurstSupported
            | CardComponentSignalKindV1::StrengthPayoffSupported
            | CardComponentSignalKindV1::AdditionalPlayTargetSupported
    )
}

/// Concrete evidence that a candidate's package role already has usable support.
///
/// `FormationNeedProgress` is deliberately excluded: it describes progress on
/// a broad deck need, not proof that this payoff has its required enabler.
pub fn is_concrete_package_support_signal_v1(signal: CardComponentSignalKindV1) -> bool {
    matches!(
        signal,
        CardComponentSignalKindV1::ExhaustConversionSupported
            | CardComponentSignalKindV1::ExhaustReuseSupported
            | CardComponentSignalKindV1::ExhaustPayoffSupported
            | CardComponentSignalKindV1::StatusPayoffSupported
            | CardComponentSignalKindV1::BlockPayoffSupported
            | CardComponentSignalKindV1::ExhaustEngineEnabler
            | CardComponentSignalKindV1::FnpEngineUnlock
            | CardComponentSignalKindV1::SelfDamagePayoffSupported
            | CardComponentSignalKindV1::StrengthConvertiblePackageUnlock
            | CardComponentSignalKindV1::StrengthPayoffConvertibleBurstSupported
            | CardComponentSignalKindV1::StrengthPayoffSupported
            | CardComponentSignalKindV1::AdditionalPlayTargetSupported
    )
}

pub fn is_unresolved_package_payoff_debt_signal_v1(signal: CardComponentSignalKindV1) -> bool {
    matches!(
        signal,
        CardComponentSignalKindV1::PayoffWithoutVisibleGapFill
            | CardComponentSignalKindV1::ExhaustReuseUnsupported
            | CardComponentSignalKindV1::ExhaustPayoffUnsupported
            | CardComponentSignalKindV1::StatusPayoffUnsupported
            | CardComponentSignalKindV1::BlockPayoffUnsupported
            | CardComponentSignalKindV1::SelfDamagePayoffUnsupported
            | CardComponentSignalKindV1::StrengthPayoffWithoutStableGenerator
            | CardComponentSignalKindV1::StrengthPayoffUnsupported
            | CardComponentSignalKindV1::AdditionalPlayTargetUnsupported
    )
}

fn add_card_specific_components(
    context: &CardComponentSignalContextV1,
    card: CardId,
    report: &mut CardComponentSignalReportV1,
) {
    match card {
        CardId::Offering | CardId::SeeingRed | CardId::Bloodletting
            if startup_energy_candidate_discounted_by_snecko_v1(&context.startup, card) =>
        {
            push_signal(
                &mut report.debt_signals,
                CardComponentSignalKindV1::SneckoEnergyDiscountDebt,
            );
            if card == CardId::Offering {
                push_signal(
                    &mut report.note_signals,
                    CardComponentSignalKindV1::OfferingEnergyGainLessReliableUnderSnecko,
                );
            } else {
                push_signal(
                    &mut report.note_signals,
                    CardComponentSignalKindV1::EnergyGainLessReliableUnderSnecko,
                );
            }
        }
        CardId::Corruption => {
            if context.startup.has_corruption_duplicate_without_payoff {
                push_signal(
                    &mut report.debt_signals,
                    CardComponentSignalKindV1::CorruptionDuplicateWithoutPayoffDebt,
                );
            } else {
                push_signal(
                    &mut report.positive_signals,
                    CardComponentSignalKindV1::ExhaustEngineEnabler,
                );
            }
            if context.startup.feel_no_pain_count > 0 {
                push_signal(
                    &mut report.positive_signals,
                    CardComponentSignalKindV1::FnpEngineUnlock,
                );
            }
        }
        CardId::FeelNoPain => {}
        CardId::Rupture => {
            if context.startup.self_damage_source_count == 0 {
                push_signal(
                    &mut report.debt_signals,
                    CardComponentSignalKindV1::SelfDamagePayoffUnsupported,
                );
            } else {
                push_signal(
                    &mut report.positive_signals,
                    CardComponentSignalKindV1::SelfDamagePayoffSupported,
                );
            }
        }
        _ => {}
    }
}

fn add_strength_payoff_component(
    context: &CardComponentSignalContextV1,
    report: &mut CardComponentSignalReportV1,
) {
    if context.startup.persistent_strength_source_count == 0 {
        if context.startup.convertible_strength_source_count > 0 {
            push_signal(
                &mut report.positive_signals,
                CardComponentSignalKindV1::StrengthPayoffConvertibleBurstSupported,
            );
            push_signal(
                &mut report.note_signals,
                CardComponentSignalKindV1::ConvertibleStrengthRequiresDrawTiming,
            );
        } else if context.startup.temporary_strength_burst_count > 0 {
            push_signal(
                &mut report.note_signals,
                CardComponentSignalKindV1::StrengthPayoffTemporaryBurstOnly,
            );
            push_signal(
                &mut report.debt_signals,
                CardComponentSignalKindV1::StrengthPayoffWithoutStableGenerator,
            );
        } else {
            push_signal(
                &mut report.debt_signals,
                CardComponentSignalKindV1::StrengthPayoffUnsupported,
            );
        }
    } else {
        push_signal(
            &mut report.positive_signals,
            CardComponentSignalKindV1::StrengthPayoffSupported,
        );
    }
}

fn component_roles(profile: &CardRewardSemanticProfileV1) -> Vec<CardComponentRoleV1> {
    let mut roles = Vec::new();
    if profile.roles.iter().any(|role| {
        matches!(
            role,
            CardRewardSemanticRoleV1::ScalingSource
                | CardRewardSemanticRoleV1::ExhaustGenerator
                | CardRewardSemanticRoleV1::StatusGenerator
                | CardRewardSemanticRoleV1::BlockRetention
                | CardRewardSemanticRoleV1::BlockMultiplier
        )
    }) {
        push_role(&mut roles, CardComponentRoleV1::Enabler);
    }
    if profile.roles.iter().any(|role| {
        matches!(
            role,
            CardRewardSemanticRoleV1::StrengthPayoff
                | CardRewardSemanticRoleV1::BlockPayoff
                | CardRewardSemanticRoleV1::StrikePayoff
                | CardRewardSemanticRoleV1::UpgradePayoff
                | CardRewardSemanticRoleV1::ExhaustReuse
                | CardRewardSemanticRoleV1::ExhaustPayoff
                | CardRewardSemanticRoleV1::EnergyOnExhaust
                | CardRewardSemanticRoleV1::StatusPayoff
                | CardRewardSemanticRoleV1::SelfDamagePayoff
                | CardRewardSemanticRoleV1::CombatExternalPayoff
                | CardRewardSemanticRoleV1::ActionAmplifier
                | CardRewardSemanticRoleV1::PackagePayoff
        )
    }) {
        push_role(&mut roles, CardComponentRoleV1::Payoff);
    }
    if profile.roles.iter().any(|role| {
        matches!(
            role,
            CardRewardSemanticRoleV1::CardDraw
                | CardRewardSemanticRoleV1::CycleAccess
                | CardRewardSemanticRoleV1::EnergySource
                | CardRewardSemanticRoleV1::EnergyOnExhaust
        )
    }) {
        push_role(&mut roles, CardComponentRoleV1::Lubricant);
    }
    if profile.roles.iter().any(|role| {
        matches!(
            role,
            CardRewardSemanticRoleV1::FrontloadDamage
                | CardRewardSemanticRoleV1::Block
                | CardRewardSemanticRoleV1::TemporaryStrengthBurst
        )
    }) {
        push_role(&mut roles, CardComponentRoleV1::Transition);
    }
    if profile.roles.iter().any(|role| {
        matches!(
            role,
            CardRewardSemanticRoleV1::Weak | CardRewardSemanticRoleV1::EnemyStrengthDown
        )
    }) {
        push_role(&mut roles, CardComponentRoleV1::Mitigation);
    }
    if roles.is_empty() {
        push_role(&mut roles, CardComponentRoleV1::Liability);
    }
    roles
}

fn advances_current_need(
    context: &CardComponentSignalContextV1,
    profile: &CardRewardSemanticProfileV1,
    report: &CardComponentSignalReportV1,
) -> bool {
    let tactical_density = candidate_has_tactical_density(context, profile);
    context.formation_needs.iter().any(|need| match need {
        StrategyDeckFormationNeedV1::Frontload => {
            tactical_density && has_frontload_formation_role(profile)
        }
        StrategyDeckFormationNeedV1::Block => tactical_density && has_block_formation_role(profile),
        StrategyDeckFormationNeedV1::Scaling => {
            profile
                .roles
                .contains(&CardRewardSemanticRoleV1::ScalingSource)
                || (profile
                    .roles
                    .contains(&CardRewardSemanticRoleV1::PackagePayoff)
                    && report
                        .positive_signals
                        .iter()
                        .any(|signal| is_concrete_package_support_signal_v1(*signal)))
        }
        StrategyDeckFormationNeedV1::DrawEnergy | StrategyDeckFormationNeedV1::Consistency => {
            effective_draw_energy_access_component(context, profile)
        }
    })
}

fn touches_tactical_need_without_density(
    context: &CardComponentSignalContextV1,
    profile: &CardRewardSemanticProfileV1,
) -> bool {
    !candidate_has_tactical_density(context, profile)
        && context.formation_needs.iter().any(|need| match need {
            StrategyDeckFormationNeedV1::Frontload => has_frontload_formation_role(profile),
            StrategyDeckFormationNeedV1::Block => has_block_formation_role(profile),
            StrategyDeckFormationNeedV1::Scaling
            | StrategyDeckFormationNeedV1::DrawEnergy
            | StrategyDeckFormationNeedV1::Consistency => false,
        })
}

fn has_frontload_formation_role(profile: &CardRewardSemanticProfileV1) -> bool {
    profile
        .roles
        .contains(&CardRewardSemanticRoleV1::FrontloadDamage)
        || profile.roles.contains(&CardRewardSemanticRoleV1::AoeDamage)
        || profile
            .roles
            .contains(&CardRewardSemanticRoleV1::Vulnerable)
        || profile
            .roles
            .contains(&CardRewardSemanticRoleV1::TemporaryStrengthBurst)
}

fn has_block_formation_role(profile: &CardRewardSemanticProfileV1) -> bool {
    profile.roles.contains(&CardRewardSemanticRoleV1::Block)
        || profile.roles.contains(&CardRewardSemanticRoleV1::Weak)
        || profile
            .roles
            .contains(&CardRewardSemanticRoleV1::EnemyStrengthDown)
}

/// Whether one additional draw slot provides more tactical output than a
/// starter-scale card, or supplies timing/target coverage that raw damage and
/// block totals cannot express.
fn candidate_has_tactical_density(
    context: &CardComponentSignalContextV1,
    profile: &CardRewardSemanticProfileV1,
) -> bool {
    let delivery = card_tactical_delivery_profile_v1(profile.card, context.candidate_upgrades);
    delivery.has_draw_slot_density()
        || profile.roles.iter().any(|role| {
            matches!(
                role,
                CardRewardSemanticRoleV1::AoeDamage
                    | CardRewardSemanticRoleV1::EnemyStrengthDown
                    | CardRewardSemanticRoleV1::TemporaryStrengthBurst
            )
        })
}

fn effective_draw_energy_access_component(
    context: &CardComponentSignalContextV1,
    profile: &CardRewardSemanticProfileV1,
) -> bool {
    if startup_energy_candidate_discounted_by_snecko_v1(&context.startup, profile.card) {
        return false;
    }
    profile.roles.contains(&CardRewardSemanticRoleV1::CardDraw)
        || profile
            .roles
            .contains(&CardRewardSemanticRoleV1::CycleAccess)
        || profile
            .roles
            .contains(&CardRewardSemanticRoleV1::EnergySource)
        || (profile
            .roles
            .contains(&CardRewardSemanticRoleV1::EnergyOnExhaust)
            && context.startup.exhaust_engine_count > 0)
}

fn effective_exhaust_access_component(
    context: &CardComponentSignalContextV1,
    profile: &CardRewardSemanticProfileV1,
) -> bool {
    if !profile
        .roles
        .contains(&CardRewardSemanticRoleV1::ExhaustGenerator)
    {
        return false;
    }
    context.startup.exhaust_payoff_count > 0
        || context.startup.feel_no_pain_count > 0
        || context.startup.exhaust_engine_count > 0
}

fn has_immediate_exhaust_conversion_output(profile: &CardRewardSemanticProfileV1) -> bool {
    use crate::content::cards::mechanics::{
        card_base_damage_delivery_v1, CardAttackHitsV1, CardBaseDamageDeliveryV1,
    };
    // This helper is used only after exhaust access/payoff support is found.
    // Hand conversion can produce damage without having a context-free total;
    // it must not borrow a FrontloadDamage role to expose that capability.
    if profile
        .roles
        .contains(&CardRewardSemanticRoleV1::ContextDependentDamage)
        && matches!(
            card_base_damage_delivery_v1(profile.card, 0),
            Some(CardBaseDamageDeliveryV1::Attack {
                hits: CardAttackHitsV1::PerHandCardAtResolutionBeforeExhaust,
                ..
            })
        )
    {
        return true;
    }
    profile.roles.iter().any(|role| {
        matches!(
            role,
            CardRewardSemanticRoleV1::FrontloadDamage
                | CardRewardSemanticRoleV1::AoeDamage
                | CardRewardSemanticRoleV1::Block
                | CardRewardSemanticRoleV1::CardDraw
                | CardRewardSemanticRoleV1::CycleAccess
                | CardRewardSemanticRoleV1::EnergySource
                | CardRewardSemanticRoleV1::Weak
                | CardRewardSemanticRoleV1::EnemyStrengthDown
        )
    })
}

fn push_role(roles: &mut Vec<CardComponentRoleV1>, role: CardComponentRoleV1) {
    if !roles.contains(&role) {
        roles.push(role);
    }
}

fn push_signal(values: &mut Vec<CardComponentSignalKindV1>, value: CardComponentSignalKindV1) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::card_semantics_v1::card_reward_semantic_profile_v1;
    use crate::state::rewards::RewardCard;
    use CardComponentSignalKindV1::*;

    fn context() -> CardComponentSignalContextV1 {
        CardComponentSignalContextV1 {
            same_card_count: 0,
            candidate_upgrades: 0,
            formation_needs: vec![StrategyDeckFormationNeedV1::Consistency],
            startup: DeckStartupProfileV1::default(),
            block_plan: BlockPlanProfileV1::default(),
            candidate_unlocks_convertible_strength_payoff: false,
            additional_play_target_supported: false,
        }
    }

    #[test]
    fn disarm_emits_generic_mitigation_signal() {
        let report = evaluate_card_component_signals_v1(
            &context(),
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::Disarm, 0)),
        );

        assert!(report.positive_signals.contains(&DamageMitigation));
    }

    #[test]
    fn rupture_without_self_damage_is_skip_preferred_not_package_support() {
        let report = evaluate_card_component_signals_v1(
            &context(),
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::Rupture, 0)),
        );

        assert!(report.debt_signals.contains(&SelfDamagePayoffUnsupported));
    }

    #[test]
    fn strength_payoff_distinguishes_temporary_and_convertible_strength() {
        let mut burst_only = context();
        burst_only.startup.temporary_strength_burst_count = 1;
        let burst_report = evaluate_card_component_signals_v1(
            &burst_only,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::HeavyBlade, 0)),
        );

        assert!(burst_report
            .note_signals
            .contains(&StrengthPayoffTemporaryBurstOnly));
        assert!(burst_report
            .debt_signals
            .contains(&StrengthPayoffWithoutStableGenerator));
        assert!(!burst_report
            .positive_signals
            .contains(&StrengthPayoffSupported));

        let mut convertible = context();
        convertible.startup.temporary_strength_burst_count = 1;
        convertible.startup.strength_converter_count = 1;
        convertible.startup.convertible_strength_source_count = 1;
        let convertible_report = evaluate_card_component_signals_v1(
            &convertible,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::HeavyBlade, 0)),
        );

        assert!(convertible_report
            .positive_signals
            .contains(&StrengthPayoffConvertibleBurstSupported));
        assert!(convertible_report
            .note_signals
            .contains(&ConvertibleStrengthRequiresDrawTiming));
    }

    #[test]
    fn candidate_strength_package_unlock_is_concrete_support_with_timing_note() {
        let mut context = context();
        context.candidate_unlocks_convertible_strength_payoff = true;
        let report = evaluate_card_component_signals_v1(
            &context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::Flex, 1)),
        );

        assert!(report
            .positive_signals
            .contains(&StrengthConvertiblePackageUnlock));
        assert!(report
            .note_signals
            .contains(&ConvertibleStrengthRequiresDrawTiming));
        assert!(report
            .positive_signals
            .iter()
            .any(|signal| is_concrete_package_support_signal_v1(*signal)));
    }

    #[test]
    fn duplicate_no_draw_access_card_emits_structural_debt() {
        let mut context = context();
        context.same_card_count = 1;

        let report = evaluate_card_component_signals_v1(
            &context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::BattleTrance, 0)),
        );

        assert!(report.debt_signals.contains(&DuplicateNoDrawAccessDebt));
        assert!(report
            .note_signals
            .contains(&DuplicateAccessRequiresTurnPlanning));
    }

    #[test]
    fn offering_under_snecko_low_cost_volatility_is_not_clean_startup_access() {
        let mut context = context();
        context.startup.has_snecko_eye = true;
        context.startup.has_snecko_low_cost_volatility = true;
        context.startup.snecko_random_cost_debt = 1;
        context.startup.has_snecko_offering_reliability_debt = true;

        let report = evaluate_card_component_signals_v1(
            &context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::Offering, 0)),
        );

        assert!(report.debt_signals.contains(&SneckoEnergyDiscountDebt));
        assert!(report
            .note_signals
            .contains(&OfferingEnergyGainLessReliableUnderSnecko));
    }

    #[test]
    fn exhaust_generator_is_exhaust_access_not_draw_energy_lubricant() {
        let mut context = context();
        context.startup.exhaust_payoff_count = 1;
        context.startup.exhaust_engine_count = 1;

        let report = evaluate_card_component_signals_v1(
            &context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::SeverSoul, 1)),
        );

        assert!(report.positive_signals.contains(&ExhaustAccess));
        assert!(report
            .positive_signals
            .contains(&ExhaustConversionSupported));
        assert!(!report.positive_signals.contains(&DrawEnergyAccess));
        assert!(!report.roles.contains(&CardComponentRoleV1::Lubricant));
    }

    #[test]
    fn supported_exhaust_conversion_covers_its_package_despite_secondary_payoff_debt() {
        let mut context = context();
        context.startup.feel_no_pain_count = 2;
        context.startup.exhaust_payoff_count = 1;
        context.startup.exhaust_engine_count = 2;

        let profile = card_reward_semantic_profile_v1(&RewardCard::new(CardId::FiendFire, 1));
        assert!(profile
            .roles
            .contains(&CardRewardSemanticRoleV1::ContextDependentDamage));
        assert!(!profile
            .roles
            .contains(&CardRewardSemanticRoleV1::FrontloadDamage));
        let report = evaluate_card_component_signals_v1(&context, &profile);

        assert!(report.positive_signals.contains(&ExhaustAccess));
        assert!(report
            .positive_signals
            .contains(&ExhaustConversionSupported));
        assert!(report.debt_signals.contains(&StrengthPayoffUnsupported));
        assert!(!report.debt_signals.contains(&PayoffWithoutVisibleGapFill));
    }

    #[test]
    fn exhume_distinguishes_owned_replay_target_from_speculative_exhaust() {
        let profile = card_reward_semantic_profile_v1(&RewardCard::new(CardId::Exhume, 0));

        let unsupported = evaluate_card_component_signals_v1(&context(), &profile);
        assert!(unsupported.debt_signals.contains(&ExhaustReuseUnsupported));
        assert!(!unsupported
            .positive_signals
            .contains(&ExhaustReuseSupported));

        let mut supported_context = context();
        supported_context.startup.exhaust_reuse_target_count = 1;
        let supported = evaluate_card_component_signals_v1(&supported_context, &profile);
        assert!(supported.positive_signals.contains(&ExhaustReuseSupported));
        assert!(!supported.debt_signals.contains(&ExhaustReuseUnsupported));
    }

    #[test]
    fn exhaust_payoff_without_generator_emits_structural_debt() {
        let report = evaluate_card_component_signals_v1(
            &context(),
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::DarkEmbrace, 0)),
        );

        assert!(report.debt_signals.contains(&ExhaustPayoffUnsupported));
        assert!(report.debt_signals.contains(&PayoffWithoutVisibleGapFill));
    }

    #[test]
    fn exhaust_payoff_with_generator_clears_unresolved_payoff_debt() {
        let mut context = context();
        context.startup.exhaust_engine_count = 1;

        let report = evaluate_card_component_signals_v1(
            &context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::DarkEmbrace, 0)),
        );

        assert!(report.positive_signals.contains(&ExhaustPayoffSupported));
        assert!(!report.debt_signals.contains(&PayoffWithoutVisibleGapFill));
    }

    #[test]
    fn formation_need_does_not_masquerade_as_status_payoff_support() {
        let mut context = context();
        context.formation_needs = vec![StrategyDeckFormationNeedV1::Scaling];

        let report = evaluate_card_component_signals_v1(
            &context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::FireBreathing, 0)),
        );

        assert!(!report.positive_signals.contains(&FormationNeedProgress));
        assert!(report.debt_signals.contains(&StatusPayoffUnsupported));
        assert!(report.debt_signals.contains(&PayoffWithoutVisibleGapFill));
        assert!(!report
            .positive_signals
            .iter()
            .any(|signal| is_concrete_package_support_signal_v1(*signal)));
    }

    #[test]
    fn owned_status_generator_is_concrete_support_for_status_payoffs() {
        let mut context = context();
        context.startup.status_generator_count = 1;
        context.startup.status_payoff_access = StatusPayoffAccessV1::Connected;
        context.formation_needs = vec![StrategyDeckFormationNeedV1::Scaling];

        for card in [CardId::Evolve, CardId::FireBreathing] {
            let report = evaluate_card_component_signals_v1(
                &context,
                &card_reward_semantic_profile_v1(&RewardCard::new(card, 0)),
            );

            assert!(report.positive_signals.contains(&StatusPayoffSupported));
            assert!(report.positive_signals.contains(&FormationNeedProgress));
            assert!(!report.debt_signals.contains(&StatusPayoffUnsupported));
            assert!(!report.debt_signals.contains(&PayoffWithoutVisibleGapFill));
            assert!(report
                .positive_signals
                .iter()
                .any(|signal| is_concrete_package_support_signal_v1(*signal)));
        }
    }

    #[test]
    fn tactical_formation_progress_distinguishes_weak_halves_from_multi_axis_delivery() {
        let mut context = context();
        context.formation_needs = vec![
            StrategyDeckFormationNeedV1::Frontload,
            StrategyDeckFormationNeedV1::Block,
        ];

        let iron_wave = evaluate_card_component_signals_v1(
            &context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::IronWave, 0)),
        );
        assert!(!iron_wave.positive_signals.contains(&FormationNeedProgress));
        assert!(iron_wave
            .note_signals
            .contains(&FormationNeedRoleWithoutTacticalDensity));

        context.candidate_upgrades = 1;
        let upgraded_iron_wave = evaluate_card_component_signals_v1(
            &context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::IronWave, 1)),
        );
        assert!(!upgraded_iron_wave
            .positive_signals
            .contains(&FormationNeedProgress));
        assert!(upgraded_iron_wave
            .note_signals
            .contains(&FormationNeedRoleWithoutTacticalDensity));

        context.candidate_upgrades = 0;
        let shrug = evaluate_card_component_signals_v1(
            &context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::ShrugItOff, 0)),
        );
        assert!(shrug.positive_signals.contains(&FormationNeedProgress));

        let uppercut = evaluate_card_component_signals_v1(
            &context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::Uppercut, 0)),
        );
        assert!(uppercut
            .positive_signals
            .contains(&MultiAxisTacticalDelivery));
        assert!(uppercut.positive_signals.contains(&FormationNeedProgress));
        assert!(!uppercut
            .note_signals
            .contains(&FormationNeedRoleWithoutTacticalDensity));
    }

    #[test]
    fn block_payoff_requires_a_supported_block_plan() {
        let unsupported = evaluate_card_component_signals_v1(
            &context(),
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::BodySlam, 1)),
        );

        assert!(unsupported.debt_signals.contains(&BlockPayoffUnsupported));
        assert!(unsupported
            .debt_signals
            .contains(&PayoffWithoutVisibleGapFill));

        let mut supported_context = context();
        supported_context.block_plan.readiness = BlockPlanReadinessV1::Supported;
        let supported = evaluate_card_component_signals_v1(
            &supported_context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::BodySlam, 1)),
        );

        assert!(supported.positive_signals.contains(&BlockPayoffSupported));
        assert!(!supported.debt_signals.contains(&BlockPayoffUnsupported));
        assert!(!supported
            .debt_signals
            .contains(&PayoffWithoutVisibleGapFill));
    }

    #[test]
    fn corruption_with_fnp_emits_engine_signal_without_boss_tax_verdict() {
        let mut context = context();
        context.startup.feel_no_pain_count = 1;
        let report = evaluate_card_component_signals_v1(
            &context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::Corruption, 0)),
        );

        assert!(report.positive_signals.contains(&FnpEngineUnlock));
        assert!(report.note_signals.is_empty());
    }

    #[test]
    fn duplicate_corruption_without_payoff_reports_shape_debt() {
        let mut context = context();
        context.startup.corruption_count = 1;
        context.startup.exhaust_payoff_count = 0;
        context.startup.has_corruption_duplicate_without_payoff = true;

        let report = evaluate_card_component_signals_v1(
            &context,
            &card_reward_semantic_profile_v1(&RewardCard::new(CardId::Corruption, 0)),
        );

        assert!(!report.positive_signals.contains(&ExhaustEngineEnabler));
        assert!(report
            .debt_signals
            .contains(&CorruptionDuplicateWithoutPayoffDebt));
    }
}
