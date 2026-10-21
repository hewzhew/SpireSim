// Human-only map read through the official Remote and projection contracts.
// No game creation, model request, game action or prompt insertion happens here.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { Remote, TypertRemoteService } from '@deepseek-ai/dsh-typert-protocol';
import { SessionId } from '@deepseek-ai/dsh-session';
import { z } from 'zod';
import { nativeBinding, isWorkspaceEvent } from './native.mjs';
import { mapGraphFromObservation } from '../run_play/render.mjs';
import { renderMapPicture } from './map-layout.mjs';
import * as ObservationView from './observation-view.mjs';

export const inject = ['sessionController', 'sessionProjections', 'typert'];
const run = promisify(execFile);
const bossNames = { SlimeBoss: '史莱姆老大', Hexaghost: '六火亡魂', TheGuardian: '守护者',
  TheChamp: '冠军', Collector: '收藏家', Automaton: '铜制自动机',
  AwakenedOne: '觉醒者', TimeEater: '时间吞噬者', DonuAndDeca: '甜圈与八体', TheHeart: '腐化之心' };

export function mapViewFromDecision(document) {
  const observation = document.decision?.observation;
  const context = observation?.domain === 'strategic' ? observation.observation : observation?.run_context;
  const publicMap = context?.public_map;
  if (!publicMap) return { status: 'unavailable', message: '当前局面没有公开地图。' };
  const map = { current: [publicMap.current_x, publicMap.current_y], boss: publicMap.boss,
    nodes: publicMap.nodes.map(node => [node.x, node.y, node.room, node.has_emerald_key,
      node.edges.map(edge => [edge.destination_x, edge.destination_y])]) };
  const { nodes, current, boss } = mapGraphFromObservation({ position: { kind: 'decision', observation: {
    domain: 'strategic', observation: { public_map: map },
  } } });
  const run = context.run ?? context;
  return { status: 'ready', act: run.act, floor: run.floor, boss, current: map.current,
    ...renderMapPicture(nodes, current, { bossTitle: bossNames[boss] ?? boss ?? '首领尚未公开' }),
    note: map.current[1] < 0 ? '尚未进入本幕第一个房间' : `当前位置 (${current})`,
  };
}

export const mapTickProjection = {
  key: 'stsMapTick', stateVersion: 1,
  stateSchema: z.object({ seq: z.number(), callId: z.string().nullable() }),
  init: () => ({ seq: -1, callId: null }),
  apply(state, event) {
    if (isWorkspaceEvent(event)) return { seq: event.seq, callId: null };
    if (event.type === 'tool/call' && event.data.name === 'sts') return { ...state, callId: event.data.callId };
    if (event.type === 'tool/result' && (event.data.meta?.schema === 'sts-native-game-v1'
        || state.callId === event.data.message.toolCallId)) return { seq: event.seq, callId: null };
    return state;
  },
  wire: { viewSchema: z.number(), view: state => state.seq },
};

const initializers = [];
export class StsMapView extends TypertRemoteService {
  constructor(ctx, { execute = run, inspectSession } = {}) {
    super(ctx, 'stsMapView');
    this.execute = execute;
    this.inspectSession = inspectSession ?? (id => ctx.sessionController.inspect(SessionId(id)));
    for (const initialize of initializers) initialize.call(this);
  }
  async read(sessionId) {
    if (typeof sessionId !== 'string' || !sessionId) throw new Error('需要指定当前会话。');
    const { meta } = await this.inspectSession(sessionId);
    if (meta.id !== sessionId) throw new Error('会话记录与选中的聊天不一致。');
    const binding = await nativeBinding({ id: sessionId, session: { header: meta } }, {}, { create: false });
    if (!binding) return { status: 'not-started', message: '这段聊天还没有游戏。让玩家开始一局后，地图会自动出现。' };
    const { stdout } = await this.execute(binding.binary, ['show', '--session', binding.session, '--decision-json'],
      { cwd: binding.repository, windowsHide: true, timeout: 15000, maxBuffer: 8 * 1024 * 1024 });
    return mapViewFromDecision(JSON.parse(stdout));
  }
}
// Standard decorator initializer, written explicitly because this package is JS.
Remote(StsMapView.prototype.read, { kind: 'method', name: 'read', static: false, private: false,
  addInitializer: initialize => initializers.push(initialize) });

export function apply(ctx) {
  new StsMapView(ctx);
  ctx.effect(() => ctx.sessionProjections.register(mapTickProjection));
  ctx.plugin(ObservationView);
}
