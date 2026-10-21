use super::super::pending_choice_ordering::PendingChoiceOrderingRole;
use super::constants::{
    ROLE_PENDING_CANCEL, ROLE_PENDING_NEUTRAL_SELECTION, ROLE_PENDING_REMOVAL_SELECTION,
    ROLE_PENDING_VALUE_SELECTION,
};
use super::role::ActionOrderingRole;

pub(super) fn pending_choice_role_rank(
    role: PendingChoiceOrderingRole,
) -> (ActionOrderingRole, i32) {
    match role {
        PendingChoiceOrderingRole::ValueSelection => (
            ActionOrderingRole::PendingChoiceValueSelection,
            ROLE_PENDING_VALUE_SELECTION,
        ),
        PendingChoiceOrderingRole::RemovalSelection => (
            ActionOrderingRole::PendingChoiceRemovalSelection,
            ROLE_PENDING_REMOVAL_SELECTION,
        ),
        PendingChoiceOrderingRole::NeutralSelection => (
            ActionOrderingRole::PendingChoiceNeutralSelection,
            ROLE_PENDING_NEUTRAL_SELECTION,
        ),
        PendingChoiceOrderingRole::Cancel => {
            (ActionOrderingRole::PendingChoiceCancel, ROLE_PENDING_CANCEL)
        }
    }
}
