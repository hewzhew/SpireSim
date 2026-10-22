/// Eternal Feather
/// Uncommon Relic
/// For every 5 cards in your deck, heal 3 HP whenever you enter a Rest Site.
///
/// This is entirely handled by the out-of-combat room-entry transition.
pub const DECK_CARDS_PER_HEAL_TRIGGER: usize = 5;
pub const HP_PER_HEAL_TRIGGER: i32 = 3;
pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::EternalFeather,
};

pub fn rest_site_entry_heal(deck_size: usize) -> i32 {
    (deck_size / DECK_CARDS_PER_HEAL_TRIGGER) as i32 * HP_PER_HEAL_TRIGGER
}

pub fn out_of_combat() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_site_entry_heal_uses_complete_five_card_groups() {
        assert_eq!(rest_site_entry_heal(4), 0);
        assert_eq!(rest_site_entry_heal(5), 3);
        assert_eq!(rest_site_entry_heal(30), 18);
    }
}
