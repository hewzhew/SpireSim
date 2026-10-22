import en from './locales/en.json' with { type: 'json' };
import zhCN from './locales/zh-CN.json' with { type: 'json' };

export const catalogs = { en, 'zh-CN': zhCN };
export const supportedLocales = Object.freeze(['en', 'zh-CN']);

function supportedLanguage(tag) {
  if (typeof tag !== 'string') return null;
  let canonical;
  try { [canonical] = Intl.getCanonicalLocales(tag); } catch { return null; }
  if (canonical === 'en' || canonical.startsWith('en-')) return 'en';
  if (canonical === 'zh' || canonical === 'zh-CN' || canonical === 'zh-SG' || canonical === 'zh-Hans' || canonical.startsWith('zh-Hans-')) return 'zh-CN';
  return null;
}

/** The dsh boundary may supply zh; our supported content is Simplified Chinese. */
export function resolveLocale(tag) {
  return supportedLanguage(tag) ?? 'en';
}

export function preferredLocale(languages) {
  for (const tag of languages ?? []) {
    const locale = supportedLanguage(tag);
    if (locale) return locale;
  }
  return 'en';
}

function flatten(value, prefix = '', result = {}) {
  for (const [key, child] of Object.entries(value)) {
    const name = prefix ? `${prefix}.${key}` : key;
    if (typeof child === 'string') {
      if (!child.trim()) throw new Error(`Empty translation: ${name}`);
      result[name] = child;
    } else if (child && typeof child === 'object' && !Array.isArray(child)) {
      flatten(child, name, result);
    } else throw new Error(`Translation must be text: ${name}`);
  }
  return result;
}

const parameters = text => [...new Set([...text.matchAll(/{{\s*(\w+)\s*}}/g)].map(match => match[1]))].sort();

/** Build-time check; fallback remains available for external or incomplete content. */
export function validateCatalogs(resources = catalogs) {
  if (!resources.en) throw new Error('Missing locale: en');
  const baseline = flatten(resources.en);
  const keys = Object.keys(baseline).sort();
  for (const locale of supportedLocales) {
    if (!resources[locale]) throw new Error(`Missing locale: ${locale}`);
    const messages = flatten(resources[locale]);
    const missing = keys.filter(key => !(key in messages));
    const extra = Object.keys(messages).filter(key => !(key in baseline));
    if (missing.length || extra.length) throw new Error(`Translation keys differ for ${locale}: missing [${missing}], extra [${extra}]`);
    for (const key of keys) {
      if (JSON.stringify(parameters(baseline[key])) !== JSON.stringify(parameters(messages[key]))) {
        throw new Error(`Translation parameters differ for ${locale}:${key}`);
      }
      for (const parameter of parameters(messages[key])) {
        if (!parameter.startsWith('entity_')) continue;
        const reference = /^entity_(card|relic|potion|enemy|enemyMove|power|stance|event|character|encounter)_(\w+)$/.exec(parameter);
        if (!reference || typeof resources[locale].game?.[reference[1]]?.[reference[2]] !== 'string') {
          throw new Error(`Unknown definition entity for ${locale}:${key}:${parameter}`);
        }
      }
    }
  }
  return true;
}
