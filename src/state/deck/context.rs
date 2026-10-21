/// A read-only snapshot constructed by `RunState` before submitting deck changes.
#[derive(Debug, Clone)]
pub struct DeckContext {
    pub has_hoarder_mod: bool,

    // Omamori interception
    pub has_omamori: bool,
    pub omamori_charges: i32,

    // Side effects
    pub has_ceramic_fish: bool,
    pub has_darkstone_periapt: bool,

    // Egg upgrades
    pub has_molten_egg: bool,
    pub has_toxic_egg: bool,
    pub has_frozen_egg: bool,
}

impl DeckContext {
    pub fn from_relics(relics: &[crate::content::relics::RelicState]) -> Self {
        use crate::content::relics::RelicId;
        let has = |id| relics.iter().any(|relic| relic.id == id);
        Self {
            has_hoarder_mod: false,
            has_omamori: has(RelicId::Omamori),
            omamori_charges: relics
                .iter()
                .find(|r| r.id == RelicId::Omamori)
                .map_or(0, |r| r.counter),
            has_ceramic_fish: has(RelicId::CeramicFish),
            has_darkstone_periapt: has(RelicId::DarkstonePeriapt),
            has_molten_egg: has(RelicId::MoltenEgg),
            has_toxic_egg: has(RelicId::ToxicEgg),
            has_frozen_egg: has(RelicId::FrozenEgg),
        }
    }
}
