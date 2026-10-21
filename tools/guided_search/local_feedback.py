"""Read-only first-divergence supervision from exact direct/search evidence."""
import gzip
import json

import numpy as np
import torch

from tools.guided_search.local_demonstrations import load_demonstrations, validate_rescue_sources
from tools.guided_search.local_train import hash_file


def first_divergences(direct_directory, search_directory, config):
    """One positive alternative at each shared-prefix divergence, never a negative.

    Both complete outcomes and exact original roots are required. Each retained
    numerical row comes from the replayed win; its observation and action domain
    are checked against the real failed trajectory through the first divergence.
    Later search states are not misrepresented as states the failed policy visited.
    """
    receipt = validate_rescue_sources(direct_directory, search_directory)
    data = load_demonstrations(search_directory, config)
    receipt.update(kind="first_divergence_demonstration", rows=[],
                   search_demonstrations_sha256=hash_file(search_directory / "demonstrations.json"),
                   direct_experience_sha256=hash_file(direct_directory / "experience.npz"))
    if data is None:
        return None, receipt
    with gzip.open(direct_directory / "evidence.json.gz", "rt", encoding="utf-8") as f:
        direct_episodes = json.load(f)["episodes"]
    with np.load(direct_directory / "experience.npz", allow_pickle=False) as values:
        direct = {k: torch.as_tensor(values[k], device=config.device) for k in values.files}
    features = set(data) - {"slots", "choices"}
    if set(direct) - {"slots", "choices", "logp", "old_value"} != features:
        raise ValueError("direct and search numerical fields differ")
    keep = []
    for source in receipt["mapping"]:
        if source["kind"] != "win":
            continue
        original, slot = source["direct_slot"], source["search_slot"]
        left = torch.where(direct["slots"] == original)[0]
        right = torch.where(data["slots"] == slot)[0]
        failed = direct["choices"][left].tolist()
        won = data["choices"][right].tolist()
        if failed != direct_episodes[original]["decision_ordinals"]:
            raise ValueError("direct numerical decisions differ from the recorded trajectory")
        step = next((i for i, (a, b) in enumerate(zip(failed, won)) if a != b), None)
        if step is None:
            raise ValueError("opposite terminal outcomes have no differing decision")
        for key in features:
            if not torch.equal(direct[key][left[:step+1]], data[key][right[:step+1]]):
                raise ValueError("first-divergence numerical sources or action domains differ")
        keep.append(int(right[step]))
        receipt["rows"].append({"search_slot": slot, "direct_slot": original, "decision_step": step,
                               "source_row": int(right[step]), "demonstrated_choice": won[step],
                               "failed_trajectory_choice": failed[step]})
    return {k: v[keep].clone() for k, v in data.items()}, receipt
