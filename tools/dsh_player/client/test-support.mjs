import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';

// Exercise the published host implementation. Its React settings UI is unused
// here; the LocaleRuntime registry itself has no React or transport dependency.
let hostModule;
export async function createHostLocale(active = 'zh') {
  if (!hostModule) {
    let definition;
    runInNewContext(await readFile(new URL(import.meta.resolve('@deepseek-ai/dsh-client-locale/client')), 'utf8'), {
      window: { __ModuleLoader__: { load: value => { definition = value; } } },
    });
    hostModule = definition.factory(() => ({}));
  }
  return new hostModule.LocaleRuntime({ emit() {} }, undefined, { preference: active, languages: [active] });
}
