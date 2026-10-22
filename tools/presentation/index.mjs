import i18next from 'i18next';
import { catalogs, resolveLocale, supportedLocales } from './catalogs.mjs';
export { catalogs, resolveLocale, supportedLocales, preferredLocale, validateCatalogs } from './catalogs.mjs';

/** Local resources initialize synchronously. Each view owns its language. */
export function createPresentation({ locale = 'en', resources = catalogs, onMissing = () => {} } = {}) {
  const instance = i18next.createInstance();
  const reported = new Set();
  instance.init({
    lng: resolveLocale(locale), supportedLngs: supportedLocales, fallbackLng: 'en',
    resources, defaultNS: 'replay', initAsync: false,
    interpolation: { escapeValue: false },
    missingInterpolationHandler(_text, match) { throw new Error(`Missing translation parameter: ${match[1]}`); },
  });
  if (!instance.isInitialized) throw new Error('Local presentation resources did not initialize synchronously');
  function entity(kind, id) {
    const key = `${kind}.${id}`;
    const translated = instance.getResource(instance.language, 'game', key);
    if (typeof translated !== 'string') {
      const gap = `${instance.language}:${key}`;
      if (!reported.has(gap)) {
        reported.add(gap);
        onMissing({ locale: instance.language, kind, id, fallback: typeof instance.getResource('en', 'game', key) === 'string' ? 'en' : 'id' });
      }
    }
    const fallback = instance.getResource('en', 'game', key);
    return typeof translated === 'string' ? translated : typeof fallback === 'string' ? fallback : String(id);
  }
  return Object.freeze({
    get locale() { return instance.language; },
    async setLocale(tag) { await instance.changeLanguage(resolveLocale(tag)); },
    text(key, params = {}) {
      if (!instance.exists(key, params)) throw new Error(`Unknown presentation message: ${key}`);
      return instance.t(key, { ...params, lng: instance.language });
    },
    definitionText(key) {
      const [namespace,path,...extra] = key.split(':');
      if (!path || extra.length) throw new Error('Definition text requires a namespaced key');
      const template = instance.getResource(instance.language,namespace,path) ?? instance.getResource('en',namespace,path);
      if (typeof template !== 'string') throw new Error(`Unknown presentation message: ${key}`);
      let result = '', remaining = template;
      for (let start; (start = remaining.indexOf('{{')) !== -1;) {
        result += remaining.slice(0,start);
        const tail = remaining.slice(start+2), end = tail.indexOf('}}');
        if (end === -1) throw new Error(`Invalid definition template: ${key}`);
        const name = tail.slice(0,end).trim();
        const reference = /^entity_(card|relic|potion|enemy|power|stance|event|character|encounter)_(\w+)$/.exec(name);
        if (!reference) throw new Error(`Invalid definition reference: ${key}:${name}`);
        const [,kind,id] = reference;
        if (!instance.exists(`game:${kind}.${id}`)) throw new Error(`Unknown definition entity: ${kind}:${id}`);
        result += entity(kind,id);
        remaining = tail.slice(end+2);
      }
      return result + remaining;
    },
    entity,
    cardName(id, upgrades = 0) {
      const suffix = upgrades === 0 ? '' : upgrades === 1 && id !== 'SearingBlow' ? '+' : `+${upgrades}`;
      return entity('card', id) + suffix;
    },
    number(value) { return new Intl.NumberFormat(instance.language).format(value); },
  });
}
