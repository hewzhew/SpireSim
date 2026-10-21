from __future__ import annotations

from sts_learning.run_resource_trace import (
    RunCombatResourceTransition,
    RunPublicContext,
    RunResourceTrace,
    RunResourceTraceAccumulator,
)


def test_completed_combat_keeps_hp_and_potion_identities_separate() -> None:
    accumulator = RunResourceTraceAccumulator()
    start = _context(
        combat=True,
        hp=70,
        gold=50,
        potions=("FearPotion", "GamblersBrew"),
    )
    end = _context(
        combat=False,
        hp=61,
        gold=60,
        potions=("GamblersBrew", "StrengthPotion"),
    )

    accumulator.record(
        {0: start},
        {0: end},
        {
            "slot_indices": [0],
            "terminal_slot_indices": [],
            "terminal_reward": [],
        },
    )
    trace = accumulator.finish()

    assert len(trace.combat_transitions) == 1
    assert trace.combat_transitions[0].hp_loss == 9
    assert trace.hp_loss_sum == 9
    assert trace.potion_identity_losses == (("FearPotion", 1),)
    assert trace.potion_identity_gains == (("StrengthPotion", 1),)
    assert trace.open_combat_count == 0
    assert len(trace.seed_summaries) == 1
    assert trace.seed_summaries[0].combat_count == 1
    assert trace.seed_summaries[0].hp_loss_sum == 9
    assert trace.seed_summaries[0].open_combat is False
    assert trace.seed_summaries[0].last_potion_ids == (
        "GamblersBrew",
        "StrengthPotion",
    )


def test_unfinished_combat_stays_explicitly_censored() -> None:
    accumulator = RunResourceTraceAccumulator()

    accumulator.record(
        {0: _context(combat=False)},
        {0: _context(combat=True)},
        {
            "slot_indices": [0],
            "terminal_slot_indices": [],
            "terminal_reward": [],
        },
    )

    trace = accumulator.finish()
    assert trace.combat_transitions == ()
    assert trace.open_combat_count == 1
    assert len(trace.seed_summaries) == 1
    assert trace.seed_summaries[0].combat_count == 0
    assert trace.seed_summaries[0].last_floor == 4
    assert trace.seed_summaries[0].open_combat is True


def test_completed_combat_history_excludes_current_future_and_other_seed() -> None:
    before = _transition(seed=123, floor=2)
    current = _transition(seed=123, floor=3)
    future = _transition(seed=123, floor=5)
    other_episode = _transition(seed=124, floor=1)
    trace = RunResourceTrace(
        combat_transitions=(future, other_episode, before, current),
        episode_endpoints=(),
    )

    assert trace.completed_combats_before(seed=123, act=1, floor=3) == (before,)


def _transition(*, seed: int, floor: int) -> RunCombatResourceTransition:
    start = _context(combat=True, seed=seed, floor=floor)
    end = _context(combat=False, seed=seed, floor=floor)
    return RunCombatResourceTransition(start=start, end=end, terminal_reward=None)


def _context(
    *,
    combat: bool,
    hp: int = 70,
    gold: int = 50,
    potions: tuple[str | None, ...] = (),
    seed: int = 123,
    floor: int = 4,
) -> RunPublicContext:
    return RunPublicContext(
        slot_index=0,
        boundary_kind=1 if combat else 0,
        is_combat=combat,
        is_terminal=False,
        seed=seed,
        act=1,
        floor=floor,
        hp=hp,
        max_hp=80,
        gold=gold,
        potion_ids=potions,
        encounter_id="JawWorm" if combat else None,
        monster_ids=("JawWorm",) if combat else (),
    )
