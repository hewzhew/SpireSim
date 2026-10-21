// Publish a fresh standing preset through official HMR and Cordis lifecycles.
// Existing Agents keep their retained generation; no game or model is opened.
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve, join, relative, isAbsolute } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { Remote, TypertRemoteService } from '@deepseek-ai/dsh-typert-protocol';

export const inject = ['hmr', 'agentPresets', 'typert'];
const repository = fileURLToPath(new URL('../../', import.meta.url));
const initializers = [];
const publishedState = Symbol.for('@sts-simulator/dsh-player/published-state');
const within = (root, file) => {
  const path = relative(root, file);
  return path === '' || (!path.startsWith('..') && !isAbsolute(path));
};

async function revision(files) {
  const hash = createHash('sha256');
  for (const file of [...files].sort()) {
    hash.update(file).update('\0').update(await readFile(file)).update('\0');
  }
  return hash.digest('hex').slice(0, 12);
}

export class StsToolUpdates extends TypertRemoteService {
  constructor(ctx, { root = repository } = {}) {
    super(ctx, 'stsToolUpdates');
    this.root = root;
    this.sourceRoots = [join(root, 'tools/dsh_player'), join(root, 'tools/run_play')];
    this.roots = ['tools/dsh_player/native.mjs', 'tools/dsh_player/map-view.mjs']
      .map(path => pathToFileURL(join(root, path)).href);
    this.files = new Set();
    this.loading = this.initialize().catch(error => { this.unavailable = error.message; });
    const processed = () => {
      // The event is emitted inside HMR's transaction. Only record completed
      // source processing here; publishing is a separate Remote transaction.
      this.loading = this.loading.then(async () => {
        try { this.loaded = await revision(this.files); this.failure = null; }
        catch (error) { this.failure = error.message; }
      });
    };
    ctx.on('hmr/reload', processed);
    ctx.on('hmr/change', url => {
      // A failed dynamic import has no cached ModuleJob. Its repaired file is
      // reported as a change rather than a module replacement.
      if (url.startsWith('file:') && this.files.has(fileURLToPath(url))) processed();
    });
    for (const initialize of initializers) initialize.call(this);
  }

  watched() {
    const hmr = this.ctx.hmr;
    const watched = hmr.config.root.map(root => resolve(hmr.baseDir, root));
    // Roots outside the watch base introduce '..' path segments, which the
    // official default hidden-file filter excludes even for absolute roots.
    return this.sourceRoots.every(root => within(hmr.baseDir, root) && watched.some(watch => within(watch, root)));
  }

  async sources() {
    const seen = new Set(), files = new Set(), pending = [...this.roots];
    for (const url of this.roots) await this.ctx.loader.import(url);
    while (pending.length) {
      const url = pending.pop();
      if (seen.has(url)) continue;
      seen.add(url);
      if (!url.startsWith('file:')) continue;
      const file = fileURLToPath(url);
      if (!this.sourceRoots.some(root => within(root, file)) || !file.endsWith('.mjs')) continue;
      files.add(file);
      pending.push(...await this.ctx.hmr.getLinked(url));
    }
    if (!files.has(join(this.root, 'tools/run_play/workbench.mjs'))) {
      throw new Error('原生预设的静态模块树没有包含共享工作台，不能发布这次工具更新。');
    }
    return files;
  }

  async initialize() {
    if (!this.watched()) { this.unavailable = '未启用本仓库的源码热重载。请由维护者更新本地服务配置。'; return; }
    this.packageFile = join(this.root, 'tools/dsh_player/package.json');
    const packageRevision = await revision([this.packageFile]);
    this.files = await this.sources();
    this.loaded = await revision(this.files);
    // The declaring Loader entry survives a UI/service module replacement.
    // Keep the publication fact on that owner, rather than assuming that a
    // recreated Remote service also recreated the standing preset.
    this.entry()[publishedState] ??= { revision: this.loaded, packageRevision };
  }

  entry() {
    const entries = [...this.ctx.loader.entries()].filter(entry => entry.options.id === 'preset-sts-a20h');
    if (entries.length !== 1 || !entries[0].fiber) throw new Error('没有唯一的杀戮尖塔预设声明。');
    return entries[0];
  }

  async status() {
    await this.loading;
    if (this.unavailable) return { status: 'unavailable', message: this.unavailable };
    const published = this.entry()[publishedState];
    if (await revision([this.packageFile]) !== published.packageRevision) {
      return { status: 'restart-required', message: '插件声明发生了变化，需要重启服务加载。' };
    }
    let current;
    try { current = await revision(this.files); }
    catch { return { status: 'restart-required', message: '后端源码文件已移走或无法读取，需要维护者检查后重启。' }; }
    if (current !== this.loaded) return { status: 'loading', message: '官方热重载尚未完成，请稍后再试。' };
    if (this.failure) return { status: 'failed', message: `工具更新失败：${this.failure}` };
    return { status: current === published.revision ? 'ready' : 'available', revision: published.revision,
      message: current === published.revision ? '当前工具已就绪。新建聊天即可试跑。' : '检测到工具更新。应用后，新建聊天会使用新版工具。' };
  }

  async publish() {
    return this.ctx.hmr.runExclusive(async () => {
      const state = await this.status();
      if (state.status !== 'available') return state;
      const expected = this.loaded;
      try {
        // Import before retiring the declaration so invalid dynamic code leaves
        // the previous preset available. HMR owns cache invalidation.
        const files = await this.sources();
        const next = await revision(files);
        if (await revision(this.files) !== expected) return { status: 'loading', message: '源码仍在变化，请等待热重载完成。' };
        const entry = this.entry();
        await entry.fiber.restart();
        await this.ctx.loader.await();
        const preset = await this.ctx.agentPresets.resolve('sts-a20h');
        if (preset.broken) throw new Error(preset.broken);
        if (await revision(files) !== next) return { status: 'loading', message: '发布期间源码又有变化，请等待下一次热重载。' };
        this.files = files;
        this.loaded = next;
        entry[publishedState] = { ...entry[publishedState], revision: next };
        this.failure = null;
        return { status: 'ready', revision: next, message: '新版工具已就绪。请新建聊天试跑；现有聊天不自动切换预设。' };
      } catch (error) {
        this.failure = error.message;
        return { status: 'failed', message: `工具更新失败：${error.message}` };
      }
    });
  }
}

for (const name of ['status', 'publish']) Remote(StsToolUpdates.prototype[name], {
  kind: 'method', name, static: false, private: false, addInitializer: initialize => initializers.push(initialize),
});

export function apply(ctx) { new StsToolUpdates(ctx); }
