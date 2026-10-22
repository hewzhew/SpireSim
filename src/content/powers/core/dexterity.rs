pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Dexterity,
        round_duration: false,
    };

pub fn on_calculate_block(mut block: f32, amount: i32) -> f32 {
    block += amount as f32;
    block
}
