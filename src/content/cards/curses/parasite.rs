// Permanent removal is executed by state::deck::manager and run::deck_mutation.
pub const REMOVAL_MAX_HP_LOSS: i32 = 3;
pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Parasite,
    };
