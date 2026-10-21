#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) enum ActionOrderingRole {
    LethalCard,
    PreventVisibleLethal,
    SustainedMitigation,
    KeySetupCard,
    TacticalPotion,
    PreventHpLoss,
    CurrentTurnRetaliationProtection,
    CurrentTurnAttackSetup,
    DeferredSetup,
    ImmediateActionSupply,
    DamageProgress,
    ReactiveRiskPreventHpLoss,
    Block,
    UtilityPlay,
    EndTurn,
    PendingChoiceValueSelection,
    PendingChoiceRemovalSelection,
    PendingChoiceNeutralSelection,
    PendingChoiceCancel,
    DiscardPotion,
    Neutral,
}

impl ActionOrderingRole {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            ActionOrderingRole::LethalCard => "lethal_card",
            ActionOrderingRole::PreventVisibleLethal => "prevent_visible_lethal",
            ActionOrderingRole::SustainedMitigation => "sustained_mitigation",
            ActionOrderingRole::KeySetupCard => "key_setup_card",
            ActionOrderingRole::TacticalPotion => "tactical_potion",
            ActionOrderingRole::PreventHpLoss => "prevent_hp_loss",
            ActionOrderingRole::CurrentTurnRetaliationProtection => {
                "current_turn_retaliation_protection"
            }
            ActionOrderingRole::CurrentTurnAttackSetup => "current_turn_attack_setup",
            ActionOrderingRole::DeferredSetup => "deferred_setup",
            ActionOrderingRole::ImmediateActionSupply => "immediate_action_supply",
            ActionOrderingRole::DamageProgress => "damage_progress",
            ActionOrderingRole::ReactiveRiskPreventHpLoss => "reactive_risk_prevent_hp_loss",
            ActionOrderingRole::Block => "block",
            ActionOrderingRole::UtilityPlay => "utility_play",
            ActionOrderingRole::EndTurn => "end_turn",
            ActionOrderingRole::PendingChoiceValueSelection => "pending_choice_value_selection",
            ActionOrderingRole::PendingChoiceRemovalSelection => "pending_choice_removal_selection",
            ActionOrderingRole::PendingChoiceNeutralSelection => "pending_choice_neutral_selection",
            ActionOrderingRole::PendingChoiceCancel => "pending_choice_cancel",
            ActionOrderingRole::DiscardPotion => "discard_potion",
            ActionOrderingRole::Neutral => "neutral",
        }
    }
}
