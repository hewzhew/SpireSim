from __future__ import annotations

from sts_learning import BehaviorManifestId


BEHAVIOR_MANIFEST_ID = BehaviorManifestId(bytes(range(32)))
UPDATED_BEHAVIOR_MANIFEST_ID = BehaviorManifestId(bytes(reversed(range(32))))
