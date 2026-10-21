use serde::Serialize;

use crate::content::relics::RelicId;
use crate::runtime::combat::CombatState;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CombatSearchV2PolicyEvidenceReport {
    pub information_access: CombatSearchV2InformationAccess,
    pub public_safe: bool,
    pub hidden_information_risks: Vec<CombatSearchV2HiddenInformationRisk>,
}

impl CombatSearchV2PolicyEvidenceReport {
    pub fn information_access_label(&self) -> &'static str {
        self.information_access.label()
    }

    pub fn public_safety_label(&self) -> &'static str {
        if self.public_safe {
            "public_safe"
        } else {
            "not_public_safe"
        }
    }

    pub fn hidden_information_risk_labels_csv(&self) -> String {
        self.hidden_information_risks
            .iter()
            .map(|risk| risk.label())
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn machine_summary(&self) -> String {
        format!(
            "information_access={} public_safe={} hidden_risks={}",
            self.information_access_label(),
            self.public_safe,
            self.hidden_information_risk_labels_csv()
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchV2InformationAccess {
    PublicObservation,
    PrivilegedSimulator,
    DebugRaw,
}

impl CombatSearchV2InformationAccess {
    pub fn label(self) -> &'static str {
        match self {
            Self::PublicObservation => "public_observation",
            Self::PrivilegedSimulator => "privileged_simulator",
            Self::DebugRaw => "debug_raw",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "public_observation" => Some(Self::PublicObservation),
            "privileged_simulator" => Some(Self::PrivilegedSimulator),
            "debug_raw" => Some(Self::DebugRaw),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchV2HiddenInformationRisk {
    PrivilegedSimulatorState,
    ExactRngState,
    ExactDrawPileOrderWithoutFrozenEye,
    ExactMonsterIntentUnderRunicDome,
}

impl CombatSearchV2HiddenInformationRisk {
    pub fn label(self) -> &'static str {
        match self {
            Self::PrivilegedSimulatorState => "privileged_simulator_state",
            Self::ExactRngState => "exact_rng_state",
            Self::ExactDrawPileOrderWithoutFrozenEye => "exact_draw_pile_order_without_frozen_eye",
            Self::ExactMonsterIntentUnderRunicDome => "exact_monster_intent_under_runic_dome",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "privileged_simulator_state" => Some(Self::PrivilegedSimulatorState),
            "exact_rng_state" => Some(Self::ExactRngState),
            "exact_draw_pile_order_without_frozen_eye" => {
                Some(Self::ExactDrawPileOrderWithoutFrozenEye)
            }
            "exact_monster_intent_under_runic_dome" => Some(Self::ExactMonsterIntentUnderRunicDome),
            _ => None,
        }
    }
}

pub fn combat_search_policy_evidence_for_combat(
    combat: &CombatState,
) -> CombatSearchV2PolicyEvidenceReport {
    let mut hidden_information_risks = vec![
        CombatSearchV2HiddenInformationRisk::PrivilegedSimulatorState,
        CombatSearchV2HiddenInformationRisk::ExactRngState,
    ];

    if !combat.entities.player.has_relic(RelicId::FrozenEye) && combat.zones.draw_pile.len() > 1 {
        hidden_information_risks
            .push(CombatSearchV2HiddenInformationRisk::ExactDrawPileOrderWithoutFrozenEye);
    }

    if combat.entities.player.has_relic(RelicId::RunicDome)
        && combat
            .entities
            .monsters
            .iter()
            .any(|monster| monster.is_alive_for_action())
    {
        hidden_information_risks
            .push(CombatSearchV2HiddenInformationRisk::ExactMonsterIntentUnderRunicDome);
    }

    CombatSearchV2PolicyEvidenceReport {
        information_access: CombatSearchV2InformationAccess::PrivilegedSimulator,
        public_safe: false,
        hidden_information_risks,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        combat_search_policy_evidence_for_combat, CombatSearchV2HiddenInformationRisk,
        CombatSearchV2InformationAccess, CombatSearchV2PolicyEvidenceReport,
    };
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::CombatCard;
    use crate::test_support::{blank_test_combat, test_monster};

    #[test]
    fn policy_evidence_labels_match_serialized_strings() {
        for access in [
            CombatSearchV2InformationAccess::PublicObservation,
            CombatSearchV2InformationAccess::PrivilegedSimulator,
            CombatSearchV2InformationAccess::DebugRaw,
        ] {
            assert_eq!(
                serde_json::to_value(access).expect("access should serialize"),
                serde_json::json!(access.label())
            );
            assert_eq!(
                CombatSearchV2InformationAccess::from_label(access.label()),
                Some(access)
            );
        }

        for risk in [
            CombatSearchV2HiddenInformationRisk::PrivilegedSimulatorState,
            CombatSearchV2HiddenInformationRisk::ExactRngState,
            CombatSearchV2HiddenInformationRisk::ExactDrawPileOrderWithoutFrozenEye,
            CombatSearchV2HiddenInformationRisk::ExactMonsterIntentUnderRunicDome,
        ] {
            assert_eq!(
                serde_json::to_value(risk).expect("risk should serialize"),
                serde_json::json!(risk.label())
            );
            assert_eq!(
                CombatSearchV2HiddenInformationRisk::from_label(risk.label()),
                Some(risk)
            );
        }
    }

    #[test]
    fn policy_evidence_summary_uses_stable_machine_labels() {
        let evidence = CombatSearchV2PolicyEvidenceReport {
            information_access: CombatSearchV2InformationAccess::PrivilegedSimulator,
            public_safe: false,
            hidden_information_risks: vec![
                CombatSearchV2HiddenInformationRisk::PrivilegedSimulatorState,
                CombatSearchV2HiddenInformationRisk::ExactRngState,
            ],
        };

        assert_eq!(evidence.information_access_label(), "privileged_simulator");
        assert_eq!(evidence.public_safety_label(), "not_public_safe");
        assert_eq!(
            evidence.hidden_information_risk_labels_csv(),
            "privileged_simulator_state,exact_rng_state"
        );
        assert_eq!(
            evidence.machine_summary(),
            "information_access=privileged_simulator public_safe=false hidden_risks=privileged_simulator_state,exact_rng_state"
        );
    }

    #[test]
    fn policy_evidence_declares_privileged_non_public_search() {
        let combat = blank_test_combat();

        let evidence = combat_search_policy_evidence_for_combat(&combat);

        assert_eq!(
            evidence.information_access,
            CombatSearchV2InformationAccess::PrivilegedSimulator
        );
        assert!(!evidence.public_safe);
        assert!(evidence
            .hidden_information_risks
            .contains(&CombatSearchV2HiddenInformationRisk::PrivilegedSimulatorState));
        assert!(evidence
            .hidden_information_risks
            .contains(&CombatSearchV2HiddenInformationRisk::ExactRngState));
    }

    #[test]
    fn policy_evidence_marks_hidden_draw_order_without_frozen_eye() {
        let mut combat = blank_test_combat();
        combat.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Defend, 12),
        ])
        .into();

        let without_frozen_eye = combat_search_policy_evidence_for_combat(&combat);
        assert!(without_frozen_eye
            .hidden_information_risks
            .contains(&CombatSearchV2HiddenInformationRisk::ExactDrawPileOrderWithoutFrozenEye));

        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::FrozenEye));
        let with_frozen_eye = combat_search_policy_evidence_for_combat(&combat);
        assert!(!with_frozen_eye
            .hidden_information_risks
            .contains(&CombatSearchV2HiddenInformationRisk::ExactDrawPileOrderWithoutFrozenEye));
    }

    #[test]
    fn policy_evidence_marks_hidden_runic_dome_intent() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicDome));

        let evidence = combat_search_policy_evidence_for_combat(&combat);

        assert!(evidence
            .hidden_information_risks
            .contains(&CombatSearchV2HiddenInformationRisk::ExactMonsterIntentUnderRunicDome));
    }
}
