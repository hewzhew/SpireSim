"""Execution-contract binding around the unchanged native state checkpoint.

The envelope is caller metadata, not a new simulator state format or a policy
loader. Restoration requires an explicitly supplied matching runtime binding.
"""
from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from dataclasses import replace
from pathlib import Path
from typing import Any, Callable

from .fixed_combat import FixedCombatConfig, FixedCombatController, FixedCombatStep, fixed_combat_options
from .strategic_navigation import StrategicRewardMemory
from .run_resource_trace import RunPublicContext

_MAGIC = b"STS-FIXED-COMBAT\x01"
_HEADER_LIMIT = 65_536


def canonical_json(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)


@dataclass(frozen=True)
class FixedCombatBinding:
    options_json: str
    native_sha256: str
    semantic_schema_sha256: str
    navigation_id: str | None = None

    def __post_init__(self) -> None:
        if canonical_json(json.loads(self.options_json)) != self.options_json:
            raise ValueError("fixed combat options must use canonical JSON")
        for value in (self.native_sha256, self.semantic_schema_sha256, *(() if self.navigation_id is None else (self.navigation_id,))):
            if len(value) != 64 or any(c not in "0123456789abcdef" for c in value):
                raise ValueError("fixed combat producer digests must be lowercase SHA-256")

    @classmethod
    def create(cls, native_type: Any, schema: object, native_sha256: str, config: FixedCombatConfig):
        return cls(canonical_json(fixed_combat_options(native_type, config)), native_sha256,
                   hashlib.sha256(canonical_json(schema).encode()).hexdigest())

    @property
    def identity(self) -> str:
        contract = {
            "schema": 1, "options": json.loads(self.options_json),
            "native_sha256": self.native_sha256,
            "semantic_schema_sha256": self.semantic_schema_sha256,
        }
        if self.navigation_id is not None:
            contract["navigation_id"] = self.navigation_id
        return hashlib.sha256(canonical_json(contract).encode()).hexdigest()


class FixedCombatUnresolved(RuntimeError):
    def __init__(self, result: FixedCombatStep) -> None:
        self.result = result
        super().__init__(f"fixed combat unresolved in slot {result.unresolved_slot}; batch unchanged")


