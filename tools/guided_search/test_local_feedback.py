import copy
import json
import time

import numpy as np
import pytest

torch = pytest.importorskip("torch")
bridge = pytest.importorskip("sts_learning_bridge")
if not hasattr(bridge.GuidedCombatBatch, "demonstration"):
    pytest.skip("rebuild bridge for numerical demonstrations", allow_module_level=True)

from tools.guided_search.local_demonstrations import freeze_policy, imitate_groups
from tools.guided_search.local_feedback import first_divergences
from tools.guided_search.local_train import Config, prepare, rollout
from tools.guided_search.test_local_demonstrations import successful_selection, save_evidence


def test_first_divergence_keeps_a_virtual_selection_and_checks_shared_numeric_source(tmp_path):
    winner = successful_selection()
    frame = winner.demonstration(0)
    data = prepare(frame)
    choices = frame["choices"].astype(np.int64)
    step = next(i for i, a in enumerate(choices) if data["actions"][i, a, 3] == 1)
    class DeviateThenPass(torch.nn.Module):
        def __init__(self):
            super().__init__()
            self.step = 0
        def forward(self, data):
            logits = torch.full(data["mask"].shape, -torch.inf)
            if self.step < step:
                action = int(choices[self.step])
            elif self.step == step:
                action = next(i for i in range(data["mask"].shape[1])
                              if data["mask"][0, i] and data["actions"][0, i, 3] == 1 and i != choices[step])
            else:
                action = next(i for i in range(data["mask"].shape[1])
                              if data["mask"][0, i] and (data["actions"][0, i, 1] == 1 or data["actions"][0, i, 4] == 1))
            logits[0, action] = 0
            self.step += 1
            return logits, torch.zeros(1)
    config = Config(task="draw_select", starting_hp=75)
    deck = [("BurningPact", 0), ("Strike", 1), ("Strike", 1), ("Headbutt", 1), ("PommelStrike", 1)]
    _, outcomes, _ = rollout(DeviateThenPass().eval(), [(17, "ThreeLouse")], config, torch.Generator(),
                            tmp_path / "direct", time.monotonic() + 30, "greedy", deck=deck)
    assert outcomes[0]["kind"] == "loss"
    save_evidence(winner, tmp_path / "search")
    selected, receipt = first_divergences(tmp_path / "direct", tmp_path / "search", config)
    assert selected["choices"].tolist() == [int(choices[step])]
    assert selected["actions"][0, selected["choices"][0], 3] == 1
    assert receipt["rows"][0]["decision_step"] == step
    for key in data:
        torch.testing.assert_close(selected[key][0], data[key][step])
    # Changes AFTER divergence are not claimed as observations of the failed policy.
    path = tmp_path / "direct/experience.npz"
    with np.load(path) as file:
        stored = {k: file[k].copy() for k in file.files}
    stored["context"][step + 1, 0] += 1
    np.savez_compressed(path, **stored)
    first_divergences(tmp_path / "direct", tmp_path / "search", config)
    stored["context"][step, 0] += 1
    np.savez_compressed(path, **stored)
    with pytest.raises(ValueError, match="numerical sources or action domains differ"):
        first_divergences(tmp_path / "direct", tmp_path / "search", config)
    # An unresolved search cannot manufacture a correction.
    unknown = bridge.GuidedCombatBatch([(17, "ThreeLouse")], 128, starting_hp=75,
                                      task="draw_select", deck=deck, search_services=1)
    f = unknown.decision_batch()
    unknown.guide(f["revision"], [np.ones(len(f["actions"])).tolist()], [None])
    assert json.loads(unknown.outcomes_json())[0]["kind"].startswith("unknown")
    save_evidence(unknown, tmp_path / "unknown")
    selected, receipt = first_divergences(tmp_path / "direct", tmp_path / "unknown", config)
    assert selected is None and receipt["rows"] == []


