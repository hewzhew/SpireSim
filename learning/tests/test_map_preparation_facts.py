"""Native compact preparation facts keep connected opportunity classes."""
from copy import deepcopy
import json

import numpy as np
import pytest

bridge = pytest.importorskip('sts_learning_bridge')
from sts_learning.strategic_field_policy import StrategicFieldEncoder, LOCAL_CARD_READOUT
from sts_learning.semantic_batch import select_semantic_decision_rows


def recipe(rooms=None):
    value = dict(loadout=dict(seed=2026101501,ascension_level=20,current_hp=70,max_hp=75,
                             boss='TheGuardian',relics=['BurningBlood'],
                             cards=[dict(id='Strike'),dict(id='Bash'),dict(id='Defend')]),
                 floor=3,offers=[dict(id='Disarm'),dict(id='PommelStrike'),dict(id='Clash')])
    if rooms is not None: value['corridor_rooms'] = rooms
    return value


def corridor(*assignments):
    rooms = ['MonsterRoom']*14+['RestRoom']
    for y,room in assignments: rooms[y]=room
    return rooms


def map_facts(batch,schema,row):
    batch = select_semantic_decision_rows(batch,[row])
    token, = np.flatnonzero(batch['semantic']['token']['kind']==schema['token_kind']['Map'])
    names = {value:name for name,value in schema['scalar_field'].items()}
    table = batch['semantic']['scalar']
    return {names[int(f)]:int(v) for t,f,v in zip(table['token_indices'],table['field'],table['value'],strict=True)
            if t==token and (names[int(f)].startswith('Preparation') or names[int(f)]=='MapPreparationStartCount')}


def test_native_preparation_distinguishes_next_elite_from_a_preceding_rest_with_same_old_statistics():
    schema = bridge.semantic_schema()
    env = bridge.LearningBatchEnv.constructed_card_rewards(json.dumps([
        recipe(corridor((3,'MonsterRoomElite'),(4,'RestRoom'))),
        recipe(corridor((3,'RestRoom'),(4,'MonsterRoomElite'))),
    ]))
    batch = env.decision_batch(semantic=True)
    assert map_facts(batch,schema,0)==dict(MapPreparationStartCount=1,PreparationEliteWithoutRestOrShopSteps=1)
    assert map_facts(batch,schema,1)==dict(MapPreparationStartCount=1,PreparationEliteWithRestOnlySteps=2)
    encoder = StrategicFieldEncoder(schema,readout=LOCAL_CARD_READOUT)
    features,splits = encoder.candidate_features(batch)
    assert features[0] != features[int(splits[1])]
    # Deliberately remove only the new fields to reproduce the schema30 collision.
    old = deepcopy(batch); table=old['semantic']['scalar']; keep=table['field']<=171
    for key in ('token_indices','field','value'): table[key]=table[key][keep]
    prior,_=encoder.candidate_features(old)
    assert prior[0] == prior[int(splits[1])]


def test_native_preparation_excludes_current_room_and_retains_both_on_one_future_path():
    schema=bridge.semantic_schema()
    env=bridge.LearningBatchEnv.constructed_card_rewards(json.dumps([
        recipe(corridor((2,'RestRoom'),(3,'ShopRoom'),(4,'MonsterRoomElite'))),
        recipe(corridor((2,'ShopRoom'),(3,'MonsterRoomElite'))),
        recipe(corridor((3,'RestRoom'),(4,'ShopRoom'),(5,'MonsterRoomElite'))),
        recipe(),
    ]))
    batch=env.decision_batch(semantic=True)
    assert map_facts(batch,schema,0)==dict(MapPreparationStartCount=1,PreparationEliteWithShopOnlySteps=2)
    assert map_facts(batch,schema,1)==dict(MapPreparationStartCount=1,PreparationEliteWithoutRestOrShopSteps=1)
    assert map_facts(batch,schema,2)==dict(MapPreparationStartCount=1,PreparationEliteWithRestAndShopSteps=3)
    assert map_facts(batch,schema,3)==dict(MapPreparationStartCount=1,PreparationFirstKnownEliteFreeEndSteps=13)


@pytest.mark.parametrize('rooms',[
    ['MonsterRoom']*15,['MonsterRoom']*13+['RestRoom'],
    corridor((3,'MonsterRoomBoss')),corridor((3,'TrueVictoryRoom')),
])
def test_constructed_reward_rejects_ambiguous_corridor_extent(rooms):
    with pytest.raises(ValueError,match='15 ordinary-act rooms ending in RestRoom'):
        bridge.LearningBatchEnv.constructed_card_rewards(json.dumps([recipe(rooms)]))
