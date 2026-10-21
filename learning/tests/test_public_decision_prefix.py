from sts_learning.decision_progress import PublicDecisionSnapshot
from sts_learning.public_decision_prefix import (
    public_combat_entry_prefix_id_v1,
    selected_public_prefix_step_v1,
)


def _snapshot(*, combat: bool, identity: str) -> PublicDecisionSnapshot:
    return PublicDecisionSnapshot(
        phase=1 if combat else 0,
        is_combat=combat,
        snapshot_id=f"snapshot-{identity}",
        observation_id=f"observation-{identity}",
        history_snapshot_id=f"history-{identity}",
        candidate_surface_id=f"surface-{identity}",
        candidate_ids=(f"candidate-{identity}-0", f"candidate-{identity}-1"),
    )


def test_public_combat_prefix_binds_prior_public_choice_without_seed_identity() -> None:
    prior = _snapshot(combat=False, identity="route")
    combat = _snapshot(combat=True, identity="entry")

    left = public_combat_entry_prefix_id_v1(
        (selected_public_prefix_step_v1(prior, 0),),
        combat,
    )
    repeated = public_combat_entry_prefix_id_v1(
        (selected_public_prefix_step_v1(prior, 0),),
        combat,
    )
    right = public_combat_entry_prefix_id_v1(
        (selected_public_prefix_step_v1(prior, 1),),
        combat,
    )

    assert left == repeated
    assert left != right
