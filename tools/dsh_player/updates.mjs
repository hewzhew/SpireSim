// Publish a fresh standing preset through official HMR and Cordis lifecycles.
// Existing Agents keep their retained generation; no game or model is opened.
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve, join, relative, isAbsolute } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { Remote, TypertRemoteService } from '@deepseek-ai/dsh-typert-protocol';
import { createPresentation } from '@sts-simulator/presentation';

export const inject = ['hmr', 'agentPresets', 'typert'];
const repository = fileURLToPath(new URL('../../', import.meta.url));
const initializers = [];
const publishedState = Symbol.for('@sts-simulator/dsh-player/published-state');
class ToolUpdateError extends Error {
  constructor(key, locale = 'en') {
    super(createPresentation({ locale }).text(`native:updates.${key}`));
    this.messageKey = key;
  }
}
const errorMessage = (error, locale) => error instanceof ToolUpdateError
  ? createPresentation({ locale }).text(`native:updates.${error.messageKey}`) : error.message;
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
    this.loading = this.initialize().catch(error => { this.unavailable = { reason: 'loadFailed', error }; });
    const processed = () => {
      // The event is emitted inside HMR's transaction. Only record completed
      // source processing here; publishing is a separate Remote transaction.
      this.loading = this.loading.then(async () => {
        try { this.loaded = await revision(this.files); this.failure = null; }
        catch (error) { this.failure = error; }
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

  async sources(locale = 'en') {
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
      throw new ToolUpdateError('missingWorkbench', locale);
    }
    return files;
  }

  async initialize() {
    if (!this.watched()) { this.unavailable = { reason: 'watchDisabled' }; return; }
    this.declarations = ['package.json', 'cordis.patch.yml'].map(file => join(this.root, 'tools/dsh_player', file));
    const packageRevision = await revision(this.declarations);
    this.files = await this.sources();
    this.loaded = await revision(this.files);
    // The declaring Loader entry survives a UI/service module replacement.
    // Keep the publication fact on that owner, rather than assuming that a
    // recreated Remote service also recreated the standing preset.
    this.entry()[publishedState] ??= { revision: this.loaded, packageRevision };
  }

  entry(locale = 'en') {
    const entries = [...this.ctx.loader.entries()].filter(entry => entry.options.id === 'preset-sts-a20h');
    if (entries.length !== 1 || !entries[0].fiber) throw new ToolUpdateError('presetNotUnique', locale);
    return entries[0];
  }

  async status(locale) {
    locale ??= 'en';
    await this.loading;
    if (this.unavailable) return { status: 'unavailable', ...this.unavailable,
      ...(this.unavailable.error ? { error: errorMessage(this.unavailable.error, locale) } : {}) };
    const published = this.entry(locale)[publishedState];
    if (await revision(this.declarations) !== published.packageRevision) {
      return { status: 'restart-required', reason: 'declarationsChanged' };
    }
    let current;
    try { current = await revision(this.files); }
    catch { return { status: 'restart-required', reason: 'sourceUnreadable' }; }
    if (current !== this.loaded) return { status: 'loading', reason: 'loading' };
    if (this.failure?.message) return { status: 'failed', reason: 'failed', error: errorMessage(this.failure, locale) };
    return { status: current === published.revision ? 'ready' : 'available', revision: published.revision,
      reason: current === published.revision ? 'ready' : 'available' };
  }

  async publish(locale) {
    locale ??= 'en';
    return this.ctx.hmr.runExclusive(async () => {
      const state = await this.status(locale);
      if (state.status !== 'available') return state;
      const expected = this.loaded;
      try {
        // Import before retiring the declaration so invalid dynamic code leaves
        // the previous preset available. HMR owns cache invalidation.
        const files = await this.sources(locale);
        const next = await revision(files);
        if (await revision(this.files) !== expected) return { status: 'loading', reason: 'sourceChanging' };
        const entry = this.entry(locale);
        await entry.fiber.restart();
        await this.ctx.loader.await();
        for (const id of ['sts-a20h-manual', 'sts-a20h']) {
          const preset = await this.ctx.agentPresets.resolve(id);
          if (preset.broken) throw new Error(preset.broken);
        }
        if (await revision(files) !== next) return { status: 'loading', reason: 'changedDuringPublish' };
        this.files = files;
        this.loaded = next;
        entry[publishedState] = { ...entry[publishedState], revision: next };
        this.failure = null;
        return { status: 'ready', revision: next, reason: 'published' };
      } catch (error) {
        this.failure = error;
        return { status: 'failed', reason: 'failed', error: errorMessage(error, locale) };
      }
    });
  }
}

for (const name of ['status', 'publish']) Remote(StsToolUpdates.prototype[name], {
  kind: 'method', name, static: false, private: false, addInitializer: initialize => initializers.push(initialize),
});

export function apply(ctx) { new StsToolUpdates(ctx); }
