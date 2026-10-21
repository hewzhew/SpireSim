"""Resource-target and pre-outcome allocation contracts, not combat quality."""
import copy

import numpy as np
import torch

from entity_resource_pilot import (
    gold_available, recipes, resource_loss, selections, targets,
)


def outcome(hp=40, *, mugged=False):
    return dict(final_hp=hp, final_gold=59 if mugged else 79,
                pending_rewards=[] if mugged else [{"StolenGold": {"amount":20}}],
                combat_mugged=mugged,
                persistent=dict(recoverable_gold_delta=-40 if mugged else 0))


def test_budget_noncompletion_only_teaches_search_completion():
    rows = [dict(outcome=None), dict(outcome=outcome())]
    truth = targets(rows)
    prediction = torch.zeros((2, 4), requires_grad=True)
    resource_loss(prediction, truth).backward()
    assert prediction.grad[0, 0] > 0
    torch.testing.assert_close(prediction.grad[0, 1:], torch.zeros(3))
    assert torch.count_nonzero(prediction.grad[1, 1:]).item() == 3
    assert truth[1, 1].item() == np.float32(40 / 75)
    assert truth[1, 2].item() == np.float32(99 / 200)


def test_gold_labels_include_pending_recovery_without_double_counting_debt():
    assert gold_available(outcome()) == 99
    assert gold_available(outcome(mugged=True)) == 59
    # Normal postcombat gold is outside this collector's declared target.
    with_other_rewards = outcome()
    with_other_rewards["pending_rewards"].append({"Gold": {"amount":15}})
    assert gold_available(with_other_rewards) == 99


def test_fresh_source_partition_keeps_entire_seed_on_one_side():
    sources = recipes()
    train = {s["spec"]["seed"] for s in sources if s["split"] == "train"}
    evaluation = {s["spec"]["seed"] for s in sources if s["split"] == "evaluation"}
    assert len(train) == 12 and len(evaluation) == 6 and train.isdisjoint(evaluation)
    assert all(s["spec"]["max_potion_uses"] == 0 for s in sources)


def test_frozen_selection_uses_predictions_without_terminal_outcomes():
    rows = [dict(source=7, group=0, encounter="SlimeBoss", pool="next_turn",
                 kept=i < 2, side_index=i, position={"combat":{"entities":{
                     "player":{"current_hp":50}}}}) for i in range(4)]
    predictions = np.array([[.9,.2,.5,.9], [.9,.3,.5,.9],
                            [.9,.8,.5,.9], [.9,.9,.5,.9]])
    before = selections(rows, predictions)
    assert before[0]["model"] == [3, 2]
    assert before[0]["kept"] == [0, 1]
    changed = copy.deepcopy(rows)
    for i, row in enumerate(changed):
        row["outcome"] = outcome(hp=i * 10)
    assert selections(changed, predictions) == before
    # The Looter task prioritizes recoverable gold without a gold/HP conversion.
    for row in rows:
        row["encounter"] = "Looter"
    predictions[:, 2] = [.9, .8, .3, .2]
    assert selections(rows, predictions)[0]["model"] == [0, 1]


def test_sparse_kept_side_gives_every_method_the_same_continuation_count():
    rows = [dict(source=0, group=0, encounter="SlimeBoss", pool="next_action",
                 kept=i == 0, side_index=i, position={"combat":{"entities":{
                     "player":{"current_hp":50}}}}) for i in range(3)]
    chosen, = selections(rows, np.array([[.9,.2,.5,.9], [.9,.4,.5,.9], [.9,.8,.5,.9]]))
    assert chosen["model"] == [2]
    assert chosen["kept"] == [0]
    assert len(chosen["current_hp"]) == 1
