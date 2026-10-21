"""Committed reward-collection memory shared by strategic execution callers."""
from __future__ import annotations

import json


class StrategicRewardMemory:
    def __init__(self, environment):
        self.environment = environment
        self._rooms = {}
        self._skipped_rewards = {}
        self._reward_card_items = {}
        self._proceeded_rewards = set()

    def observe_rooms(self, contexts):
        for slot, context in contexts.items():
            room = (context.seed, context.act, context.floor)
            if self._rooms.get(slot) != room:
                self.clear_slots([slot])
                self._rooms[slot] = room

    def clear_slots(self, slots):
        for slot in slots:
            self._rooms.pop(slot, None)
            self._skipped_rewards.pop(slot, None)
            self._reward_card_items.pop(slot, None)
            self._proceeded_rewards.discard(slot)

    def state(self, slots=None):
        selected = None if slots is None else set(slots)
        keep = lambda slot: selected is None or slot in selected
        return {
            "rooms": {str(slot): room for slot, room in self._rooms.items() if keep(slot)},
            "skipped_rewards": {str(slot): sorted(items) for slot, items in self._skipped_rewards.items() if keep(slot)},
            "reward_card_items": {str(slot): items for slot, items in self._reward_card_items.items() if keep(slot)},
            "proceeded_rewards": sorted(slot for slot in self._proceeded_rewards if keep(slot)),
        }

    def restore_state(self, state, slots=None):
        if set(state) != {"rooms", "skipped_rewards", "reward_card_items", "proceeded_rewards"}:
            raise ValueError("invalid strategic reward memory")
        rooms = {int(slot): tuple(room) for slot, room in state["rooms"].items()}
        skipped = {int(slot): set(items) for slot, items in state["skipped_rewards"].items()}
        cards = {int(slot): list(items) for slot, items in state["reward_card_items"].items()}
        proceeded = set(state["proceeded_rewards"])
        keys = set(rooms) | set(skipped) | set(cards) | proceeded
        allowed = set(range(self.environment.slot_count)) if slots is None else set(slots)
        if not keys <= allowed or any(len(room) != 3 for room in rooms.values()):
            raise ValueError("strategic reward memory slots or rooms are invalid")
        if any(type(index) is not int or index < 0 for values in (*skipped.values(), *cards.values()) for index in values):
            raise ValueError("strategic reward memory indices are invalid")
        self.clear_slots(allowed)
        self._rooms.update(rooms)
        self._skipped_rewards.update(skipped)
        self._reward_card_items.update(cards)
        self._proceeded_rewards.update(proceeded)

    def commit_rewards(self, decisions, after):
        for decision in decisions:
            audit = decision["audit"]
            if audit is None:
                continue
            slot = decision["slot"]
            candidate = audit["candidates"][decision["ordinal"]]
            # The navigation hint lasts through the next committed root
            # choice. A failed macro or a symbolic choice cannot consume it.
            self._proceeded_rewards.discard(slot)
            if candidate["kind"] == "skip_card_reward":
                index = candidate["reward_item_index"]
                if index is not None:
                    self._skipped_rewards.setdefault(slot, set()).add(index)
            if candidate["kind"] == "proceed" and candidate["site"] == "reward":
                # A room can produce another collection without advancing
                # its floor. Abandoned skips belong only to the old one.
                self._skipped_rewards.pop(slot, None)
                self._reward_card_items.pop(slot, None)
                self._proceeded_rewards.add(slot)
        self._refresh_reward_indices(decisions, after)

    def _baseline_ordinal(self, slot, audit, ordinal):
        # Skip is a decision about this collection's reward item. Opening the same
        # unclaimed reward again would undo it; other rewards remain claimable.
        if audit is None or audit["decision_site"] != "reward":
            return ordinal
        candidates = audit["candidates"]
        skipped = self._skipped_rewards.get(slot, set())
        selected = candidates[ordinal]
        if (selected["kind"] != "open_card_reward"
                or selected["reward_item_index"] not in skipped):
            return ordinal
        for kind in ("claim_reward", "open_card_reward", "proceed", "cancel"):
            for index, candidate in enumerate(candidates):
                if candidate["kind"] == kind and not (
                    kind == "open_card_reward" and candidate["reward_item_index"] in skipped
                ):
                    return index
        raise RuntimeError("skipped reward has no represented continuation")

    @staticmethod
    def _card_reward_indices(audit):
        indices = audit.get("reward_card_item_indices")
        if indices is not None:
            if (not isinstance(indices, list) or indices != sorted(set(indices))
                    or any(type(index) is not int or index < 0 for index in indices)):
                raise ValueError("invalid public card reward collection indices")
            return list(indices)
        return sorted(candidate["reward_item_index"] for candidate in audit["candidates"]
                      if candidate["kind"] == "open_card_reward")

    def _refresh_reward_indices(self, decisions, after=None):
        # Reward indices are positions, not identities. Claiming gold, a potion,
        # or a linked relic/key can remove preceding items. Read the resulting
        # public card-reward order instead of copying those game effects here.
        for decision in decisions:
            audit = decision["audit"]
            if audit is None:
                continue
            slot = decision["slot"]
            if after is not None:
                context = after[slot]
                room = (context.seed, context.act, context.floor)
                if self._rooms.get(slot) != room:
                    self._rooms[slot] = room
                    self._skipped_rewards.pop(slot, None)
                    self._reward_card_items.pop(slot, None)
                    self._proceeded_rewards.discard(slot)
                    continue
            previous = list(self._reward_card_items.get(slot, ()))
            chosen = audit["candidates"][decision["ordinal"]]
            if chosen["kind"] in ("take_card", "singing_bowl"):
                consumed = chosen["reward_item_index"]
                previous = [index for index in previous if index != consumed]
                self._skipped_rewards.get(slot, set()).discard(consumed)
            raw = self.environment.strategic_decision_audit_json(slot)
            following = None if raw is None else json.loads(raw)
            if following is None or following["decision_site"] != "reward":
                continue
            current = self._card_reward_indices(following)
            skipped = self._skipped_rewards.get(slot, set())
            if skipped and len(previous) != len(current):
                raise RuntimeError("public reward continuation changed card order unexpectedly")
            remap = dict(zip(previous, current, strict=bool(skipped)))
            self._skipped_rewards[slot] = {remap[index] for index in skipped}
            self._reward_card_items[slot] = current
