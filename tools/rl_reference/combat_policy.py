"""Entity actor/critic plugged into upstream SB3; PPO and GAE stay upstream."""
import torch
from stable_baselines3.common.policies import ActorCriticPolicy

from tools.guided_search.entity_learning import EntityNetwork


class CombatPolicy(ActorCriticPolicy):
    def __init__(self, *args, hidden=32, layers=1, **kwargs):
        self.hidden, self.layers = hidden, layers
        super().__init__(*args, **kwargs)

    def _build(self, lr_schedule):
        self.network = EntityNetwork(self.hidden, self.layers)
        self.optimizer = self.optimizer_class(self.parameters(), lr=lr_schedule(1), **self.optimizer_kwargs)

    def _get_constructor_parameters(self):
        return {**super()._get_constructor_parameters(), "hidden": self.hidden, "layers": self.layers}

    def _distribution_value(self, obs):
        # Rollout buffers can store integer spaces as float32; the small exact
        # vocabulary and source-local references are converted back here.
        e = int(obs["entity_mask"].sum(1).max())
        a = int(obs["mask"].sum(1).max())
        data = {}
        for k, v in obs.items():
            value = v[:, :e if k.startswith("entity_") else a]
            data[k] = (value.bool() if k in ("mask", "entity_mask") else
                       value.long() if k.endswith(("_ids", "_refs")) else value.float())
        logits, values = self.network(data)
        logits = torch.nn.functional.pad(logits, (0, self.action_space.n - a), value=-torch.inf)
        return self.action_dist.proba_distribution(action_logits=logits), values[:, None]

    def forward(self, obs, deterministic=False):
        distribution, values = self._distribution_value(obs)
        actions = distribution.get_actions(deterministic=deterministic)
        return actions, values, distribution.log_prob(actions)

    def evaluate_actions(self, obs, actions):
        distribution, values = self._distribution_value(obs)
        return values, distribution.log_prob(actions.long()), distribution.entropy()

    def get_distribution(self, obs):
        return self._distribution_value(obs)[0]

    def predict_values(self, obs):
        return self._distribution_value(obs)[1]
