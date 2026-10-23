const phases = { combat: 'combat', reward: 'reward', card_reward: 'reward', map: 'map', event: 'event', rewards: 'reward' };
export const phaseKind = kind => phases[kind];

export function publicMapView(map) {
  if (!map) return null;
  if (JSON.stringify(map.node_tuple_fields) !== JSON.stringify(['x', 'y', 'room', 'emerald_key', 'destinations'])) throw new Error('Unbound public map tuple layout');
  const coordinate = point => Array.isArray(point) && point.length === 2 && point.every(Number.isSafeInteger);
  if (!coordinate(map.current) || !Array.isArray(map.nodes)) throw new Error('Invalid public map');
  const nodes = map.nodes.map(row => {
    if (row.length !== 5 || !coordinate(row.slice(0, 2)) || typeof row[3] !== 'boolean' || !Array.isArray(row[4]) || !row[4].every(coordinate)
        || !['monster', 'elite', 'question_mark', 'rest', 'shop', 'treasure', 'boss'].includes(row[2])) throw new Error('Invalid public map node');
    return { x: row[0], y: row[1], room: row[2], emerald_key: row[3], destinations: row[4].map(point => [...point]) };
  });
  const keys = new Set(nodes.map(node => `${node.x},${node.y}`));
  if (keys.size !== nodes.length || nodes.some(node => node.destinations.some(point => !keys.has(String(point))))) throw new Error('Disconnected public map reference');
  return { current: [...map.current], boss: map.boss, nodes };
}

// Recording membership is explicit. Floor labels do not merge unrelated runs,
// and source map rows never become floor numbers by arithmetic.
export function recordedJourney(story) {
  const seen = new Set();
  const recordings = story.recordings.map(recording => {
    const visits = [];
    for (const sceneId of recording.scene_ids) {
      const scene = story.scenes.find(scene => scene.id === sceneId);
      if (!scene || seen.has(sceneId)) throw new Error('Missing or duplicate recording scene');
      seen.add(sceneId);
      const states = scene.kind === 'sequence' ? scene.snapshots : [scene];
      states.forEach((state, cursor) => {
        const location = scene.kind === 'sequence' ? state.location : recording.locations?.[sceneId];
        if (!Number.isSafeInteger(location?.act) || !Number.isSafeInteger(location?.floor)) throw new Error('Missing recorded visit location');
        const identity = JSON.stringify([location.act, location.floor, location.node, location.visit_id]);
        let visit = visits.at(-1);
        if (!visit || visit.identity !== identity) {
          visit = { identity, act: location.act, floor: location.floor, node: location.node,
            encounter_id: scene.encounter_id, event_id: scene.event_id, phases: [] };
          visits.push(visit);
        }
        const kind = phaseKind(state.kind);
        if (!kind) throw new Error('Unbound journey phase');
        const previous = visit.phases.at(-1);
        if (previous?.kind === kind && previous.scene_id === sceneId) previous.end_cursor = cursor;
        else visit.phases.push({ kind, scene_id: sceneId, cursor, end_cursor: cursor, turn: scene.turn });
      });
    }
    return { ...recording, visits };
  });
  if (seen.size !== story.scenes.length) throw new Error('Unassigned recording scene');
  return recordings;
}

export function replayMap(scene, cursor) {
  const state = scene.snapshots[cursor], location = state.location;
  if (!location?.map_id) return null;
  const graph = scene.maps.find(map => map.id === location.map_id);
  if (!graph) throw new Error('Missing recorded map graph');
  const visited = new Map(), route = [];
  let previous;
  for (let index = 0; index <= cursor; index++) {
    const place = scene.snapshots[index].location;
    if (place?.map_id !== location.map_id) { previous = undefined; continue; }
    const key = String(place.node);
    if (!visited.has(key)) visited.set(key, { scene_id: scene.id, cursor: index });
    if (previous && previous !== key) {
      const node = graph.nodes.find(node => `${node.x},${node.y}` === previous);
      if (node?.destinations.some(point => String(point) === key)) route.push([previous, key]);
    }
    previous = key;
  }
  return { ...graph, current: location.node, route, nodes: graph.nodes.map(node => ({ ...node,
    current: String(location.node) === `${node.x},${node.y}`, target: visited.get(`${node.x},${node.y}`) })) };
}
