use super::types::CombatStrategicSite;

pub(super) fn combat_site(enemies: &[String]) -> CombatStrategicSite {
    if enemies.iter().any(|enemy| {
        matches!(
            enemy.as_str(),
            "TheGuardian"
                | "Hexaghost"
                | "SlimeBoss"
                | "BronzeAutomaton"
                | "Champ"
                | "TheCollector"
                | "AwakenedOne"
                | "TimeEater"
                | "Donu"
                | "Deca"
        )
    }) {
        CombatStrategicSite::ActBoss
    } else if enemies.iter().any(|enemy| {
        matches!(
            enemy.as_str(),
            "GremlinNob"
                | "Lagavulin"
                | "Sentry"
                | "GremlinLeader"
                | "BookOfStabbing"
                | "Taskmaster"
                | "Nemesis"
                | "GiantHead"
                | "Reptomancer"
        )
    }) {
        CombatStrategicSite::EliteLike
    } else {
        CombatStrategicSite::HallwayOrUnknown
    }
}
