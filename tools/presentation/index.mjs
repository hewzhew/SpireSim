import i18next from 'i18next';
import { catalogs, resolveLocale, supportedLocales } from './catalogs.mjs';
export { catalogs, resolveLocale, supportedLocales, preferredLocale, validateCatalogs } from './catalogs.mjs';

/** Each view owns an instance. No game, model, browser storage or global locale. */
export async function createPresentation({ locale = 'en', resources = catalogs, onMissing = () => {} } = {}) {
  const instance = i18next.createInstance();
  const reported = new Set();
  await instance.init({
    lng: resolveLocale(locale), supportedLngs: supportedLocales, fallbackLng: 'en',
    resources, defaultNS: 'replay', initAsync: false,
    interpolation: { escapeValue: false },
    missingInterpolationHandler(_text, match) { throw new Error(`Missing translation parameter: ${match[1]}`); },
  });
  function entity(kind, id) {
    const key = `${kind}.${id}`;
    if (typeof instance.getResource(instance.language, 'game', key) !== 'string') {
      const gap = `${instance.language}:${key}`;
      if (!reported.has(gap)) {
        reported.add(gap);
        onMissing({ locale: instance.language, kind, id, fallback: typeof instance.getResource('en', 'game', key) === 'string' ? 'en' : 'id' });
      }
    }
    return instance.t(`game:${key}`, { defaultValue: String(id) });
  }
  return Object.freeze({
    get locale() { return instance.language; },
    async setLocale(tag) { await instance.changeLanguage(resolveLocale(tag)); },
    text(key, params = {}) {
      if (!instance.exists(key, params)) throw new Error(`Unknown presentation message: ${key}`);
      return instance.t(key, { ...params, lng: instance.language });
    },
    entity,
    cardName(id, upgrades = 0) {
      const suffix = upgrades === 0 ? '' : upgrades === 1 && id !== 'SearingBlow' ? '+' : `+${upgrades}`;
      return entity('card', id) + suffix;
    },
    number(value) { return new Intl.NumberFormat(instance.language).format(value); },
  });
}
