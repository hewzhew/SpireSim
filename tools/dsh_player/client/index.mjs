// Same client module and sidebar extension contracts as the shipped dsh viewers.
import { mapNamespace, gameNamespace, mapDictionaries, gameDictionaries, mapTokenTitle } from './locale.mjs';

window.__ModuleLoader__.load({ id: '@sts-simulator/dsh-player', factory(require) {
  const React = require('react');
  const h = React.createElement;
  const inject = ['slots', 'sidebarRight', 'sidebarRightTabs', 'connection', 'locale'];
  const tabId = 'sts-map';
  const observeTabId = 'sts-observe';
  const buttonStyle = { padding: '5px 9px', border: '1px solid color-mix(in srgb, currentColor 16%, transparent)',
    borderRadius: 6, background: 'transparent', color: 'inherit', cursor: 'pointer', font: 'inherit', fontSize: 12 };
  const legend = [['?', 'question_mark'], ['$', 'shop'], ['T', 'treasure'], ['R', 'rest'], ['M', 'monster'], ['E', 'elite']];
  const outcomeNames = { completed: '已返回', error: '报错', unconfirmed: '未确认', model: '模型输出' };
  const phaseNames = { completed: '已结束', cancelled: '已取消', error: '出错',
    'running or interrupted without a recorded end': '尚无结束记录', 'no model turn': '尚无模型运行记录' };
  const inputTokens = buckets => buckets.uncachedInputTokens + buckets.cacheReadTokens + buckets.cacheWriteTokens;
  const count = value => value.toLocaleString('zh-CN');
  function requestUsage(usage) {
    if (!usage) return null;
    const isCount = value => Number.isSafeInteger(value) && value >= 0;
    const amount = value => isCount(value) ? count(value) : '未报告';
    const reportedInput = isCount(usage.totalTokens) && isCount(usage.outputTokens) ? usage.totalTokens - usage.outputTokens : null;
    const buckets = [usage.inputTokens, usage.cacheReadTokens, usage.cacheWriteTokens];
    const total = isCount(reportedInput) ? reportedInput : buckets.every(isCount) ? buckets.reduce((sum, value) => sum + value, 0) : null;
    return h('div', { className: 'observe-request-usage', 'aria-label': '本步最后一次报告的用量' },
      h('span', { title: '包括未缓存输入、缓存读取和缓存写入；不代表本步新增内容。' }, '总输入 ' + amount(total)),
      h('span', null, '未缓存 ' + amount(usage.inputTokens)),
      h('span', null, '缓存读取 ' + amount(usage.cacheReadTokens)),
      usage.cacheWriteTokens > 0 ? h('span', null, '缓存写入 ' + amount(usage.cacheWriteTokens)) : null,
      h('span', null, '输出 ' + amount(usage.outputTokens)));
  }
  const observationStyles = `
.sts-observe{height:100%;box-sizing:border-box;padding:14px;display:flex;flex-direction:column;gap:10px;overflow:auto;container-type:inline-size;font-size:13px;line-height:1.5}
.sts-observe button,.sts-observe select,.sts-observe input{font:inherit;color:inherit;background:transparent;border:1px solid color-mix(in srgb,currentColor 16%,transparent);border-radius:5px;padding:4px 8px;box-sizing:border-box}
.sts-observe button{cursor:pointer}.sts-observe button:disabled{opacity:.4;cursor:default}.sts-observe button:hover:not(:disabled){background:color-mix(in srgb,currentColor 7%,transparent)}
.sts-observe button:focus-visible,.sts-observe input:focus-visible,.sts-observe select:focus-visible{outline:2px solid color-mix(in srgb,currentColor 55%,transparent);outline-offset:1px}
.sts-observe p{margin:8px 0}.observe-header,.observe-detail-head,.observe-call-title{display:flex;align-items:center;justify-content:space-between;gap:8px}.observe-header>strong{font-size:15px}.observe-header>div,.observe-step-nav{display:flex;gap:6px}
.observe-summary{display:flex;align-items:center;gap:6px 12px;flex-wrap:wrap;font-size:12px}.observe-summary>button{border:0;padding:0}.observe-phase{font-weight:600}.observe-muted{opacity:.62}.observe-error-count,.observe-outcome.error{color:color-mix(in srgb,#bd3b35 80%,currentColor)}
.observe-usage{font-size:12px}.observe-usage summary{cursor:pointer}.observe-usage[open]{flex-basis:100%}.observe-toolbar{display:flex;gap:6px;align-items:center;flex-wrap:wrap}.observe-toolbar select{max-width:100%}.sts-observe .is-active{background:color-mix(in srgb,currentColor 9%,transparent);border-color:color-mix(in srgb,currentColor 40%,transparent)}
.sts-observe .observe-search{width:100%;min-width:0}.observe-search::placeholder{color:inherit;opacity:.55}.observe-body{display:flex;flex-direction:column;gap:10px;flex:1;min-height:0}
.observe-history{height:clamp(96px,22vh,176px);flex-shrink:0;overflow:auto;border:1px solid color-mix(in srgb,currentColor 12%,transparent);border-radius:7px;padding:3px}
.sts-observe .observe-row{display:block;width:100%;text-align:left;border:0;border-radius:4px;padding:7px 9px;margin:1px 0}.sts-observe .observe-row.is-selected{background:color-mix(in srgb,currentColor 10%,transparent);box-shadow:inset 2px 0 currentColor}
.observe-row-meta{display:flex;justify-content:space-between;gap:8px;font-size:11px;opacity:.7}.observe-row-label{display:block;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;font-family:Consolas,"Cascadia Mono",monospace;font-size:12px;margin-top:2px}.observe-outcome{font-size:11px}
.observe-detail{flex:1;min-height:200px;display:flex;flex-direction:column;overflow:hidden;border:1px solid color-mix(in srgb,currentColor 12%,transparent);border-radius:7px}
.observe-detail-head{padding:9px 10px}.observe-step-nav button{padding:2px 8px}.observe-tabs{display:flex;gap:4px;padding:0 10px 8px;border-bottom:1px solid color-mix(in srgb,currentColor 10%,transparent)}.observe-tabs button{border:0}.observe-tabs .is-active{box-shadow:inset 0 -2px currentColor}
.observe-reader-tools{display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:6px 8px;padding:7px 10px;font-size:11px}.observe-reader-tools button{padding:2px 7px}.observe-reader{padding:0 10px 12px;overflow:auto;flex:1;min-height:0}
.observe-request-usage{display:flex;flex-wrap:wrap;gap:4px 12px;padding:0 10px 8px;font-size:11px;opacity:.68}.observe-request-usage span{white-space:nowrap}
.observe-code,.observe-command{font-family:Consolas,"Cascadia Mono",monospace;font-size:12px;line-height:1.6;white-space:pre-wrap;overflow-wrap:anywhere;margin:7px 0}.observe-command{font-weight:600}.observe-call+.observe-call,.observe-message+.observe-message{border-top:1px solid color-mix(in srgb,currentColor 10%,transparent);padding-top:12px;margin-top:12px}
.observe-message .observe-code{font-family:inherit;font-size:13px}.observe-notice{font-size:12px;opacity:.7}.observe-input-message,.observe-input-extra{border:1px solid color-mix(in srgb,currentColor 12%,transparent);border-radius:5px;padding:7px 9px;margin:7px 0}.observe-input-message summary,.observe-input-extra summary{cursor:pointer;overflow-wrap:anywhere}.observe-input-preview{display:block;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;opacity:.6;font-size:12px;margin-top:3px}.observe-input-message .observe-code{margin-top:10px}.observe-delivery+.observe-delivery{border-top:1px solid color-mix(in srgb,currentColor 12%,transparent);margin-top:14px;padding-top:10px}
@container(min-width:640px){.observe-body{display:grid;grid-template-columns:minmax(200px,.85fr) minmax(0,1.5fr)}.observe-history{height:auto;min-height:0}.observe-detail{min-height:0}}
.observe-header{flex-wrap:wrap}.observe-header>div{flex-wrap:wrap}.workspace-controls{display:flex;gap:8px;align-items:center;flex-wrap:wrap}.workspace-controls>span{font-size:12px}.return-point{border-top:1px solid color-mix(in srgb,currentColor 12%,transparent);padding:10px 0;display:flex;align-items:center;justify-content:space-between;gap:12px}.return-point>div{min-width:0;overflow-wrap:anywhere}.return-point small{display:block;opacity:.65}.return-point button{flex-shrink:0}.workspace-events summary{cursor:pointer}.workspace-events p{overflow-wrap:anywhere}
`;
  const duration = ms => ms < 1000 ? Math.round(ms) + ' ms' : (ms / 1000).toFixed(1) + ' 秒';
  function returnLocation(point) {
    const location = point?.location;
    if (!location) return '位置未提供';
    const sites = { map: '地图', reward: '奖励', rewards: '奖励', shop: '商店', event: '事件', campfire: '营火',
      card_reward: '卡牌奖励', boss_relic: 'Boss 遗物', selection: '选牌', chest: '宝箱' };
    return location.kind === 'terminal' ? location.label : '第 ' + location.act + ' 幕 F' + location.floor + ' '
      + (location.kind === 'combat' ? location.encounter + ' T' + location.turn : sites[location.site] ?? location.site);
  }
  const returnResources = point => point.resources ? 'HP ' + point.resources.hp + '/' + point.resources.max_hp
    + '，' + point.resources.gold + ' 金，药水 ' + (point.resources.potions.filter(Boolean).join(' / ') || '无') : '';
  const pointOrigins = point => point.origins.map(origin => origin.kind === 'recent' ? origin.steps + ' 次提交前'
    : origin.kind === 'floor' ? 'F' + origin.floor + ' 落点' : origin.kind === 'mark' ? origin.name : '刚才的终点').join('，');
  function renderInputs(inputs, copyMessage) {
    if (!inputs.length) return h('p', { className: 'observe-muted' }, '本步没有保存请求边界，原始输入不可用。');
    const roles = { system: '系统', user: '用户', assistant: '模型', tool: '工具返回' };
    return h(React.Fragment, null, h('p', { className: 'observe-muted' }, '按实际请求边界展开输入。'),
      inputs.map((item, index) => h('div', { key: item.eventSeq, className: 'observe-delivery' },
        h('p', null, '输入记录 ' + (index + 1) + '，' + item.request.messages.length + ' 条消息 · '
          + (item.precision === 'journal-derived-at-recorded-delivery' ? '服务端交付记录' : 'SDK 请求记录')),
        item.request.messages.map((message, index) => h('details', { key: index, className: 'observe-input-message' },
          h('summary', null, (index + 1) + '  ' + (roles[message.role] ?? message.role),
            h('span', { className: 'observe-input-preview' }, message.content.find(block => block.type === 'text')?.text.split('\n')[0].slice(0, 120)
              ?? (message.content.some(block => block.type === 'tool-call') ? '工具调用声明' : ''))),
          message.content.map((block, index) => h('div', { key: index },
            block.type === 'reasoning' ? h('span', { className: 'observe-muted' }, '推理') : null,
            h('pre', { className: 'observe-code' }, block.text ?? JSON.stringify(block, null, 2)))),
          h('button', { type: 'button', onClick: () => copyMessage(JSON.stringify(message, null, 2)) }, '复制消息'))),
        h('details', { className: 'observe-input-extra' }, h('summary', null, '工具定义'),
          h('pre', { className: 'observe-code' }, item.request.tools ? JSON.stringify(item.request.tools, null, 2) : '请求中未提供独立工具定义列表。')),
        h('details', { className: 'observe-input-extra' }, h('summary', null, '请求 JSON'),
          h('pre', { className: 'observe-code' }, JSON.stringify(item.request, null, 2))))));
  }
  function tokenStyle(token, highlighted) {
    const dim = token.state === 'past' || token.state === 'unreachable';
    if (token.kind === 'axis') return { opacity: token.state === 'current' ? 0.85 : 0.5 };
    if (token.kind === 'room') return { opacity: dim ? 0.62 : 1,
      display: 'inline-block', position: 'relative', fontWeight: token.state === 'current' ? 700 : 500,
      ...(token.emerald ? { color: 'color-mix(in srgb, #558640 75%, currentColor)' } : {}),
      ...(token.text === highlighted ? { background: 'color-mix(in srgb, currentColor 18%, transparent)',
        boxShadow: '0 0 0 2px color-mix(in srgb, currentColor 18%, transparent)', borderRadius: 3 } : {}),
    };
    return { opacity: dim ? 0.4 : 0.6 };
  }
  function apply(ctx) {
    ctx.effect(() => ctx.locale.register(mapNamespace, mapDictionaries));
    ctx.effect(() => ctx.locale.register(gameNamespace, gameDictionaries));
    const mapText = ctx.locale.bind(mapNamespace), gameText = ctx.locale.bind(gameNamespace);
    function MapButton({ t }) {
      return h('button', { type: 'button', title: t('open'),
        onClick: () => ctx.sidebarRight.openTab(tabId),
        style: { padding: '4px 8px', border: '1px solid currentColor', borderRadius: 6, background: 'transparent', color: 'inherit', cursor: 'pointer' },
      }, t('button'));
    }
    function ObserveButton() {
      return h('button', { type: 'button', title: '查看过程、暂停或返回游戏',
        onClick: () => ctx.sidebarRight.openTab(observeTabId), style: buttonStyle }, '对局');
    }
    function ObservePanel({ sessionId, useProjection }) {
      const tick = useProjection('stsObservationTick'), gameTick = useProjection('stsMapTick');
      const [view, setView] = React.useState(null), [error, setError] = React.useState('');
      const [revision, refresh] = React.useReducer(n => n + 1, 0);
      const [round, setRound] = React.useState(null), [focus, setFocus] = React.useState(null);
      const [search, setSearch] = React.useState(''), [errorsOnly, setErrorsOnly] = React.useState(false);
      const [content, setContent] = React.useState(null), [detail, setDetail] = React.useState(null);
      const [detailError, setDetailError] = React.useState(null);
      const [live, setLive] = React.useState(null), [liveRequest, setLiveRequest] = React.useState(null);
      const [copyNotice, setCopyNotice] = React.useState(null);
      const [returns, setReturns] = React.useState(null), [controlResult, setControlResult] = React.useState(null);
      const [working, setWorking] = React.useState(null);
      const operationRef = React.useRef(false);
      const panelRef = React.useRef(null);
      const turn = round?.sessionId === sessionId ? round.turn : null;
      const current = view?.sessionId === sessionId && view.requestedTurn === turn ? view : null;
      const timeline = current?.timeline ?? [];
      const pinned = focus?.sessionId === sessionId && focus.turn === current?.turn;
      const selected = pinned ? timeline.find(item => item.step === focus.step) : timeline.at(-1);
      const key = selected ? sessionId + ':' + current.turn + ':' + selected.step : null;
      const mode = content?.sessionId === sessionId && ['live', 'returns'].includes(content.mode) ? content.mode
        : content?.key === key ? content.mode : selected?.calls.length ? 'result' : 'model';
      const wantsInput = mode === 'input';
      const selectedDetail = detail?.key === key ? detail : null;
      const modelMessages = selectedDetail?.messages.map(message => ({ ...message, blocks: message.blocks.filter(block => block.text.trim()) }))
        .filter(message => message.blocks.length) ?? [];
      const selectedError = detailError?.key === key && (!detailError.input || wantsInput) ? detailError.message : null;
      const currentLive = live?.sessionId === sessionId && live.request === liveRequest ? live : null;
      const readingLive = liveRequest?.sessionId === sessionId && !currentLive;
      const currentReturns = returns?.sessionId === sessionId ? returns : null;
      const actionResult = controlResult?.sessionId === sessionId ? controlResult : null;
      const control = actionResult?.control && (actionResult.throughSeq ?? -1) >= (current?.throughSeq ?? -1)
        ? actionResult.control : current?.control;
      const busy = Boolean(working);
      const query = search.trim().toLowerCase();
      const visible = timeline.filter(item => (!errorsOnly || item.outcome === 'error')
        && (!query || (item.label + ' ' + item.calls.map(call => call.name + ' ' + call.command).join(' ')).toLowerCase().includes(query)));
      const previous = visible.findLast(item => item.seq < (selected?.seq ?? Infinity));
      const next = visible.find(item => item.seq > (selected?.seq ?? -1));
      React.useEffect(() => {
        const history = panelRef.current?.querySelector('.observe-history');
        const active = history?.querySelector('[aria-current="step"]');
        if (!active) return;
        const bounds = history.getBoundingClientRect(), row = active.getBoundingClientRect();
        if (row.top < bounds.top) history.scrollTop += row.top - bounds.top - 3;
        else if (row.bottom > bounds.bottom) history.scrollTop += row.bottom - bounds.bottom + 3;
      }, [key, query, errorsOnly]);
      React.useEffect(() => {
        const reader = panelRef.current?.querySelector('.observe-reader');
        if (reader) reader.scrollTop = 0;
      }, [key, mode]);
      React.useEffect(() => {
        const abort = new AbortController();
        setError('');
        ctx.connection.rpc.call('/api', 'stsObservation/read', { args: { sessionId, turn } }, abort.signal).then(result => {
          if (abort.signal.aborted) return;
          if (result.ok && result.value.status === 'ready' && !Array.isArray(result.value.timeline)) setError('观察服务尚未更新到这版界面，请更新服务后刷新。');
          else if (result.ok) setView({ ...result.value, sessionId, requestedTurn: turn }); else setError(result.error.message);
        }).catch(error => { if (!abort.signal.aborted) setError(error.message); });
        return () => abort.abort();
      }, [sessionId, turn, tick, revision]);
      React.useEffect(() => {
        if (!key || ['live', 'returns'].includes(mode)) return;
        if (selectedDetail && selectedDetail.throughSeq >= selected.revision && (!wantsInput || selectedDetail.inputs)) return;
        const abort = new AbortController();
        setDetailError(null);
        ctx.connection.rpc.call('/api', 'stsObservation/step', { args: {
          sessionId, turn: current.turn, step: selected.step, input: wantsInput,
        } }, abort.signal).then(result => {
          if (abort.signal.aborted) return;
          if (result.ok && result.value.status === 'ready') setDetail({ ...result.value, key });
          else setDetailError({ key, input: wantsInput, message: result.ok ? result.value.message : result.error.message });
        }).catch(error => { if (!abort.signal.aborted) setDetailError({ key, input: wantsInput, message: error.message }); });
        return () => abort.abort();
      }, [key, selected?.revision, wantsInput, mode === 'live' || mode === 'returns', revision]);
      React.useEffect(() => {
        if (mode !== 'returns') return;
        const abort = new AbortController();
        ctx.connection.rpc.call('/api', 'stsObservation/points', { args: { sessionId } }, abort.signal).then(result => {
          if (!abort.signal.aborted) setReturns({ ...(result.ok ? result.value : { status: 'error', message: result.error.message }), sessionId });
        }).catch(error => { if (!abort.signal.aborted) setReturns({ status: 'error', message: error.message, sessionId }); });
        return () => abort.abort();
      }, [sessionId, mode === 'returns', gameTick, revision]);
      React.useEffect(() => {
        if (liveRequest?.sessionId !== sessionId) return;
        const abort = new AbortController();
        ctx.connection.rpc.call('/api', 'stsObservation/live', { args: { sessionId } }, abort.signal).then(result => {
          if (abort.signal.aborted) return;
          setLive({ ...(result.ok ? result.value : { status: 'error', message: result.error.message }),
            sessionId, gameTick: liveRequest.gameTick, request: liveRequest });
        }).catch(error => { if (!abort.signal.aborted) setLive({ status: 'error', message: error.message, sessionId, request: liveRequest }); });
        return () => abort.abort();
      }, [sessionId, liveRequest]);
      const choose = item => { setFocus({ sessionId, turn: current.turn, step: item.step }); setContent(null); };
      const follow = () => { setRound(null); setFocus(null); setContent(null); setSearch(''); setErrorsOnly(false); };
      const inspectLive = () => { setContent({ sessionId, mode: 'live' }); setLiveRequest({ sessionId, gameTick }); };
      async function operate(kind, point) {
        if (operationRef.current) return;
        operationRef.current = true;
        setWorking({ sessionId, kind });
        try {
          const args = { sessionId };
          if (kind === 'restore') args.request = { schema: 'run-play-return-request-v1', source: currentReturns.points.source, target: point.identity };
          const result = await ctx.connection.rpc.call('/api', 'stsObservation/' + kind, { args });
          const value = result.ok ? result.value : { status: 'error', message: result.error.message };
          setControlResult({ ...value, sessionId });
          if (value.points) setReturns({ status: 'ready', points: value.points, control: value.control, sessionId });
          else if (kind === 'restore') setReturns({ status: 'error', message: value.message ?? '需要重新读取返回点。', sessionId });
          if (kind !== 'resume' && value.text) {
            const request = { sessionId, gameTick };
            setLiveRequest(request); setLive({ status: 'ready', text: value.text, sessionId, request, gameTick });
          }
          if (kind === 'pause' || kind === 'restore') setContent({ sessionId, mode: 'returns' });
          refresh();
        } catch (error) { setControlResult({ status: 'error', message: error.message, sessionId }); }
        finally { setWorking(null); operationRef.current = false; }
      }
      const totals = current?.accounting?.totals, last = current?.accounting?.last;
      const text = mode === 'returns' ? actionResult?.text : mode === 'live' ? currentLive?.text : mode === 'input' ? selectedDetail?.inputs && JSON.stringify(selectedDetail.inputs, null, 2)
        : mode === 'model' ? selectedDetail?.messages.map(message => message.blocks.map(block => block.text).join('\n')).join('\n\n')
        : selectedDetail?.calls.map(call => call.name + '\n' + call.command + '\n' + (call.text ?? '尚无返回，执行结果未确认。')).join('\n\n');
      const copyKey = mode === 'live' ? liveRequest : key + ':' + mode;
      async function copyText(value) {
        try { await navigator.clipboard.writeText(value); setCopyNotice({ key: copyKey, text: '已复制' }); }
        catch { setCopyNotice({ key: copyKey, text: '复制失败，请在内容中手动选取。' }); }
      }
      const tab = (id, label) => h('button', { type: 'button', 'aria-pressed': mode === id,
        className: mode === id ? 'is-active' : '', onClick: () => setContent({ sessionId, key, mode: id }) }, label);
      return h('section', { className: 'sts-observe', ref: panelRef },
        h('style', null, observationStyles),
        h('header', { className: 'observe-header' }, h('strong', null, '对局工作台'),
          h('div', null, h('button', { type: 'button', onClick: inspectLive }, '当前游戏'),
            h('button', { type: 'button', onClick: () => setContent({ sessionId, mode: 'returns' }) }, '返回点'),
            h('button', { type: 'button', onClick: refresh }, '刷新'))),
        current?.status === 'ready' ? h('div', { className: 'workspace-controls' },
          h('span', null, busy ? '正在' + ({ pause: '暂停', restore: '返回', resume: '继续' }[working.kind]) + '…'
            : control?.paused ? '已暂停' : control?.running ? '模型运行中' : '模型已停止'),
          control?.queued ? h('span', { className: 'observe-muted' }, control.queued + ' 条消息等待处理') : null,
          h('button', { type: 'button', disabled: busy || control?.available === false, onClick: () => operate(control?.paused ? 'resume' : 'pause') },
            control?.paused ? '继续运行' : '暂停'),
          control?.available === false ? h('span', { className: 'observe-muted' }, '本聊天仍用旧版工具，新建聊天可使用返回功能。') : null,
          actionResult?.status === 'error' ? h('p', { role: 'alert' },
            (actionResult.receipt ? '返回已经提交，后续核对失败：' : '') + actionResult.message) : null) : null,
        error ? h('p', { role: 'alert' }, error) : !current ? h('p', null, '正在读取记录…')
          : current.status !== 'ready' ? h('p', null, current.message) : h(React.Fragment, null,
            h('div', { className: 'observe-summary' },
              h('span', { className: 'observe-phase' }, phaseNames[current.phase] ?? current.phase),
              h('span', null, timeline.length + ' 步'),
              h('span', null, current.gameCalls + ' 次游戏调用'),
              h('button', { type: 'button', className: current.errors ? 'observe-error-count' : '', 'aria-pressed': errorsOnly,
                onClick: () => setErrorsOnly(!errorsOnly) }, current.errors + ' 次报错'),
              h('details', { className: 'observe-usage' }, h('summary', null, '用量'),
                totals ? h('div', null,
                  h('p', null, '截至这轮累计 ' + count(inputTokens(totals) + totals.outputTokens) + ' tokens'),
                  h('p', null, last ? '最后一次已报告输入 ' + count(inputTokens(last)) + ' tokens' : '尚无最近一次请求用量'),
                  h('p', { className: 'observe-muted' }, '未缓存 ' + count(totals.uncachedInputTokens) + '，缓存读取 ' + count(totals.cacheReadTokens)
                    + '，缓存写入 ' + count(totals.cacheWriteTokens) + '，输出 ' + count(totals.outputTokens) + '。累计量包含历史的反复传入。'))
                  : h('p', null, '模型尚未报告 token 用量。'))),
            current.recordedMayBeStale ? h('p', { className: 'observe-notice' }, '日志画面可能已过时，实际进度请查看当前游戏。') : null,
            current.interventions?.length ? h('details', { className: 'workspace-events' }, h('summary', null, '最近人工操作'),
              current.interventions.map(event => h('p', { key: event.seq },
                ({ 'pause-requested': '请求暂停', 'return-requested': '请求返回', paused: '已暂停', returned: '已返回', resumed: '继续运行', 'pause-failed': '暂停操作报错', 'return-failed': '返回操作报错', 'resume-failed': '继续操作报错' })[event.kind],
                event.receipt ? '：状态 ' + event.receipt.from.state_revision + ' → ' + event.receipt.to.state_revision : '',
                event.message ? ' ' + event.message : ''))) : null,
            h('div', { className: 'observe-toolbar' },
              h('select', { value: turn ?? '', 'aria-label': '查看轮次', onChange: event => {
                setRound({ sessionId, turn: event.target.value ? Number(event.target.value) : null }); setFocus(null); setContent(null);
              } }, h('option', { value: '' }, '最近一轮'),
                [...current.turns].reverse().map(number => h('option', { key: number, value: number }, '第 ' + number + ' 轮'))),
              h('button', { type: 'button', 'aria-pressed': errorsOnly, onClick: () => setErrorsOnly(!errorsOnly) }, '仅报错'),
              h('button', { type: 'button', className: !pinned && turn === null ? 'is-active' : '', onClick: follow },
                !pinned && turn === null ? '跟随最新' : '回到最新')),
            h('input', { className: 'observe-search', type: 'search', 'aria-label': '搜索命令或工具名', placeholder: '搜索命令或工具名',
              value: search, onChange: event => setSearch(event.target.value) }),
            h('div', { className: 'observe-body' },
              h('div', { className: 'observe-history', 'aria-label': '步骤列表' },
                !visible.length ? h('p', { className: 'observe-muted' }, timeline.length ? '没有匹配的步骤。' : '这一轮尚无步骤。')
                  : [...visible].reverse().map(item => h('button', { key: item.seq, type: 'button', title: item.label,
                    className: 'observe-row' + (selected?.step === item.step ? ' is-selected' : ''),
                    'aria-current': selected?.step === item.step ? 'step' : undefined,
                    'aria-label': '第 ' + item.step + ' 步 ' + item.label, onClick: () => choose(item) },
                    h('span', { className: 'observe-row-meta' }, h('span', null, '第 ' + item.step + ' 步'),
                      h('span', { className: 'observe-outcome ' + item.outcome }, outcomeNames[item.outcome])),
                    h('span', { className: 'observe-row-label' }, item.label)))),
              h('div', { className: 'observe-detail' },
                h('div', { className: 'observe-detail-head' },
                  h('strong', null, mode === 'returns' ? '返回点' : mode === 'live' ? '当前游戏' : selected ? '第 ' + current.turn + ' 轮，第 ' + selected.step + ' 步' : '步骤详情'),
                  !['live', 'returns'].includes(mode) ? h('div', { className: 'observe-step-nav' },
                    h('button', { type: 'button', disabled: !previous, 'aria-label': '上一步', onClick: () => choose(previous) }, '←'),
                    h('button', { type: 'button', disabled: !next, 'aria-label': '下一步', onClick: () => choose(next) }, '→')) : null),
                !['live', 'returns'].includes(mode) && selected ? h('nav', { className: 'observe-tabs', 'aria-label': '步骤内容' },
                  tab('result', '返回'), tab('model', '模型输出'), tab('input', '输入')) : null,
                h('div', { className: 'observe-reader-tools' },
                  h('span', { className: 'observe-muted' }, mode === 'returns' ? '返回后保持暂停' : mode === 'live' ? '只读核对'
                    : selected?.durationMs !== null && selected ? '本步耗时 ' + duration(selected.durationMs) : '尚无完整耗时'),
                  h('span', { role: 'status' }, copyNotice?.key === copyKey ? copyNotice.text : ''),
                  h('button', { type: 'button', disabled: !text, onClick: () => copyText(text) }, '复制')),
                !['live', 'returns'].includes(mode) ? requestUsage(selected?.usage) : null,
                h('div', { className: 'observe-reader', 'aria-label': '步骤详情内容' },
                  mode === 'returns' ? h(React.Fragment, null,
                    h('button', { type: 'button', disabled: busy, onClick: refresh }, '重新读取'),
                    !currentReturns ? h('p', null, '正在读取返回点…') : currentReturns.status !== 'ready' ? h('p', { role: 'alert' }, currentReturns.message)
                      : h(React.Fragment, null,
                        h('p', null, '当前：' + returnLocation(currentReturns.points.current)),
                        h('p', { className: 'observe-muted' }, returnResources(currentReturns.points.current)),
                        !currentReturns.points.targets.length ? h('p', null, '还没有保留的返回点。') : null,
                        currentReturns.points.targets.map(point => h('article', { className: 'return-point', key: point.identity.head_revision },
                          h('div', null, h('strong', null, returnLocation(point)),
                            h('small', null, pointOrigins(point)), h('small', null, returnResources(point))),
                          h('button', { type: 'button', disabled: busy || control?.available === false, onClick: () => operate('restore', point) },
                            point.origins.some(origin => origin.kind === 'previous_endpoint') ? '回到刚才' : '返回这里')))),
                    actionResult?.receipt ? h('p', { role: 'status' }, '返回已提交，当前游戏保持暂停。') : null,
                    actionResult?.text ? h('details', null, h('summary', null, '返回后实际局面与库存'), h('pre', { className: 'observe-code' }, actionResult.text)) : null)
                    : mode === 'live' ? h(React.Fragment, null,
                    h('button', { type: 'button', disabled: readingLive, onClick: inspectLive }, readingLive ? '正在核对…' : '重新核对'),
                    currentLive?.status === 'error' ? h('p', { role: 'alert' }, currentLive.message) : currentLive ? h(React.Fragment, null,
                      currentLive.gameTick !== gameTick ? h('p', { className: 'observe-notice' }, '核对后有新的游戏记录，需要时请重新核对。') : null,
                      h('pre', { className: 'observe-code' }, currentLive.text ?? currentLive.message)) : null)
                    : !selected ? h('p', null, '请选择一步查看。')
                      : h(React.Fragment, null,
                        selectedError ? h('p', { role: 'alert' }, selectedError) : null,
                        !selectedDetail || (wantsInput && !selectedDetail.inputs) ? !selectedError ? h('p', null, '正在读取…') : null
                          : mode === 'result' ? selectedDetail.calls.length ? selectedDetail.calls.map((call, index) => h('article', { className: 'observe-call', key: index },
                            h('div', { className: 'observe-call-title' }, h('strong', null, call.name),
                              h('span', { className: 'observe-outcome ' + call.outcome }, outcomeNames[call.outcome])),
                            h('pre', { className: 'observe-command' }, call.command),
                            h('pre', { className: 'observe-code' }, call.text ?? '尚无返回，执行结果未确认。'))) : h('p', { className: 'observe-muted' }, '这一步没有记录工具调用。')
                            : mode === 'model' ? modelMessages.length ? modelMessages.map((message, index) => h('article', { className: 'observe-message', key: index },
                              !message.committed ? h('p', { className: 'observe-notice' }, '这份模型输出未完成。') : null,
                              message.blocks.map((block, index) => h('div', { key: index },
                                h('span', { className: 'observe-muted' }, block.type === 'reasoning' ? '推理' : '回答'),
                                h('pre', { className: 'observe-code' }, block.text))))) : h('p', { className: 'observe-muted' }, '这一步没有模型文字输出记录。')
                              : renderInputs(selectedDetail.inputs, copyText))))
              ))
          );
    }
    function MapPanel({ sessionId, useProjection, t }) {
      const tick = useProjection('stsMapTick');
      const [view, setView] = React.useState(null);
      const [error, setError] = React.useState('');
      const [revision, refresh] = React.useReducer(n => n + 1, 0);
      const [update, setUpdate] = React.useState(null);
      const [updating, setUpdating] = React.useState(false);
      const [highlighted, setHighlighted] = React.useState(null);
      React.useEffect(() => {
        const abort = new AbortController();
        ctx.connection.rpc.call('/api', 'stsToolUpdates/status', { args: {} }, abort.signal).then(result => {
          if (!abort.signal.aborted) setUpdate(result.ok ? result.value : { reason: 'failed', error: result.error.message });
        }).catch(error => { if (!abort.signal.aborted) setUpdate({ reason: 'failed', error: error.message }); });
        return () => abort.abort();
      }, [revision]);
      async function publishTools() {
        setUpdating(true);
        try {
          const result = await ctx.connection.rpc.call('/api', 'stsToolUpdates/publish', { args: {} });
          setUpdate(result.ok ? result.value : { reason: 'failed', error: result.error.message });
        } catch (error) { setUpdate({ reason: 'failed', error: error.message }); }
        finally { setUpdating(false); }
      }
      React.useEffect(() => {
        const abort = new AbortController();
        setError('');
        ctx.connection.rpc.call('/api', 'stsMapView/read', { args: { sessionId } }, abort.signal).then(result => {
          if (abort.signal.aborted) return;
          if (result.ok) setView({ ...result.value, sessionId }); else setError(result.error.message);
        }).catch(error => { if (!abort.signal.aborted) setError(error.message); });
        return () => abort.abort();
      }, [sessionId, tick, revision]);
      const current = view?.sessionId === sessionId ? view : null;
      return h('section', { style: { height: '100%', overflow: 'auto', padding: 16, boxSizing: 'border-box' } },
        h('header', { style: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 12 } },
          h('div', null,
            h('strong', { style: { fontSize: 15 } }, current?.status === 'ready' ? t('act', { act: current.act }) : t('title')),
            current?.status === 'ready' ? h('p', { style: { margin: '5px 0 0', opacity: 0.6, fontSize: 12 } }, `F${current.floor}  ${current.current[1] < 0 ? t('beforeStart') : t('position', { position: current.current.join(',') })}`) : null),
          h('button', { type: 'button', style: buttonStyle, onClick: refresh }, t('refresh'))),
        error ? h('p', { role: 'alert' }, error) : current?.status === 'ready' ? h(React.Fragment, null,
          h('div', { style: { display: 'flex', flexWrap: 'wrap', alignItems: 'flex-start', gap: 12, margin: '16px 0 12px' } },
            h('figure', { style: { margin: 0, minWidth: 0, maxWidth: '100%', padding: '12px 6px', boxSizing: 'border-box', borderRadius: 9,
              background: 'color-mix(in srgb, currentColor 3%, transparent)', border: '1px solid color-mix(in srgb, currentColor 9%, transparent)' } },
              h('pre', { 'aria-label': t('diagram'), style: { width: 'max-content', maxWidth: '100%', margin: '0 auto', padding: '2px 5px', boxSizing: 'border-box',
                overflowX: 'auto', fontFamily: 'Consolas, "Cascadia Mono", monospace', fontSize: 15, lineHeight: 1.2, whiteSpace: 'pre' } },
                current.lines.flatMap((tokens, row) => [
                  ...tokens.map((token, column) => h('span', { key: `${row}-${column}`, title: mapTokenTitle(token, current.boss, t, gameText),
                    'data-state': token.state, style: tokenStyle(token, highlighted) }, token.text,
                    token.kind === 'room' && token.state === 'current' ? h('span', { 'aria-hidden': true, style: {
                      position: 'absolute', width: 18, height: 18, top: '50%', left: '50%', transform: 'translate(-50%, -50%)',
                      boxSizing: 'border-box', borderRadius: '50%', pointerEvents: 'none',
                      border: '1px solid color-mix(in srgb, currentColor 45%, transparent)',
                    } }) : null)),
                  row === current.lines.length - 1 ? '' : '\n',
                ]))),
            h('aside', { 'aria-label': t('legend'), style: { display: 'flex', flexDirection: 'column', flexShrink: 0, gap: 11,
              minWidth: 66, padding: '12px 8px', boxSizing: 'border-box', borderRadius: 9, fontSize: 12,
              background: 'color-mix(in srgb, currentColor 3%, transparent)', border: '1px solid color-mix(in srgb, currentColor 9%, transparent)' } },
              legend.map(([symbol, room]) => h('button', { key: symbol, type: 'button', 'aria-label': t('highlight', { room: gameText(`room.${room}`) }),
                onMouseEnter: () => setHighlighted(symbol), onMouseLeave: () => setHighlighted(null),
                onFocus: () => setHighlighted(symbol), onBlur: () => setHighlighted(null),
                style: { display: 'flex', alignItems: 'center', gap: 8, margin: 0, padding: 0, border: 0,
                  borderRadius: 3, font: 'inherit', color: 'inherit', textAlign: 'left', cursor: 'default',
                  background: highlighted === symbol ? 'color-mix(in srgb, currentColor 12%, transparent)' : 'transparent' } },
                h('span', { style: { width: 14, textAlign: 'center', flexShrink: 0, fontFamily: 'Consolas, "Cascadia Mono", monospace', fontSize: 15 } }, symbol),
                h('span', null, gameText(`room.${room}`))))),
          ),
          current.extraConnections.length ? h('p', { style: { fontSize: 12 } }, t('extraConnections', { connections: current.extraConnections.join('; ') })) : null,
        ) : h('p', null, t(current?.status === 'not-started' ? 'notStarted' : current?.status === 'unavailable' ? 'unavailable' : 'loading')),
        h('details', { style: { marginTop: 16, paddingTop: 12, borderTop: '1px solid color-mix(in srgb, currentColor 10%, transparent)', fontSize: 12 } },
          h('summary', { style: { cursor: 'pointer', opacity: 0.6 } }, t('updates.title')),
          h('div', { style: { marginTop: 10 } },
            h('button', { type: 'button', style: buttonStyle, disabled: updating, onClick: publishTools }, t(updating ? 'updates.updating' : 'updates.action')),
            h('p', { role: 'status', style: { fontSize: 12, lineHeight: 1.6, opacity: 0.7 } }, t(`updates.${update?.reason ?? 'checking'}`)),
            update?.error ? h('pre', { style: { whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' } }, update.error) : null)));
    }
    ctx.effect(() => ctx.sidebarRightTabs.register({ id: tabId, kind: tabId, title: () => mapText('title'),
      guide: [{ id: tabId, order: 20, title: () => mapText('title'), description: () => mapText('guide') }] }));
    ctx.effect(() => ctx.slots.inject('sidebar.right.pane.tab.title', () => ctx.slots.register({
      name: 'sidebar.right.pane.tab.title', key: tabId, locale: mapNamespace,
    }, ({ t }) => t('title'))));
    ctx.effect(() => ctx.slots.inject('conversation.session.header.actions', () => ctx.slots.register({
      name: 'conversation.session.header.actions', id: tabId, order: 20, locale: mapNamespace,
    }, MapButton)));
    ctx.effect(() => ctx.slots.inject('sidebar.right.pane.tab', () => ctx.slots.register({
      name: 'sidebar.right.pane.tab', key: tabId, locale: mapNamespace,
    }, MapPanel)));
    ctx.effect(() => ctx.sidebarRightTabs.register({ id: observeTabId, kind: observeTabId, title: () => '对局工作台',
      guide: [{ id: observeTabId, order: 21, title: () => '对局工作台', description: () => '查看过程和当前游戏，暂停或返回保留的决策点。' }] }));
    ctx.effect(() => ctx.slots.inject('conversation.session.header.actions', () => ctx.slots.register({
      name: 'conversation.session.header.actions', id: observeTabId, order: 21,
    }, ObserveButton)));
    ctx.effect(() => ctx.slots.inject('sidebar.right.pane.tab', () => ctx.slots.register({
      name: 'sidebar.right.pane.tab', key: observeTabId,
    }, ObservePanel)));
  }
  return { apply, inject };
} });
