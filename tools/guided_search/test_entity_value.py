import json
import copy

import pytest
torch = pytest.importorskip("torch")

from tools.guided_search.entity_value import encode_return, objective


def test_budget_miss_has_no_resource_or_potion_training_gradient():
    resources = torch.tensor([[10., -5., 3.], [.5, .2, .1]], requires_grad=True)
    reserve = torch.tensor([[4., -3.], [.1, -.1]], requires_grad=True)
    prediction = {"teacher_found":torch.zeros(2, requires_grad=True),
                  "resources":resources, "potion_retained":reserve}
    target = (torch.tensor([False, True]), torch.zeros((2, 3)),
              torch.zeros((2, 2)), torch.tensor([[False, False], [True, True]]))
    objective(prediction, target).backward()
    assert resources.grad[0].abs().sum() == 0
    assert reserve.grad[0].abs().sum() == 0
    assert resources.grad[1].abs().sum() > 0
    assert reserve.grad[1].abs().sum() > 0


def test_no_wins_still_has_finite_coverage_learning_only():
    prediction = {"teacher_found":torch.zeros(2, requires_grad=True),
                  "resources":torch.ones((2, 3), requires_grad=True),
                  "potion_retained":torch.ones((2, 2), requires_grad=True)}
    target = (torch.zeros(2, dtype=torch.bool), torch.zeros((2, 3)),
              torch.zeros((2, 2)), torch.zeros((2, 2), dtype=torch.bool))
    loss = objective(prediction, target)
    assert torch.isfinite(loss)
    loss.backward()
    assert prediction["teacher_found"].grad.abs().sum() > 0
    assert prediction["resources"].grad.abs().sum() == 0
    assert prediction["potion_retained"].grad.abs().sum() == 0


def test_return_rejects_different_root_before_encoding():
    class Bridge:
        def combat_entity_batch(self, *_args, **_kwargs):
            raise AssertionError("root must be checked before encoding")
    sample = {"position":{"engine":"original"}}
    report = {"root":{"engine":"changed"}}
    with pytest.raises(ValueError, match="teacher root differs"):
        encode_return(Bridge(), sample, report, root="test", group=0,
                      kept=True, potion_limit=None)


def test_same_plan_supplies_hp_growth_and_exact_potion_slot_targets():
    bridge = pytest.importorskip("sts_learning_bridge")
    spec = {"seed":73, "encounter":"JawWorm", "act":1, "room":"MonsterRoom",
            "hp":68, "deck":[("Strike", 0)] * 5, "relics":[],
            "potions":["DexterityPotion", "PowerPotion"], "max_potion_uses":None}
    env = bridge.GuidedCombatBatch.from_specifications([spec], 128)
    source = json.loads(env.evidence_json())["episodes"][0]["root"]
    successor = copy.deepcopy(source)
    player = successor["combat"]["entities"]["player"]
    player["current_hp"] -= 12
    player["max_hp"] += 3
    report = {"root":source, "config":{"max_potion_uses":None}, "plans":[
        {"successor":successor, "outcome":{"starting_potion_expenditure_slots":2}}]}
    row = encode_return(bridge, {"position":source, "potion_uses":1}, report,
        root="test", group=1, kept=False, potion_limit=None)
    assert row.resources == pytest.approx((-.16, .04, 0))
    assert row.potion_retained == (1., 0.)
    assert row.position.columns["entity_values"][0, 8] == 1
