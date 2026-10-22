pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::TungstenRod,
};

/// Modifier: reduce HP loss by 1 (minimum 0)
pub fn modify_hp_loss(amount: i32) -> i32 {
    (amount - 1).max(0)
}
