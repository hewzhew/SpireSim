//! Finite, analyst-authored recipes. All powers are played normally in Java;
//! the tested transition starts only after those preparation receipts settle.
use super::*;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HpDrawPowerCaseV1 {
    PuzzleCombust,
    PuzzleCombustBuffer,
    PuzzleCombustTungsten,
    CubeCombust,
    CubeCombustBuffer,
    CubeCombustTungsten,
    PuzzleCombustRegret,
    PuzzleCombustRegretBuffer,
    CubeCombustRegret,
    CubeCombustRegretBuffer,
    PuzzleCombustStacked,
    CubeCombustStacked,
    PuzzleBrutality,
    PuzzleBrutalityBuffer,
    PuzzleBrutalityTungsten,
    CubeBrutality,
    CubeBrutalityBuffer,
    CubeBrutalityTungsten,
}

impl HpDrawPowerCaseV1 {
    pub(super) const ALL: &[HpDrawCaseV1] = &[
        HpDrawCaseV1::Power(Self::PuzzleCombust),
        HpDrawCaseV1::Power(Self::PuzzleCombustBuffer),
        HpDrawCaseV1::Power(Self::PuzzleCombustTungsten),
        HpDrawCaseV1::Power(Self::CubeCombust),
        HpDrawCaseV1::Power(Self::CubeCombustBuffer),
        HpDrawCaseV1::Power(Self::CubeCombustTungsten),
        HpDrawCaseV1::Power(Self::PuzzleCombustRegret),
        HpDrawCaseV1::Power(Self::PuzzleCombustRegretBuffer),
        HpDrawCaseV1::Power(Self::CubeCombustRegret),
        HpDrawCaseV1::Power(Self::CubeCombustRegretBuffer),
        HpDrawCaseV1::Power(Self::PuzzleCombustStacked),
        HpDrawCaseV1::Power(Self::CubeCombustStacked),
        HpDrawCaseV1::Power(Self::PuzzleBrutality),
        HpDrawCaseV1::Power(Self::PuzzleBrutalityBuffer),
        HpDrawCaseV1::Power(Self::PuzzleBrutalityTungsten),
        HpDrawCaseV1::Power(Self::CubeBrutality),
        HpDrawCaseV1::Power(Self::CubeBrutalityBuffer),
        HpDrawCaseV1::Power(Self::CubeBrutalityTungsten),
    ];
    pub(super) fn is_brutality(self) -> bool {
        matches!(
            self,
            Self::PuzzleBrutality
                | Self::PuzzleBrutalityBuffer
                | Self::PuzzleBrutalityTungsten
                | Self::CubeBrutality
                | Self::CubeBrutalityBuffer
                | Self::CubeBrutalityTungsten
        )
    }
    pub(super) fn stacked(self) -> bool {
        matches!(self, Self::PuzzleCombustStacked | Self::CubeCombustStacked)
    }
    pub(super) fn regret(self) -> bool {
        matches!(
            self,
            Self::PuzzleCombustRegret
                | Self::PuzzleCombustRegretBuffer
                | Self::CubeCombustRegret
                | Self::CubeCombustRegretBuffer
        )
    }
    pub(super) fn relics(self) -> Vec<RelicId> {
        use RelicId::*;
        let mut relics = vec![match self {
            Self::CubeCombust
            | Self::CubeCombustBuffer
            | Self::CubeCombustTungsten
            | Self::CubeCombustRegret
            | Self::CubeCombustRegretBuffer
            | Self::CubeCombustStacked
            | Self::CubeBrutality
            | Self::CubeBrutalityBuffer
            | Self::CubeBrutalityTungsten => RunicCube,
            _ => CentennialPuzzle,
        }];
        if matches!(
            self,
            Self::PuzzleCombustBuffer
                | Self::CubeCombustBuffer
                | Self::PuzzleCombustRegretBuffer
                | Self::CubeCombustRegretBuffer
                | Self::PuzzleBrutalityBuffer
                | Self::CubeBrutalityBuffer
        ) {
            relics.push(FossilizedHelix);
        }
        if matches!(
            self,
            Self::PuzzleCombustTungsten
                | Self::CubeCombustTungsten
                | Self::PuzzleBrutalityTungsten
                | Self::CubeBrutalityTungsten
        ) {
            relics.push(TungstenRod);
        }
        relics
    }
    pub(super) fn preparations(self) -> Vec<(CardId, u8)> {
        let mut cards = vec![
            (CardId::Rupture, 1),
            (
                if self.is_brutality() {
                    CardId::Brutality
                } else {
                    CardId::Combust
                },
                0,
            ),
        ];
        if self.stacked() {
            cards.push((CardId::Combust, 1));
        }
        cards
    }
    pub(super) fn master_deck(self) -> Vec<(CardId, u8)> {
        use CardId::*;
        // The constructor adds master ingredients to the top, reversing their
        // order. This seed draws recipe slots 3/2/1/7/0 into its opening hand.
        // These are preparation ingredients, never Java/Rust identity evidence.
        // Collection still requires each preparation card to be observed in hand.
        let mut deck = vec![
            (Wound, 0),
            (Wound, 0),
            (Wound, 0),
            (Wound, 0),
            (Burn, 0),
            (Wound, 0),
            (Wound, 0),
            (Wound, 0),
            (Regret, 0),
            (Burn, 0),
        ];
        for (slot, card) in self.preparations().into_iter().enumerate() {
            deck[3 - slot] = card;
        }
        if self.regret() {
            deck[7] = (Regret, 0);
        }
        deck
    }
    pub(super) fn energy(self) -> u8 {
        if self.is_brutality() {
            2
        } else if self.stacked() {
            0
        } else {
            1
        }
    }
    pub(super) fn powers(self) -> Vec<Power> {
        let mut powers = vec![power(PowerId::Rupture, 2, 0)];
        if self.is_brutality() {
            powers.push(power(PowerId::Brutality, 1, 0));
        } else {
            powers.push(power(
                PowerId::Combust,
                if self.stacked() { 12 } else { 5 },
                if self.stacked() { 2 } else { 1 },
            ));
        }
        powers
    }
}

pub(super) fn power(power_type: PowerId, amount: i32, extra_data: i32) -> Power {
    Power {
        power_type,
        amount,
        instance_id: None,
        extra_data,
        payload: sts_oracle_run_control::runtime::combat::PowerPayload::None,
        just_applied: false,
    }
}
