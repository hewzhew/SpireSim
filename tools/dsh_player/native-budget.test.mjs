import test from 'node:test';
import assert from 'node:assert/strict';
import { validateNativeBudget, nativeBudgetUsage, nativeBudgetStopReason } from './native-budget.mjs';

test('budget language preserves limits and admission decisions while translating diagnostics', () => {
  const value = { maxRequests: 3, maxReportedTokens: 50, maxTokensPerRequest: 8192, deadlineAt: '2026-10-23T10:00:00+08:00' };
  const before = structuredClone(value);
  const en = validateNativeBudget(value, 'en'), zh = validateNativeBudget(value, 'zh-CN');
  assert.deepEqual(en, zh);
  assert.deepEqual(value, before);
  assert.equal(zh.deadlineAt, '2026-10-23T02:00:00.000Z');
  assert.throws(() => validateNativeBudget({ maxRequests: 0 }, 'zh-CN'), /maxRequests 必须是安全范围内的正整数/);
  assert.throws(() => validateNativeBudget({ deadlineAt: '2026-10-23' }, 'zh-CN'), /带时区/);
  assert.throws(() => validateNativeBudget({ unknown: 1 }, 'zh-CN'), /预算字段/);
  const usage = { missingUsageResponses: 0, settledAttempts: 2, reportedTokens: 40 };
  for (const [budget, requests, sample, now, expected] of [
    [{ deadlineAt: en.deadlineAt }, 0, usage, Date.parse(en.deadlineAt), '已到截止时间'],
    [{ maxRequests: 2 }, 2, usage, 0, '已达到请求次数上限'],
    [{ maxReportedTokens: 10 }, 0, { ...usage, missingUsageResponses: 1 }, 0, '服务商用量不可用'],
    [{ maxReportedTokens: 30 }, 2, usage, 0, '达到阈值'],
    [{ maxRequests: 3 }, 2, usage, 0, null],
  ]) {
    const english = nativeBudgetStopReason(budget, requests, sample, now, 'en');
    const chinese = nativeBudgetStopReason(budget, requests, sample, now, 'zh-CN');
    assert.equal(english === null, chinese === null);
    if (expected === null) assert.equal(chinese, null);
    else { assert.ok(chinese.includes(expected)); assert.notEqual(english, chinese); }
  }
});

const budget = { baselineSeq: -1, baselineTokens: 0, maxReportedTokens: 100, maxRequests: 5 };
const finish = (failure, prefix = []) => ({ seq: 0, type: 'assistant/attempt', data: { stream: [
  ...prefix.map(chunk => ({ type: 'chunk', time: 1, chunk })),
  { type: 'chunk', time: 2, chunk: { type: 'finish', reason: { kind: 'error', failure } } },
] } });
const meter = { totals: { uncachedInputTokens: 0, outputTokens: 0 } };

test('only explicit pre-response HTTP server and rate-limit failures are unmetered retry errors', () => {
  for (const [failure, allowed] of [
    [{ code: 'SERVER', status: 503 }, true], [{ code: 'SERVER', status: 500 }, true],
    [{ code: 'RATE_LIMIT', status: 429 }, true], [{ code: 'SERVER' }, false],
    [{ code: 'SERVER', status: 200 }, false], [{ code: 'TRANSPORT' }, false],
    [{ code: 'EMPTY_RESPONSE' }, false],
  ]) {
    const usage = nativeBudgetUsage([finish(failure)], budget, meter);
    assert.equal(usage.unmeteredProviderErrors, allowed ? 1 : 0);
    assert.equal(usage.hasUsage, false, 'HTTP errors do not pretend to have token usage');
    assert.equal(Boolean(nativeBudgetStopReason(budget, 1, usage)), !allowed);
  }
});

test('tool-call or reasoning output before HTTP failure remains an unmetered response', () => {
  for (const chunk of [
    { type: 'tool-call-delta', index: 0, id: 'call', name: 'sts', argumentsDelta: '{}' },
    { type: 'reasoning-delta', index: 0, text: 'thinking' },
    { type: 'block-start', index: 0, blockType: 'text' },
  ]) {
    const usage = nativeBudgetUsage([finish({ code: 'SERVER', status: 503 }, [chunk])], budget, meter);
    assert.equal(usage.missingUsageResponses, 1);
    assert.match(nativeBudgetStopReason(budget, 1, usage), /usage unavailable/);
  }
});

test('later metered replies do not erase earlier missing usage, and unmatched admissions remain unknown', () => {
  const missing = { seq: 0, type: 'assistant/message', data: {} };
  const reported = { seq: 1, type: 'assistant/message', data: { usage: { inputTokens: 2, outputTokens: 3 } } };
  const usage = nativeBudgetUsage([missing, reported], budget, meter);
  assert.equal(usage.hasUsage, true);
  assert.equal(usage.missingUsageResponses, 1);
  assert.match(nativeBudgetStopReason(budget, 2, usage), /usage unavailable/);
  const interrupted = nativeBudgetUsage([], budget, meter);
  assert.match(nativeBudgetStopReason(budget, 1, interrupted), /usage unavailable/);
});