class FixedCombatEnvironment:
    """Driver adapter with explicit provenance and no automatic recovery."""
    def __init__(self, native: Any, config: FixedCombatConfig, binding: FixedCombatBinding,
                 evidence_sink: Callable[[FixedCombatStep], None] | None = None):
        self._native = native
        self._controller = FixedCombatController(native, config)
        if canonical_json(self._controller.options) != binding.options_json:
            raise ValueError("executor options disagree with the supplied binding")
        self.binding = binding
        self._evidence_sink = evidence_sink
        self.last_combat_fallbacks: tuple[tuple[int, bool], ...] | None = None
        self._reward_memory = StrategicRewardMemory(self) if binding.navigation_id is not None else None
        self._decision_audits = {}
        self._pending_navigation = []

    def decision_batch(self, **options):
        if self._reward_memory is None:
            return self._native.decision_batch(**options)
        contexts = {slot: RunPublicContext.from_bridge_row((slot, view))
                    for slot, view in self.public_run_contexts()}
        self._reward_memory.observe_rooms(contexts)
        batch = self._native.decision_batch(**options)
        self._decision_audits = {}
        for row, slot in enumerate(batch["slot_indices"]):
            slot = int(slot)
            raw = self.strategic_decision_audit_json(slot)
            audit = None if raw is None else json.loads(raw)
            if audit is not None and len(audit["candidates"]) != int(batch["candidate_counts"][row]):
                raise ValueError("navigation audit disagrees with the live candidate surface")
            token = batch["decision_token_ids"][row]
            self._decision_audits[token] = (slot, audit)
            if audit is not None and audit["decision_site"] == "reward":
                self._reward_memory._reward_card_items[slot] = self._reward_memory._card_reward_indices(audit)
        return batch

    def routine_choice_ordinals(self, batch):
        """Return only forced choices and reward menus containing navigation alone."""
        if self._reward_memory is None:
            return {}
        choices = {}
        for row, (token, count) in enumerate(zip(batch["decision_token_ids"], batch["candidate_counts"], strict=True)):
            slot, audit = self._decision_audits[token]
            if int(count) == 1:
                choices[row] = 0
            elif audit is not None and audit["decision_site"] == "reward" and all(
                    candidate["kind"] in ("open_card_reward", "proceed", "cancel")
                    for candidate in audit["candidates"]):
                candidates = audit["candidates"]
                skipped = self._reward_memory._skipped_rewards.get(slot, set())
                ordinal = next((index for index, candidate in enumerate(candidates)
                                if candidate["kind"] == "open_card_reward"
                                and candidate["reward_item_index"] not in skipped), None)
                if ordinal is None:
                    ordinal = next((index for index, candidate in enumerate(candidates)
                                    if candidate["kind"] == "proceed"), None)
                if ordinal is not None:
                    choices[row] = ordinal
        return choices

    def choose(self, tokens, ordinals):
        pending = []
        if self._reward_memory is not None:
            for token, ordinal in zip(tokens, ordinals, strict=True):
                slot, audit = self._decision_audits[token]
                if audit is not None:
                    pending.append({"slot": slot, "ordinal": 0, "audit": {
                        "decision_site": audit["decision_site"],
                        "candidates": [audit["candidates"][int(ordinal)]]}})
        self._native.choose(tokens, ordinals)
        self._pending_navigation.extend(pending)

    def __getattr__(self, name: str):
        return getattr(self._native, name)

    def step(self):
        result = self.fixed_step()
        if not result.advanced:
            raise FixedCombatUnresolved(result)
        return result.transitions

    def fixed_step(self) -> FixedCombatStep:
        """Same executor, with interruption evidence returned without throwing."""
        try:
            result = self._controller.step()
        except Exception as error:
            capture = getattr(self._evidence_sink, "failure", None)
            if callable(capture):
                try:
                    capture(self, error)
                except Exception as diagnostic_error:
                    error.add_note(f"Failure evidence capture failed: {diagnostic_error}")
            raise
        self.last_combat_fallbacks = None
        if self._evidence_sink is not None:
            self._evidence_sink(result)
        if not result.advanced:
            return result
        if self._reward_memory is not None:
            after = {slot: RunPublicContext.from_bridge_row((slot, view))
                     for slot, view in self.public_run_contexts()}
            self._reward_memory.commit_rewards(self._pending_navigation, after)
            self._pending_navigation.clear()
            self._decision_audits.clear()
        self.last_combat_fallbacks = tuple(
            (combat["slot_index"], combat.get("exit") is not None
             and combat["exit"]["boundary_kind"] == "terminal"
             and combat["exit"]["hp"] == 0)
            for combat in result.evidence["combats"] if combat["fallback"] is not None
        )
        return result

    def checkpoint_bytes(self, *, max_bytes: int) -> bytes:
        if type(max_bytes) is not int or max_bytes <= len(_MAGIC) + 4 + 256:
            raise ValueError("fixed combat checkpoint byte limit is too small")
        # Header hashes are fixed-width, so the payload budget is exact.
        metadata = {"binding": self.binding.identity, "payload_sha256": "0" * 64}
        if self._reward_memory is not None:
            metadata["navigation"] = self._navigation_state()
        header = canonical_json(metadata).encode()
        if len(header) > _HEADER_LIMIT:
            raise ValueError("fixed combat checkpoint header exceeds limit")
        payload = self._native.checkpoint_bytes(max_bytes=max_bytes-len(_MAGIC)-4-len(header))
        metadata["payload_sha256"] = hashlib.sha256(payload).hexdigest()
        header = canonical_json(metadata).encode()
        return _MAGIC + len(header).to_bytes(4, "little") + header + payload

    @classmethod
    def from_checkpoint_bytes(cls, payload: bytes, *, native_type: Any, config: FixedCombatConfig,
                              binding: FixedCombatBinding, expected_slots: int, max_bytes: int,
                              evidence_sink=None):
        if len(payload) > max_bytes or not payload.startswith(_MAGIC):
            raise ValueError("invalid or oversized fixed combat checkpoint")
        start = len(_MAGIC) + 4
        size = int.from_bytes(payload[len(_MAGIC):start], "little")
        if size > _HEADER_LIMIT or size == 0 or start + size >= len(payload):
            raise ValueError("invalid fixed combat checkpoint header length")
        header = json.loads(payload[start:start+size])
        expected_keys = {"binding", "payload_sha256"} | ({"navigation"} if binding.navigation_id is not None else set())
        if not isinstance(header, dict) or set(header) != expected_keys:
            raise ValueError("invalid fixed combat checkpoint header")
        if header["binding"] != binding.identity:
            raise ValueError("fixed combat checkpoint execution binding mismatch")
        if canonical_json(fixed_combat_options(native_type, config)) != binding.options_json:
            raise ValueError("executor options disagree with the supplied binding")
        raw = payload[start+size:]
        if hashlib.sha256(raw).hexdigest() != header["payload_sha256"]:
            raise ValueError("fixed combat checkpoint payload digest mismatch")
        native = native_type.from_checkpoint_bytes(raw, expected_slots=expected_slots, max_bytes=max_bytes)
        environment = cls(native, config, binding, evidence_sink)
        if environment._reward_memory is not None:
            environment._restore_navigation(header["navigation"])
        return environment

    def _navigation_state(self, slots=None):
        return {"rewards": self._reward_memory.state(slots),
                "pending": [choice for choice in self._pending_navigation
                            if slots is None or choice["slot"] in slots]}

    def _restore_navigation(self, state, slots=None):
        if set(state) != {"rewards", "pending"}:
            raise ValueError("invalid navigation checkpoint")
        allowed = set(range(self.slot_count)) if slots is None else set(slots)
        if any(choice["slot"] not in allowed for choice in state["pending"]):
            raise ValueError("navigation pending slot outside checkpoint")
        self._reward_memory.restore_state(state["rewards"], slots)
        self._pending_navigation = [choice for choice in self._pending_navigation if choice["slot"] not in allowed]
        self._pending_navigation.extend(state["pending"])
        self._decision_audits.clear()

    def checkpoint_slots(self, slots):
        native = self._native.checkpoint_slots(slots)
        if self._reward_memory is None:
            return native
        return StrategicCheckpointBatch(native, tuple(slots), self._navigation_state(slots), self.binding.navigation_id)

    def restore_slots(self, slots, checkpoints):
        if self._reward_memory is None:
            return self._native.restore_slots(slots, checkpoints)
        if not isinstance(checkpoints, StrategicCheckpointBatch) or checkpoints.navigation_id != self.binding.navigation_id:
            raise ValueError("navigation checkpoint bank binding mismatch")
        if len(slots) != len(checkpoints.slots):
            raise ValueError("navigation checkpoint bank slot count mismatch")
        mapping = dict(zip(checkpoints.slots, slots, strict=True))
        state = checkpoints.remapped_state(mapping)
        self._native.restore_slots(slots, checkpoints.native)
        self._restore_navigation(state, slots)

    def reset_slots(self, slots, seeds):
        self._native.reset_slots(slots, seeds)
        if self._reward_memory is not None:
            self._reward_memory.clear_slots(slots)
            self._pending_navigation = [choice for choice in self._pending_navigation if choice["slot"] not in slots]
            self._decision_audits.clear()

    def reset_slots_checkpointed(self, slots, seeds):
        native = self._native.reset_slots_checkpointed(slots, seeds)
        if self._reward_memory is None:
            return native
        self._reward_memory.clear_slots(slots)
        self._pending_navigation = [choice for choice in self._pending_navigation if choice["slot"] not in slots]
        self._decision_audits.clear()
        return StrategicCheckpointBatch(native, tuple(slots), self._navigation_state(slots), self.binding.navigation_id)


