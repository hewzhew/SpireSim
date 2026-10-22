/// Magic Flower: Healing is 50% more effective during combat.
/// Handled statically inside engine HP modifications (`Heal` action).
/// This hook is essentially a no-op but acts as documentation. (A function could be exported to wrap heals).
pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::MagicFlower,
};

pub fn modify_heal(amount: i32) -> i32 {
    (amount as f32 * 1.5).round() as i32
}
