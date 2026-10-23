// Recorded intent damage is already a visible per-hit value. This view only
// selects its glyphs and labels; it does not calculate combat effects.
const visuals = {
  attack: { label: 'attack', glyphs: ['swords'], damage: true },
  attack_buff: { label: 'attackBuff', glyphs: ['swords', 'flame'], damage: true },
  attack_debuff: { label: 'attackDebuff', glyphs: ['swords', 'intent-down'], damage: true },
  attack_defend: { label: 'attackDefend', glyphs: ['swords', 'shield'], damage: true },
  buff: { label: 'buff', glyphs: ['flame'] },
  heal: { label: 'heal', glyphs: ['heart'] },
  debuff: { label: 'debuff', glyphs: ['intent-down'] },
  strong_debuff: { label: 'strongDebuff', glyphs: ['intent-down-strong'] },
  debug: { label: 'debug', glyphs: ['intent-unknown'] },
  defend: { label: 'defend', glyphs: ['shield'] },
  defend_debuff: { label: 'defendDebuff', glyphs: ['shield', 'intent-down'] },
  defend_buff: { label: 'defendBuff', glyphs: ['shield', 'flame'] },
  escape: { label: 'escape', glyphs: ['intent-escape'] },
  magic: { label: 'magic', glyphs: ['bolt'] },
  none: { label: 'none', glyphs: ['intent-idle'] },
  sleep: { label: 'sleep', glyphs: ['intent-sleep'] },
  stun: { label: 'stun', glyphs: ['intent-stun'] },
  unknown: { label: 'unknown', glyphs: ['intent-unknown'] },
};
const tones = {
  swords: 'attack', shield: 'defend', flame: 'buff', heart: 'heal',
  'intent-down': 'debuff', 'intent-down-strong': 'debuff',
};

export function intentView(intent, display) {
  const spec = visuals[intent?.kind] ?? visuals.unknown;
  const text = (key, params) => display.text('combat:intent.' + key, params);
  let label = text(intent ? spec.label : 'unavailable');
  let caption = label;
  if (spec.damage) {
    const damage = Number.isFinite(intent.damage) && intent.damage >= 0 ? display.number(intent.damage) : '?';
    // This recording omits the hit count for single attacks.
    const hits = intent.hits ?? 1;
    caption = hits > 1 ? damage + '×' + display.number(hits) : damage;
    label = text(hits > 1 ? 'multi' : 'single', { label, damage, hits });
  } else if (intent?.kind === 'heal' && Number.isFinite(intent.amount)) {
    caption = label = text('healing', { label, amount: intent.amount });
  }
  return {
    label, caption, attack: Boolean(spec.damage),
    glyphs: spec.glyphs.map(id => ({ id, tone: tones[id] ?? 'neutral' })),
  };
}