class StrategicCheckpointBatch:
    """The existing opaque slot bank, bound to its reward-processing memory."""
    _magic = b"STS-STRATEGIC-BANK\x01"

    def __init__(self, native, slots, state, navigation_id):
        self.native, self.slots, self.state, self.navigation_id = native, slots, state, navigation_id
        if len(native) != len(slots) or len(set(slots)) != len(slots):
            raise ValueError("navigation checkpoint bank slots are misaligned")
        self.state = json.loads(canonical_json(state))

    def __len__(self):
        return len(self.native)

    def remapped_state(self, mapping):
        rewards = self.state["rewards"]
        return {"rewards": {
            **{key: {str(mapping[int(slot)]): value for slot, value in rewards[key].items() if int(slot) in mapping}
               for key in ("rooms", "skipped_rewards", "reward_card_items")},
            "proceeded_rewards": sorted(mapping[slot] for slot in rewards["proceeded_rewards"] if slot in mapping)},
            "pending": [{**choice, "slot": mapping[choice["slot"]]} for choice in self.state["pending"]
                        if choice["slot"] in mapping]}

    def select(self, slots):
        if not set(slots) <= set(self.slots):
            raise ValueError("navigation checkpoint selection outside bank")
        return type(self)(self.native.select(slots), tuple(slots),
                          self.remapped_state({slot: slot for slot in slots}), self.navigation_id)

    def updated(self, replacements):
        if not isinstance(replacements, type(self)) or self.navigation_id != replacements.navigation_id:
            raise ValueError("navigation checkpoint replacement binding mismatch")
        if not set(replacements.slots) <= set(self.slots):
            raise ValueError("navigation checkpoint replacement outside bank")
        keep = {slot: slot for slot in self.slots if slot not in replacements.slots}
        state = self.remapped_state(keep)
        for key in ("rooms", "skipped_rewards", "reward_card_items"):
            state["rewards"][key].update(replacements.state["rewards"][key])
        state["rewards"]["proceeded_rewards"] = sorted(
            state["rewards"]["proceeded_rewards"] + replacements.state["rewards"]["proceeded_rewards"])
        state["pending"].extend(replacements.state["pending"])
        return type(self)(self.native.updated(replacements.native), self.slots, state, self.navigation_id)

    def checkpoint_bytes(self, *, max_bytes):
        metadata = {"navigation_id": self.navigation_id, "slots": self.slots,
                    "state": self.state, "payload_sha256": "0"*64}
        header = canonical_json(metadata).encode()
        if len(header) > _HEADER_LIMIT or len(header) + len(self._magic) + 4 >= max_bytes:
            raise ValueError("navigation checkpoint bank exceeds byte limit")
        raw = self.native.checkpoint_bytes(max_bytes=max_bytes-len(header)-len(self._magic)-4)
        metadata["payload_sha256"] = hashlib.sha256(raw).hexdigest()
        header = canonical_json(metadata).encode()
        return self._magic + len(header).to_bytes(4, "little") + header + raw

    @classmethod
    def from_checkpoint_bytes(cls, payload, *, decoder, navigation_id, expected_slot_indices, max_bytes):
        if len(payload) > max_bytes or not payload.startswith(cls._magic):
            raise ValueError("invalid navigation checkpoint bank")
        start = len(cls._magic)+4
        size = int.from_bytes(payload[len(cls._magic):start], "little")
        if not 0 < size <= _HEADER_LIMIT or start+size >= len(payload):
            raise ValueError("invalid navigation checkpoint bank header")
        header = json.loads(payload[start:start+size])
        if set(header) != {"navigation_id", "slots", "state", "payload_sha256"} or header["navigation_id"] != navigation_id:
            raise ValueError("navigation checkpoint bank binding mismatch")
        raw = payload[start+size:]
        if hashlib.sha256(raw).hexdigest() != header["payload_sha256"]:
            raise ValueError("navigation checkpoint bank digest mismatch")
        if tuple(header["slots"]) != tuple(expected_slot_indices):
            raise ValueError("navigation checkpoint bank expected slots mismatch")
        native = decoder(raw, expected_slot_indices=expected_slot_indices, max_bytes=max_bytes)
        return cls(native, tuple(header["slots"]), header["state"], navigation_id)


