use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::LiveForever,
        name: "Live Forever",
        card_type: CardType::Power,
        rarity: CardRarity::Special,
        cost: -2,
        base_damage: 0,
        base_block: 0,
        base_magic: 6,
        target: CardTarget::None,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 2,
    }
}
