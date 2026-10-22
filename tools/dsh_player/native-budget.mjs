// Admission limits belong to the native game owner, not to a polling script.
// Usage is folded by dsh's official meter; a settled-token threshold stops the
// next request and is not a prediction of uncached input or a currency cap.
import { expandAssistantStream, lastAssistantStreamChunk } from '@deepseek-ai/dsh-llm';
import { createPresentation } from '@sts-simulator/presentation';

function isPreResponseProviderError(event) {
  if (event.type !== 'assistant/attempt') return false;
  const chunks = expandAssistantStream(event.data.stream ?? []).map(record => record.chunk);
  if (chunks.length !== 1 || chunks[0].type !== 'finish') return false;
  const reason = chunks[0].reason;
  if (reason.kind !== 'error') return false;
  const { code, status } = reason.failure;
  return code === 'SERVER' && Number.isInteger(status) && status >= 500 && status <= 599
    || code === 'RATE_LIMIT' && status === 429;
}

export function validateNativeBudget(value, locale = 'en') {
  if (value === null) return null;
  const text = (key, parameters) => createPresentation({ locale }).text(`native:budget.${key}`, parameters);
  const keys = ['maxRequests', 'maxReportedTokens', 'maxTokensPerRequest', 'deadlineAt'];
  if (!value || typeof value !== 'object' || Array.isArray(value) || !Object.keys(value).length
      || Object.keys(value).some(key => !keys.includes(key))) throw new Error(text('fields'));
  const budget = { ...value };
  for (const key of keys.slice(0, 3)) if (key in budget && (!Number.isSafeInteger(budget[key]) || budget[key] < 1)) {
    throw new Error(text('positiveInteger', { field: key }));
  }
  if ('deadlineAt' in budget) {
    if (typeof budget.deadlineAt !== 'string' || !/T.*(?:Z|[+-]\d{2}:\d{2})$/.test(budget.deadlineAt)
        || !Number.isFinite(Date.parse(budget.deadlineAt))) throw new Error(text('timestamp'));
    budget.deadlineAt = new Date(budget.deadlineAt).toISOString();
  }
  return budget;
}

export function nativeBudgetRecord(events) {
  for (let i = events.length - 1; i >= 0; i--) {
    const event = events[i];
    if (event.type !== 'command/done' || typeof event.data.text !== 'string') continue;
    let data;
    try { data = JSON.parse(event.data.text); } catch { continue; }
    if (data.schema === 'sts-native-workspace-v1' && data.kind === 'budget-configured') return data.result.budget;
  }
  return null;
}

export function nativeBudgetUsage(events, budget, usageState) {
  let conservativeRequests = 0, latestSettlement, settledAttempts = 0, missingUsageResponses = 0, unmeteredProviderErrors = 0;
  for (const event of events) {
    if (event.seq <= budget.baselineSeq) continue;
    if (event.type === 'assistant/message' || event.type === 'assistant/attempt') {
      latestSettlement = event;
      settledAttempts++;
      const usage = event.data.usage ?? lastAssistantStreamChunk(event.data.stream ?? [], 'usage')?.usage;
      if (!usage) {
        // An explicit HTTP refusal with no generated response is not a reply
        // missing its token meter. Let the official retry owner handle it;
        // admission still counts it, and its unreported usage stays visible.
        if (isPreResponseProviderError(event)) unmeteredProviderErrors++;
        else missingUsageResponses++;
      }
    }
    // A persisted step may have stopped before dispatch. Count it anyway on
    // reattachment, so a crash or cancellation never buys another free request.
    if (event.type === 'step/start' || event.type === 'llm/retry-started') conservativeRequests++;
  }
  const reported = usageState?.totals;
  const reportedTokens = reported ? Math.max(0, reported.uncachedInputTokens + reported.outputTokens - budget.baselineTokens) : null;
  const sample = latestSettlement?.data.usage
    ?? (latestSettlement && lastAssistantStreamChunk(latestSettlement.data.stream ?? [], 'usage')?.usage);
  return { reportedTokens,
    hasUsage: Boolean(sample),
    settledAttempts, missingUsageResponses, unmeteredProviderErrors,
    conservativeRequests };
}

export function nativeBudgetStopReason(budget, admittedRequests, usage, now = Date.now(), locale = 'en') {
  const text = key => createPresentation({ locale }).text(`native:budget.${key}`);
  if (budget.deadlineAt && now >= Date.parse(budget.deadlineAt)) return text('deadline');
  if (budget.maxRequests && admittedRequests >= budget.maxRequests) return text('requests');
  if (budget.maxReportedTokens) {
    if (usage.missingUsageResponses > 0 || admittedRequests > usage.settledAttempts) return text('usageMissing');
    if (usage.reportedTokens >= budget.maxReportedTokens) return text('tokens');
  }
  return null;
}
