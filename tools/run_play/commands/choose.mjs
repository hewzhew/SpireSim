// The default gesture: choose what you see — a row number (0), a
// bare name (Disarm), or verb+name (take Disarm), resolved through the
// decision's semantic actions.
import { parseChoiceToken } from '../tokens.mjs';
import { isRewardReading } from '../contract.mjs';

export async function choose(ctx, command, parameters) {
  const { game, printCurrentReading, resolveOptionByName } = ctx;
  const tokens = command === undefined ? parameters : [command, ...parameters];
  if (tokens.length === 0) {
    await printCurrentReading(await game.decision({ details: ['brief'] }));
    return;
  }
  let reading = await game.decision({ details: ['options'] });
  if (command === 'choose' || command === 'select') {
    throw new Error(`${command} is not a standalone command; copy the complete displayed command.\n${await ctx.commandHint(reading)}`);
  }
  const nameQuery = tokens.filter(t => t !== '').filter(t => !/^\d+$/.test(t)).join(' ');
  let choices;
  if (nameQuery) {
    const index = await resolveOptionByName(nameQuery, reading);
    choices = [{ index }];
  } else {
    if (isRewardReading(reading)) {
      throw new Error('reward rows can move after a claim; numeric choices are not accepted here. Use the named command printed on the page (./p options), or ./p loot for routine rewards. No action taken.');
    }
    choices = tokens.filter(t => t !== '').map(value => parseChoiceToken(value));
    if (choices.length > 1 && !reading.selections?.length) {
      throw new Error('multiple row numbers are only valid within one card selection; choose one current option at a time. No action taken.');
    }
  }
  while (choices.length > 0) {
    // An active selection owns the row numbers: the reading's option
    // list at the same time is usually just a Cancel escape hatch, and
    // letting it win the index race turned more than one selection
    // into an accidental cancel. Cancel stays reachable by name.
    if (reading.selections?.length) {
      const selection = reading.selections[0];
      const members = choices
        .slice(0, selection.max)
        .map(choice => selection.members[choice.index])
        .filter(member => member !== undefined);
      if (members.length < selection.min || members.length !== choices.length) {
        throw new Error(`selection expects ${selection.min}..${selection.max} members; got ${choices.map(choice => choice.index).join(' ')}`);
      }
      reading = await selection.select(members);
      choices = [];
      const picked = members.map(m => m.label).join(', ');
      ctx.recordAction?.(`select ${picked}`);
      await (ctx.showSuccessor ?? printCurrentReading)(reading);
      continue;
    }
    if (reading.options?.length) {
      const [choice, ...rest] = choices;
      const index = choice.index;
      if (index >= reading.options.length) {
        throw new Error(`option ${index} is not on the current reading (0..${reading.options.length - 1})`);
      }
      const label = reading.options[index].label ?? '';
      reading = await reading.options[index].choose();
      choices = rest;
      ctx.recordAction?.(label);
      await (ctx.showSuccessor ?? printCurrentReading)(reading);
      continue;
    } else {
      throw new Error('current reading has no options or selections to act on');
    }
  }
}
