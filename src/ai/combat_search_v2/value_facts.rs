use super::*;

pub(super) fn terminal_rank(label: SearchTerminalLabel) -> i32 {
    match label {
        SearchTerminalLabel::Win => 2,
        SearchTerminalLabel::Unresolved => 1,
        SearchTerminalLabel::Escape | SearchTerminalLabel::Loss => 0,
    }
}

pub(super) fn total_living_enemy_hp(combat: &CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| monster.current_hp.max(0))
        .sum()
}

pub(super) fn living_enemy_count(combat: &CombatState) -> usize {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::test_support::{blank_test_combat, test_monster};

    #[test]
    fn terminal_rank_orders_win_above_unresolved_above_loss() {
        assert!(
            terminal_rank(SearchTerminalLabel::Win)
                > terminal_rank(SearchTerminalLabel::Unresolved)
        );
        assert!(
            terminal_rank(SearchTerminalLabel::Unresolved)
                > terminal_rank(SearchTerminalLabel::Loss)
        );
    }

    #[test]
    fn living_enemy_facts_ignore_dead_or_zero_hp_monsters() {
        let mut combat = blank_test_combat();
        let mut living = test_monster(EnemyId::JawWorm);
        living.current_hp = 24;
        let mut defeated = test_monster(EnemyId::Cultist);
        defeated.current_hp = 0;
        combat.entities.monsters = vec![living, defeated];

        assert_eq!(living_enemy_count(&combat), 1);
        assert_eq!(total_living_enemy_hp(&combat), 24);
    }
}
