pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "card Block bonus",
    effect: "Adds amount to Block gained from cards before other Block modifiers. Does not add Block by itself.",
    timing: "Persists until another effect changes or removes it.",
    round_duration: false,
};

pub fn on_calculate_block(mut block: f32, amount: i32) -> f32 {
    block += amount as f32;
    block
}
