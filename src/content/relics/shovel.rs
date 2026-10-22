// Shovel: You can now Dig for loot at Rest Sites.
// Metagame logic checked during rest site evaluation.

pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::Shovel,
};

pub fn has_shovel() -> bool {
    true
}
