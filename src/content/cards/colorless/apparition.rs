use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Apparition,
        name: "Apparition",
        card_type: CardType::Skill,
        rarity: CardRarity::Special,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 1,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: true,
        ethereal: true,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}