class InstalledFixedCombatRuntime:
    """Resolve producer identity once, before creating any experiment state."""
    def __init__(self, config: FixedCombatConfig, evidence_sink=None, *, final_act=False, strategic_navigation=False):
        if type(final_act) is not bool or type(strategic_navigation) is not bool:
            raise ValueError("final_act and strategic_navigation must be boolean")
        import sts_learning_bridge as native
        import sts_learning_bridge._native as binary
        self.native = native
        self.config = config
        self.final_act = final_act
        self.evidence_sink = evidence_sink
        self.schema = native.semantic_schema()
        with Path(binary.__file__).open("rb") as source:
            self.native_sha256 = hashlib.file_digest(source, "sha256").hexdigest()
        self.binding = FixedCombatBinding.create(native.LearningBatchEnv, self.schema,
                                                self.native_sha256, config)
        if strategic_navigation:
            source = Path(__file__).read_bytes() + Path(__file__).with_name("strategic_navigation.py").read_bytes()
            self.binding = replace(self.binding, navigation_id=hashlib.sha256(source).hexdigest())
        self.baseline_manifest = native.public_legality_bootstrap_manifest()

    def _with_baseline(self, environment):
        environment.baseline_manifest = json.loads(canonical_json(self.baseline_manifest))
        return environment

    def environment(self, seeds, ascension):
        factory = (self.native.LearningBatchEnv if self.config.allow_potions else
                   self.native.LearningBatchEnv.without_combat_potions)
        native = factory(seeds, ascension, **({"final_act": True} if self.final_act else {}))
        environment = FixedCombatEnvironment(native, self.config, self.binding, self.evidence_sink)
        return self._with_baseline(environment)

    def environment_without_combat_potions(self, seeds, ascension):
        config = replace(self.config, allow_potions=False)
        binding = FixedCombatBinding.create(self.native.LearningBatchEnv, self.schema,
                                           self.native_sha256, config)
        binding = replace(binding, navigation_id=self.binding.navigation_id)
        return self._with_baseline(FixedCombatEnvironment(
            self.native.LearningBatchEnv.without_combat_potions(
                seeds, ascension, **({"final_act": True} if self.final_act else {})),
            config, binding, self.evidence_sink))

    def environment_from_checkpoint(self, payload, *, expected_slots, max_bytes):
        return self._with_baseline(FixedCombatEnvironment.from_checkpoint_bytes(
            payload, native_type=self.native.LearningBatchEnv, config=self.config,
            binding=self.binding, expected_slots=expected_slots, max_bytes=max_bytes,
            evidence_sink=self.evidence_sink))

    def session_bridge(self):
        from .torch_session_config import CategoricalSessionBridge
        return CategoricalSessionBridge(
            environment=self.environment,
            environment_without_combat_potions=self.environment_without_combat_potions,
            environment_from_checkpoint=self.environment_from_checkpoint,
            checkpoint_bank_from_checkpoint=self.checkpoint_bank_from_checkpoint,
            semantic_schema=self.schema)

    def checkpoint_bank_from_checkpoint(self, payload, *, expected_slot_indices, max_bytes):
        decoder = self.native.LearningCheckpointBatch.from_checkpoint_bytes
        if self.binding.navigation_id is None:
            return decoder(payload, expected_slot_indices=expected_slot_indices, max_bytes=max_bytes)
        return StrategicCheckpointBatch.from_checkpoint_bytes(payload, decoder=decoder,
            navigation_id=self.binding.navigation_id, expected_slot_indices=expected_slot_indices, max_bytes=max_bytes)
