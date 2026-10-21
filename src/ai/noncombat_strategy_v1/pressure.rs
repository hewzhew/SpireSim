use super::types::{StrategyPlanPressureV1, StrategyRouteFutureV1};

pub fn route_pressure_v1(route: Option<&StrategyRouteFutureV1>) -> StrategyPlanPressureV1 {
    let Some(route) = route else {
        return StrategyPlanPressureV1::Medium;
    };
    if forced_first_elite_underprepared_v1(route) {
        return StrategyPlanPressureV1::High;
    }
    pressure_from_count_v1(route.max_early_pressure).max(pressure_from_need_v1(route.avoid_damage))
}

pub fn pressure_from_need_v1(value: f32) -> StrategyPlanPressureV1 {
    if value >= 0.65 {
        StrategyPlanPressureV1::High
    } else if value >= 0.30 {
        StrategyPlanPressureV1::Medium
    } else {
        StrategyPlanPressureV1::Low
    }
}

fn pressure_from_count_v1(value: usize) -> StrategyPlanPressureV1 {
    if value >= 3 {
        StrategyPlanPressureV1::High
    } else if value >= 1 {
        StrategyPlanPressureV1::Medium
    } else {
        StrategyPlanPressureV1::Low
    }
}

fn forced_first_elite_underprepared_v1(route: &StrategyRouteFutureV1) -> bool {
    route.first_elite_forced
        && route.max_hallways_before_first_elite < 2
        && !route.can_bail_to_rest_before_first_elite
        && !route.can_bail_to_shop_before_first_elite
}
