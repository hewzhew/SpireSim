"""Actual outcomes and before-outcome decisions, without optimality labels."""
import copy

import numpy as np
import pytest
import torch

from entity_continuation_trial import choose, prepare, recipes, targets


def row(kind, hp):
    return dict(trace=dict(terminal_kind=kind, endpoint=dict(combat=dict(entities=dict(
        player=dict(current_hp=hp, max_hp=75))))))


def test_actual_death_supervises_both_heads_and_cutoff_is_not_death():
    truth = targets([row('win', 30), row('loss', -4)])
    torch.testing.assert_close(truth, torch.tensor([[1., .4], [0., 0.]]))
    for kind in ('cutoff', 'escape'):
        with pytest.raises(ValueError, match='completed combat'):
            targets([row(kind, 30)])


def test_selection_cannot_read_continuation_results():
    rows = [dict(group='same-source', family='strength_duration', position=dict(combat=dict(
        entities=dict(player=dict(current_hp=40))))) for _ in range(2)]
    prediction = np.array([[.9, .2], [.9, .7]])
    chosen = choose(rows, prediction)
    assert chosen[0]['model'] == 1 and chosen[0]['first'] == 0
    changed = copy.deepcopy(rows)
    changed[0].update(row('win', 75))
    changed[1].update(row('loss', 0))
    assert choose(changed, prediction) == chosen


def test_preparation_uses_slot_zero_and_preserves_real_attached_effects():
    spec = dict(seed=4300100, encounter='JawWorm', act=1, room='MonsterRoom', hp=60,
                deck=[('BattleTrance', 0), ('ShrugItOff', 1)],
                relics=[], potions=[], max_potion_uses=0)
    inputs, position = prepare(spec, ['BattleTrance', 'ShrugItOff'])
    assert len(inputs) == 2
    assert position['combat']['entities']['player']['block'] == 11
    assert any(p['power_type'] == 'NoDraw' for powers in position['combat']['entities']['power_db'].values()
               for p in powers)


def test_fresh_sources_include_actual_energy_tradeoffs_and_disjoint_seeds():
    rows = recipes()
    train = {r['spec']['seed'] for r in rows if r['split'] == 'train'}
    evaluation = {r['spec']['seed'] for r in rows if r['split'] == 'evaluation'}
    assert train.isdisjoint(evaluation)
    assert len(rows) == 60
    for row in rows:
        if row['family'] == 'strength_duration':
            assert row['position']['combat']['turn']['energy'] == 0
    for group in {r['group'] for r in rows}:
        members = [r for r in rows if r['group'] == group]
        assert len({r['spec']['seed'] for r in members}) == 1
        assert len({r['split'] for r in members}) == 1