def test_frozen_anchor_has_no_gradient_and_resists_change_without_padding_mass():
    class Policy(torch.nn.Module):
        def __init__(self):
            super().__init__()
            self.logits = torch.nn.Parameter(torch.zeros(3))
        def forward(self, data):
            return self.logits.expand(len(data["slots"]), -1).masked_fill(~data["mask"], -torch.inf), torch.zeros(len(data["slots"]))
    data = {"slots": torch.zeros(2, dtype=torch.int64), "choices": torch.ones(2, dtype=torch.int64),
            "mask": torch.tensor([[True, True, False], [True, True, False]]), "context": torch.zeros((2, 1))}
    ordinary, anchored = Policy().eval(), Policy().eval()
    reference = freeze_policy(anchored, data)
    assert not reference.log_probabilities.requires_grad
    assert torch.isneginf(reference.log_probabilities[:, 2]).all()
    optimizers = [torch.optim.SGD(m.parameters(), lr=.2) for m in (ordinary, anchored)]
    for iteration in range(2):
        imitate_groups(ordinary, optimizers[0], [(data, 1)], Config())
        metric = imitate_groups(anchored, optimizers[1], [(data, 1)], Config(), anchors=[(0, reference, 2.0)])
        if iteration == 0:
            assert metric["anchors"][0]["kl"] == pytest.approx(0)
    assert metric["anchors"][0]["kl"] > 0
    assert 0 < anchored.logits[1] < ordinary.logits[1]
    assert anchored.logits[2] == 0
    assert reference.log_probabilities.grad is None
    before = anchored.logits.detach().clone()
    changed = copy.deepcopy(data)
    changed["context"][0, 0] = 1
    with pytest.raises(ValueError, match="different numerical source"):
        imitate_groups(anchored, optimizers[1], [(changed, 1)], Config(), anchors=[(0, reference, 1)])
    invalid = copy.deepcopy(reference)
    invalid.log_probabilities[0, 2] = -20
    with pytest.raises(ValueError, match="padding"):
        imitate_groups(anchored, optimizers[1], [(data, 1)], Config(), anchors=[(0, invalid, 1)])
    for group, strength in [(1, 1), (0, 0), (0, -1), (0, float("nan"))]:
        with pytest.raises(ValueError, match="invalid anchor"):
            imitate_groups(anchored, optimizers[1], [(data, 1)], Config(), anchors=[(group, reference, strength)])
    torch.testing.assert_close(anchored.logits, before)


def test_anchor_averages_episodes_and_is_independent_of_supervision_group_mass():
    class Policy(torch.nn.Module):
        def __init__(self):
            super().__init__()
            self.logits = torch.nn.Parameter(torch.tensor([0., 0., 0.]))
            self.calls = 0
        def forward(self, data):
            self.calls += 1
            return self.logits.expand(len(data["slots"]), -1).masked_fill(~data["mask"], -torch.inf), torch.zeros(len(data["slots"]))
    # One short episode and one longer episode have different legal domains.
    data = {"slots": torch.tensor([0, 1, 1, 1]), "choices": torch.tensor([0, 1, 1, 1]),
            "mask": torch.tensor([[True, True, False], [True, True, True],
                                  [True, True, True], [True, True, True]])}
    model = Policy().eval()
    reference = freeze_policy(model, data)
    with torch.no_grad():
        model.logits.copy_(torch.tensor([1., -1., .5]))
    legal_logp = model.logits.detach().log_softmax(-1)
    short_logp = model.logits.detach()[:2].log_softmax(-1)
    expected_kl = ((-np.log(2) - short_logp).mean() + (-np.log(3) - legal_logp).mean()) / 2
    expected_mle = (-short_logp[0] - legal_logp[1]) / 2
    assert expected_kl > 0
    model.calls = 0
    # A second copy changes MLE group mass without scaling the first group's KL.
    metrics = imitate_groups(model, torch.optim.SGD(model.parameters(), lr=0),
                             [(data, 1), (data, 9)], Config(), anchors=[(0, reference, 3)])
    assert model.calls == 2
    assert metrics["anchors"][0]["kl"] == pytest.approx(float(expected_kl), abs=1e-6)
    assert metrics["policy_loss"] == pytest.approx(float(expected_mle + 3 * expected_kl), abs=1e-6)
