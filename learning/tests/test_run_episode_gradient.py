"""Exact finite-MDP checks for the whole-run objective, without sampled games."""
from dataclasses import replace
from itertools import product

import pytest

torch = pytest.importorskip("torch")

from learning.tests.torch_outcome_fixtures import (
    behavior_manifest_fixture, completed_attempt_fixture, decision_batch_fixture,
    public_attempt_trajectory_fixture, with_run_progress_fixture,
)
from sts_learning import (
    BehaviorManifestRegistry, FloorProgressReturnConfig, RunPolicyUpdateConfig,
    SelectionProbability, SemanticBatchConcatLimits, TerminalAdvantageMode,
)
from sts_learning.torch_outcomes import on_policy_terminal_loss
from sts_learning.torch_policy import (
    RaggedActorCriticOutput, RaggedCandidateLogits, RaggedCategoricalPolicyConfig,
)


class _BranchPolicy:
    """Two root actions; subsequent three-way choices have no effect on reward."""

    def __init__(self, baseline):
        self.theta = torch.nn.Parameter(torch.zeros((), dtype=torch.float64))
        self.baseline = baseline

    def __call__(self, payload):
        rows = [torch.stack((self.theta * 0, self.theta)) if count == 2
                else self.theta * torch.zeros(3, dtype=torch.float64)
                for count in payload["candidate_counts"]]
        return RaggedCandidateLogits(torch.cat(rows),
            torch.as_tensor(payload["candidate_row_splits"], dtype=torch.long))

    def actor_critic(self, payload):
        logits = self(payload)
        return RaggedActorCriticOutput(logits,
            self.theta * torch.zeros(logits.row_count, dtype=torch.float64) + self.baseline)


def _enumerated_attempts(manifest_id, extra_decisions):
    # At theta=0, each root branch has probability 1/2. Enumerate every
    # three-way suffix and replicate the short branch to give equal sample
    # weights. Thus this batch is the exact trajectory distribution, not a
    # Monte-Carlo approximation. Returns are 0.2 (short) and 0.6 (long).
    attempts = []
    for suffix in product(range(3), repeat=extra_decisions):
        for root in (0, 1):
            slot = len(attempts)
            choices = [(0, root, 0.5)]
            if root:
                choices.extend((1, ordinal, 1 / 3) for ordinal in suffix)
            batches = tuple(with_run_progress_fixture(decision_batch_fixture(
                slot=slot, semantic_row=row, selected_ordinal=ordinal,
                manifest_id=manifest_id, selection_probability=SelectionProbability.known(probability)),
                act=1, floor=0, is_combat=False, strategic_context_kind=2,
                identity_suffix=f"decision-{index}")
                for index, (row, ordinal, probability) in enumerate(choices))
            attempt = completed_attempt_fixture(slot=slot, batches=batches, reward=-1)
            attempt = replace(attempt, terminal=replace(attempt.terminal,
                terminal=replace(attempt.terminal.terminal, terminal_floor=60 + root * 20)))
            attempts.append(public_attempt_trajectory_fixture(attempt))
    return tuple(attempts)


@pytest.mark.parametrize("extra_decisions", [0, 3])
@pytest.mark.parametrize("algorithm,baseline,normalize", [
    ("reinforce", 0.0, False), ("ppo", 0.0, False), ("ppo", 0.4, False),
    ("ppo", 0.0, True), ("ppo", 0.4, True),
])
def test_policy_gradient_matches_exact_expected_return_with_unequal_episode_lengths(
    extra_decisions, algorithm, baseline, normalize,
):
    policy = _BranchPolicy(baseline)
    config = RaggedCategoricalPolicyConfig()
    registry = BehaviorManifestRegistry(capacity=1)
    identity = registry.register(behavior_manifest_fixture(behavior_rule=config.behavior_rule))
    attempts = _enumerated_attempts(identity, extra_decisions)
    update = (RunPolicyUpdateConfig() if algorithm == "reinforce" else
        replace(RunPolicyUpdateConfig.ppo_clip_value(), normalize_advantage=normalize,
                entropy_coefficient=0.0))
    mode = (TerminalAdvantageMode.RAW_RETURN if algorithm == "reinforce"
            else TerminalAdvantageMode.DECISION_LOCAL_GAE)
    result = on_policy_terminal_loss(policy, attempts, registry,
        SemanticBatchConcatLimits(max_rows=256, max_input_array_bytes=8 * 1024 * 1024),
        config, FloorProgressReturnConfig(target_floor=100), mode, update_config=update)

    # d E[R] / d theta = (0.6 - 0.2) p(1-p) = 0.1 at p=1/2.
    # A state-only baseline and irrelevant suffix decisions cannot change it.
    expected_return = 0.2 + 0.4 * policy.theta.sigmoid()
    expected_gradient, = torch.autograd.grad(-expected_return, policy.theta)
    if normalize:
        # Current PPO uses one batch-wide affine normalization. Its exact
        # standard deviation here is known from the enumerated state visits;
        # this is a shared scale, not a separate 1/T factor for each episode.
        long_fraction = (extra_decisions + 1) / (extra_decisions + 2)
        scale = 0.4 * (long_fraction * (1 - long_fraction)) ** 0.5
        expected_gradient = expected_gradient / scale
    actual_gradient, = torch.autograd.grad(result.value, policy.theta)
    torch.testing.assert_close(actual_gradient, expected_gradient, atol=1e-12, rtol=1e-12)
