use serde::{Deserialize, Serialize};

crate::stable_gameplay_id! {
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OrbId {
    Empty => ("Empty", 0), // Placeholder for an empty orb slot
    Lightning => ("Lightning", 1),
    Dark => ("Dark", 2),
    Frost => ("Frost", 3),
    Plasma => ("Plasma", 4),
}
}

impl crate::content::identity::GameplayIdentitySurface for OrbId {
    fn display_name_v1(self) -> Option<&'static str> {
        Some(match self {
            Self::Empty => "Empty",
            Self::Lightning => "Lightning",
            Self::Dark => "Dark",
            Self::Frost => "Frost",
            Self::Plasma => "Plasma",
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OrbEntity {
    pub id: OrbId,
    pub base_passive_amount: i32,
    pub base_evoke_amount: i32,
    pub passive_amount: i32,
    pub evoke_amount: i32,
}

impl OrbEntity {
    pub fn new(id: OrbId) -> Self {
        match id {
            OrbId::Empty => OrbEntity {
                id,
                base_passive_amount: 0,
                base_evoke_amount: 0,
                passive_amount: 0,
                evoke_amount: 0,
            },
            OrbId::Lightning => OrbEntity {
                id,
                base_passive_amount: 3,
                base_evoke_amount: 8,
                passive_amount: 3,
                evoke_amount: 8,
            },
            OrbId::Dark => OrbEntity {
                id,
                base_passive_amount: 6,
                base_evoke_amount: 6,
                passive_amount: 6,
                evoke_amount: 6,
            },
            OrbId::Frost => OrbEntity {
                id,
                base_passive_amount: 2,
                base_evoke_amount: 5,
                passive_amount: 2,
                evoke_amount: 5,
            },
            OrbId::Plasma => OrbEntity {
                id,
                base_passive_amount: 1,
                base_evoke_amount: 2,
                passive_amount: 1,
                evoke_amount: 2,
            },
        }
    }
}

crate::stable_gameplay_id! {
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StanceId {
    Neutral => ("Neutral", 0),
    Wrath => ("Wrath", 1),
    Calm => ("Calm", 2),
    Divinity => ("Divinity", 3),
}
}

impl StanceId {
    pub fn as_str(&self) -> &'static str {
        match self {
            StanceId::Neutral => "Neutral",
            StanceId::Wrath => "Wrath",
            StanceId::Calm => "Calm",
            StanceId::Divinity => "Divinity",
        }
    }
}

impl crate::content::identity::GameplayIdentitySurface for StanceId {
    fn display_name_v1(self) -> Option<&'static str> {
        Some(self.as_str())
    }
}
