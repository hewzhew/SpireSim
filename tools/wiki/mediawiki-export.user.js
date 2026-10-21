// ==UserScript==
// @name         STS MediaWiki corpus exporter
// @namespace    https://github.com/sts-simulator-local-research
// @version      0.1.0
// @description  Resumable, rate-limited MediaWiki API export for the local STS research corpus.
// @match        https://slaythespire.wiki.gg/*
// @match        https://sts.huijiwiki.com/*
// @match        https://slay-the-spire.fandom.com/*
// @grant        none
// @run-at       document-idle
// ==/UserScript==

(() => {
  "use strict";

  const BUNDLE_SCHEMA = "mediawiki-text-export-v1";
  const DATABASE_NAME = "sts-mediawiki-export-v1";
  const DATABASE_VERSION = 1;
  const PAGE_STORE = "pages";
  const STATE_STORE = "states";
  const DEFAULT_DELAY_MS = 700;
  const REQUEST_TIMEOUT_MS = 30000;
  const REVISION_BATCH_SIZE = 20;
  const MAX_RETRIES = 7;
  const SOURCE_BY_HOST = {
    "slaythespire.wiki.gg": "slaythespire-wikigg-en",
    "sts.huijiwiki.com": "slay-the-spire-huiji-zh",
    "slay-the-spire.fandom.com": "slay-the-spire-fandom-en",
  };

  let stopRequested = false;
  let crawlRunning = false;
  let lastRequestAt = 0;
  let currentActivity = "";

  const sleep = (milliseconds) =>
    new Promise((resolve) => window.setTimeout(resolve, milliseconds));

  function utcNow() {
    return new Date().toISOString();
  }

  function sourceIdForPage() {
    return SOURCE_BY_HOST[location.hostname] || `mediawiki-${location.hostname}`;
  }

  function apiUrlForPage() {
    return `${location.origin}/api.php`;
  }

  function openDatabase() {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open(DATABASE_NAME, DATABASE_VERSION);
      request.onupgradeneeded = () => {
        const database = request.result;
        if (!database.objectStoreNames.contains(PAGE_STORE)) {
          const pages = database.createObjectStore(PAGE_STORE, { keyPath: "key" });
          pages.createIndex("by_source", "sourceId", { unique: false });
        }
        if (!database.objectStoreNames.contains(STATE_STORE)) {
          database.createObjectStore(STATE_STORE, { keyPath: "sourceId" });
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  }

  async function withStore(storeName, mode, operation) {
    const database = await openDatabase();
    try {
      return await new Promise((resolve, reject) => {
        const transaction = database.transaction(storeName, mode);
        const store = transaction.objectStore(storeName);
        let result;
        try {
          result = operation(store, transaction);
        } catch (error) {
          reject(error);
          return;
        }
        transaction.oncomplete = () => resolve(result);
        transaction.onerror = () => reject(transaction.error);
        transaction.onabort = () => reject(transaction.error || new Error("IndexedDB transaction aborted"));
      });
    } finally {
      database.close();
    }
  }

  async function getState(sourceId) {
    const database = await openDatabase();
    try {
      return await new Promise((resolve, reject) => {
        const request = database
          .transaction(STATE_STORE, "readonly")
          .objectStore(STATE_STORE)
          .get(sourceId);
        request.onsuccess = () => resolve(request.result || null);
        request.onerror = () => reject(request.error);
      });
    } finally {
      database.close();
    }
  }

  async function putState(state) {
    state.updatedAt = utcNow();
    await withStore(STATE_STORE, "readwrite", (store) => store.put(state));
  }

  async function clearSource(sourceId) {
    const database = await openDatabase();
    try {
      await new Promise((resolve, reject) => {
        const transaction = database.transaction([PAGE_STORE, STATE_STORE], "readwrite");
        const pages = transaction.objectStore(PAGE_STORE).index("by_source");
        const cursor = pages.openKeyCursor(IDBKeyRange.only(sourceId));
        cursor.onsuccess = () => {
          const value = cursor.result;
          if (!value) return;
          transaction.objectStore(PAGE_STORE).delete(value.primaryKey);
          value.continue();
        };
        transaction.objectStore(STATE_STORE).delete(sourceId);
        transaction.oncomplete = resolve;
        transaction.onerror = () => reject(transaction.error);
        transaction.onabort = () => reject(transaction.error || new Error("IndexedDB clear aborted"));
      });
    } finally {
      database.close();
    }
  }

  async function storePages(sourceId, pages) {
    await withStore(PAGE_STORE, "readwrite", (store) => {
      for (const page of pages) {
        if (!Number.isInteger(page.pageid) || !Number.isInteger(page.ns)) {
          throw new Error("MediaWiki response contained a page without integer pageid/ns");
        }
        store.put({ key: `${sourceId}:${page.pageid}`, sourceId, page });
      }
    });
  }

  async function readPages(sourceId) {
    const database = await openDatabase();
    try {
      return await new Promise((resolve, reject) => {
        const pages = [];
        const request = database
          .transaction(PAGE_STORE, "readonly")
          .objectStore(PAGE_STORE)
          .index("by_source")
          .openCursor(IDBKeyRange.only(sourceId));
        request.onsuccess = () => {
          const cursor = request.result;
          if (!cursor) {
            resolve(pages);
            return;
          }
          pages.push(cursor.value.page);
          cursor.continue();
        };
        request.onerror = () => reject(request.error);
      });
    } finally {
      database.close();
    }
  }

  async function rateLimit(delayMs) {
    const remaining = delayMs - (performance.now() - lastRequestAt);
    if (remaining > 0) await sleep(remaining);
  }

  async function apiRequest(apiUrl, parameters, delayMs) {
    const query = new URLSearchParams({
      format: "json",
      formatversion: "2",
      utf8: "1",
      maxlag: "5",
      ...Object.fromEntries(
        Object.entries(parameters).map(([key, value]) => [key, String(value)])
      ),
    });
    const url = `${apiUrl}?${query}`;
    let lastError = null;
    for (let attempt = 0; attempt <= MAX_RETRIES; attempt += 1) {
      await rateLimit(delayMs);
      const controller = new AbortController();
      const timeout = window.setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
      try {
        lastRequestAt = performance.now();
        const response = await fetch(url, {
          credentials: "include",
          headers: { Accept: "application/json" },
          signal: controller.signal,
        });
        if (!response.ok) {
          const error = new Error(`HTTP ${response.status} from MediaWiki API`);
          error.status = response.status;
          error.retryAfter = response.headers.get("Retry-After");
          throw error;
        }
        const payload = await response.json();
        if (payload.error) {
          if (payload.error.code === "maxlag" && attempt < MAX_RETRIES) {
            await sleep(Math.min(30000, 1000 * 2 ** attempt));
            continue;
          }
          const error = new Error(
            `MediaWiki API ${payload.error.code}: ${payload.error.info || "unknown error"}`
          );
          error.apiError = true;
          error.apiCode = payload.error.code;
          throw error;
        }
        return payload;
      } catch (error) {
        if (error?.name === "AbortError") {
          lastError = new Error(`MediaWiki request exceeded ${REQUEST_TIMEOUT_MS}ms`);
          lastError.requestTimedOut = true;
          break;
        }
        lastError = error;
        const retryable =
          !error.apiError &&
          (!error.status || [408, 425, 429, 500, 502, 503, 504].includes(error.status));
        if (!retryable || attempt >= MAX_RETRIES) break;
        const retryAfter = Number(error.retryAfter);
        await sleep(
          Number.isFinite(retryAfter) && retryAfter > 0
            ? retryAfter * 1000
            : Math.min(60000, 1000 * 2 ** attempt)
        );
      } finally {
        window.clearTimeout(timeout);
      }
    }
    const failure = new Error(`MediaWiki request failed after retries: ${lastError}`);
    failure.requestTimedOut = Boolean(lastError?.requestTimedOut);
    failure.apiError = Boolean(lastError?.apiError);
    failure.apiCode = lastError?.apiCode || null;
    throw failure;
  }

  function publicNamespaceIds(siteinfo) {
    const namespaces = Array.isArray(siteinfo.namespaces)
      ? siteinfo.namespaces
      : Object.values(siteinfo.namespaces || {});
    return [...new Set(namespaces.map((entry) => entry.id).filter((id) => Number.isInteger(id) && id >= 0))].sort(
      (left, right) => left - right
    );
  }

  async function createFreshState(sourceId, delayMs) {
    const apiUrl = apiUrlForPage();
    const response = await apiRequest(
      apiUrl,
      {
        action: "query",
        meta: "siteinfo",
        siprop: "general|namespaces|namespacealiases|statistics|rightsinfo",
      },
      delayMs
    );
    if (!response.query) throw new Error("siteinfo response lacks query object");
    await clearSource(sourceId);
    const state = {
      schema: BUNDLE_SCHEMA,
      sourceId,
      apiUrl,
      siteinfo: response.query,
      namespaceIds: publicNamespaceIds(response.query),
      namespacePosition: 0,
      continuation: {},
      batchesCompleted: 0,
      status: "paused",
      delayMs,
      startedAt: utcNow(),
      updatedAt: utcNow(),
      lastError: null,
    };
    await putState(state);
    return state;
  }

  function derivedPageUrl(title) {
    const encoded = encodeURIComponent(title.replaceAll(" ", "_")).replaceAll("%2F", "/");
    return `${location.origin}/wiki/${encoded}`;
  }

  function acquisitionIssue(stage, error) {
    return {
      stage,
      code: error.apiCode || (error.requestTimedOut ? "request_timeout" : "request_failed"),
      message: String(error.message || error),
    };
  }

  function isPageSpecificApiFailure(error) {
    return error.apiError && String(error.apiCode || "").startsWith("internal_api_error_");
  }

  async function pageInfoForStubs(state, stubs) {
    if (!stubs.length) return [];
    currentActivity = `读取 ${stubs.length} 个页面元数据`;
    renderStatus(state);
    try {
      const response = await apiRequest(
        state.apiUrl,
        {
          action: "query",
          pageids: stubs.map((page) => page.pageid).join("|"),
          prop: "info",
          inprop: "url",
        },
        state.delayMs
      );
      const infoPages = response.query?.pages || [];
      if (!Array.isArray(infoPages)) {
        throw new Error("page info response query.pages was not an array");
      }
      const infoByPage = new Map(infoPages.map((page) => [page.pageid, page]));
      return stubs.map((stub) => {
        const info = infoByPage.get(stub.pageid);
        if (!info) throw new Error(`page info response omitted pageid ${stub.pageid}`);
        return { ...stub, ...info };
      });
    } catch (error) {
      if (!isPageSpecificApiFailure(error)) throw error;
      if (stubs.length > 1) {
        const midpoint = Math.ceil(stubs.length / 2);
        return [
          ...(await pageInfoForStubs(state, stubs.slice(0, midpoint))),
          ...(await pageInfoForStubs(state, stubs.slice(midpoint))),
        ];
      }
      const stub = stubs[0];
      return [
        {
          ...stub,
          fullurl: derivedPageUrl(stub.title),
          acquisition_warnings: [acquisitionIssue("page_info", error)],
        },
      ];
    }
  }

  async function latestRevisionForPage(state, page) {
    currentActivity = `按 page ID 读取 ${page.pageid} 的当前 revision`;
    renderStatus(state);
    try {
      const response = await apiRequest(
        state.apiUrl,
        {
          action: "query",
          pageids: page.pageid,
          prop: "revisions",
          rvlimit: 1,
          rvdir: "older",
          rvslots: "main",
          rvprop: "ids|timestamp|sha1|size|content|contentmodel",
        },
        state.delayMs
      );
      const revisionPages = response.query?.pages || [];
      if (!Array.isArray(revisionPages) || revisionPages.length !== 1) {
        throw new Error(`pageid ${page.pageid} current revision response was not singular`);
      }
      const revisionPage = revisionPages[0];
      const revisions = revisionPage.revisions || [];
      if (!Array.isArray(revisions) || revisions.length !== 1) {
        throw new Error(`pageid ${page.pageid} current revision was not singular`);
      }
      return {
        ...page,
        ...revisionPage,
        lastrevid: revisions[0].revid,
        revisions,
      };
    } catch (error) {
      if (!isPageSpecificApiFailure(error)) throw error;
      return {
        ...page,
        acquisition_errors: [
          ...(page.acquisition_errors || []),
          acquisitionIssue("current_revision", error),
        ],
      };
    }
  }

  async function revisionPagesForEntries(state, entries) {
    const revisionIds = entries.map((page) => page.lastrevid);
    currentActivity = `读取 ${revisionIds.length} 个明确 revision`;
    renderStatus(state);
    try {
      const response = await apiRequest(
        state.apiUrl,
        {
          action: "query",
          revids: revisionIds.join("|"),
          prop: "revisions",
          rvslots: "main",
          rvprop: "ids|timestamp|sha1|size|content|contentmodel",
        },
        state.delayMs
      );
      if (response.continue) {
        throw new Error("explicit revid lookup unexpectedly returned continuation");
      }
      const revisionPages = response.query?.pages || [];
      if (!Array.isArray(revisionPages)) {
        throw new Error("explicit revid response query.pages was not an array");
      }
      return revisionPages;
    } catch (error) {
      const pageSpecificFailure = isPageSpecificApiFailure(error);
      if (!(error.requestTimedOut || pageSpecificFailure) || entries.length <= 1) {
        if (!pageSpecificFailure) throw error;
        return [
          {
            pageid: entries[0].pageid,
            revisions: [],
            acquisition_errors: [acquisitionIssue("explicit_revision", error)],
          },
        ];
      }
      const midpoint = Math.ceil(entries.length / 2);
      return [
        ...(await revisionPagesForEntries(state, entries.slice(0, midpoint))),
        ...(await revisionPagesForEntries(state, entries.slice(midpoint))),
      ];
    }
  }

  async function attachLatestRevisions(state, pages) {
    const enrichedPages = [];
    for (const page of pages) {
      enrichedPages.push(
        Number.isInteger(page.lastrevid) && page.lastrevid > 0
          ? page
          : await latestRevisionForPage(state, page)
      );
    }
    const revisionsByPage = new Map();
    const errorsByPage = new Map();
    for (const page of enrichedPages) {
      if (Array.isArray(page.revisions) && page.revisions.length) {
        revisionsByPage.set(page.pageid, page.revisions);
      }
    }
    const pendingPages = enrichedPages.filter(
      (page) =>
        Number.isInteger(page.lastrevid) &&
        page.lastrevid > 0 &&
        !revisionsByPage.has(page.pageid)
    );
    for (let offset = 0; offset < pendingPages.length; offset += REVISION_BATCH_SIZE) {
      const chunk = pendingPages.slice(offset, offset + REVISION_BATCH_SIZE);
      const revisionPages = await revisionPagesForEntries(state, chunk);
      for (const revisionPage of revisionPages) {
        if (!Number.isInteger(revisionPage.pageid) || !Array.isArray(revisionPage.revisions)) {
          throw new Error("explicit revid response contained an invalid page");
        }
        if (revisionsByPage.has(revisionPage.pageid)) {
          throw new Error(`explicit revid response repeated pageid ${revisionPage.pageid}`);
        }
        revisionsByPage.set(revisionPage.pageid, revisionPage.revisions);
        if (Array.isArray(revisionPage.acquisition_errors)) {
          errorsByPage.set(revisionPage.pageid, revisionPage.acquisition_errors);
        }
      }
    }
    return enrichedPages.map((page) => {
      const revisions = revisionsByPage.get(page.pageid) || [];
      if (
        Number.isInteger(page.lastrevid) &&
        page.lastrevid > 0 &&
        (revisions.length !== 1 || revisions[0].revid !== page.lastrevid)
      ) {
        throw new Error(
          `explicit latest revision mismatch for pageid ${page.pageid}: expected ${page.lastrevid}`
        );
      }
      return {
        ...page,
        revisions,
        acquisition_errors: [
          ...(page.acquisition_errors || []),
          ...(errorsByPage.get(page.pageid) || []),
        ],
      };
    });
  }

  function allPagesContinuation(continuation) {
    if (continuation?.apcontinue) return continuation;
    if (continuation?.gapcontinue) return { apfrom: continuation.gapcontinue };
    return {};
  }

  async function crawl(sourceId) {
    if (crawlRunning) return;
    let state = await getState(sourceId);
    if (!state) throw new Error("No saved export exists; choose New first");
    if (state.status === "complete") throw new Error("This export is already complete");
    crawlRunning = true;
    stopRequested = false;
    state.status = "running";
    state.lastError = null;
    await putState(state);
    renderStatus(state);
    try {
      while (state.namespacePosition < state.namespaceIds.length) {
        if (stopRequested) {
          state.status = "paused";
          await putState(state);
          return;
        }
        const namespaceId = state.namespaceIds[state.namespacePosition];
        currentActivity = `枚举命名空间 ${namespaceId}`;
        renderStatus(state);
        const response = await apiRequest(
          state.apiUrl,
          {
            action: "query",
            list: "allpages",
            apnamespace: namespaceId,
            aplimit: 50,
            apfilterredir: "all",
            ...allPagesContinuation(state.continuation),
          },
          state.delayMs
        );
        const enumeratedPages = response.query?.allpages || [];
        if (!Array.isArray(enumeratedPages)) throw new Error("query.allpages was not an array");
        const pageStubs = await pageInfoForStubs(state, enumeratedPages);
        if (pageStubs.some((page) => page.ns !== namespaceId)) {
          throw new Error(`API returned a page outside namespace ${namespaceId}`);
        }
        const pages = await attachLatestRevisions(state, pageStubs);
        await storePages(sourceId, pages);
        state.batchesCompleted += 1;
        if (response.continue) {
          state.continuation = response.continue;
        } else {
          state.namespacePosition += 1;
          state.continuation = {};
        }
        await putState(state);
        renderStatus(state);
      }
      state.status = "complete";
      state.completedAt = utcNow();
      currentActivity = "";
      await putState(state);
    } catch (error) {
      state.status = "paused";
      state.lastError = String(error?.stack || error);
      await putState(state);
      throw error;
    } finally {
      crawlRunning = false;
      renderStatus(state);
    }
  }

  async function sha256Hex(text) {
    const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
    return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
  }

  async function exportBundle(sourceId) {
    const state = await getState(sourceId);
    if (!state || state.status !== "complete") {
      throw new Error("Only a completed crawl can be exported as a complete bundle");
    }
    const pages = await readPages(sourceId);
    pages.sort((left, right) => left.pageid - right.pageid);
    const pageIds = pages.map((page) => page.pageid);
    if (new Set(pageIds).size !== pageIds.length) throw new Error("duplicate page IDs in IndexedDB");
    const namespaceCounts = {};
    let acquisitionWarningPages = 0;
    let acquisitionErrorPages = 0;
    for (const page of pages) {
      namespaceCounts[String(page.ns)] = (namespaceCounts[String(page.ns)] || 0) + 1;
      acquisitionWarningPages += Boolean(page.acquisition_warnings?.length);
      acquisitionErrorPages += Boolean(page.acquisition_errors?.length);
    }
    const header = {
      schema: BUNDLE_SCHEMA,
      kind: "header",
      source_id: sourceId,
      api_url: state.apiUrl,
      transport: "browser-mediawiki-api",
      retrieved_at: state.startedAt,
      namespace_ids: state.namespaceIds,
      siteinfo: state.siteinfo,
      request_contract: {
        page_inventory: "allpages",
        includes: [
          "page-info-with-declared-fallback",
          "explicit-lastrevid-content",
          "pageid-current-revision-fallback",
        ],
        includes_redirect_pages: true,
        includes_file_binaries: false,
      },
    };
    const trailer = {
      schema: BUNDLE_SCHEMA,
      kind: "complete",
      source_id: sourceId,
      page_count: pages.length,
      namespace_counts: namespaceCounts,
      acquisition_warning_pages: acquisitionWarningPages,
      acquisition_error_pages: acquisitionErrorPages,
      page_ids_sha256: await sha256Hex(pageIds.map((pageId) => `${pageId}\n`).join("")),
      completed_at: state.completedAt,
    };
    const parts = [JSON.stringify(header), "\n"];
    for (const page of pages) parts.push(JSON.stringify({ kind: "page", page }), "\n");
    parts.push(JSON.stringify(trailer), "\n");
    let blob = new Blob(parts, { type: "application/x-ndjson" });
    let extension = "jsonl";
    if (typeof CompressionStream === "function") {
      blob = await new Response(blob.stream().pipeThrough(new CompressionStream("gzip"))).blob();
      extension = "jsonl.gz";
    }
    const link = document.createElement("a");
    link.href = URL.createObjectURL(blob);
    link.download = `${sourceId}-${state.completedAt.replace(/[:.]/g, "-")}.${extension}`;
    document.body.appendChild(link);
    link.click();
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(link.href), 30000);
  }

  function panelElements() {
    return {
      panel: document.getElementById("sts-wiki-export-panel"),
      source: document.getElementById("sts-wiki-source"),
      delay: document.getElementById("sts-wiki-delay"),
      status: document.getElementById("sts-wiki-status"),
    };
  }

  async function renderStatus(explicitState = null) {
    const elements = panelElements();
    if (!elements.panel) return;
    const sourceId = elements.source.value.trim();
    const state = explicitState || (await getState(sourceId));
    if (!state) {
      elements.status.textContent = "尚未建立采集；点击“新建并开始”。";
      return;
    }
    const namespaceId = state.namespaceIds[state.namespacePosition];
    const displayedStatus =
      state.status === "running" && !crawlRunning
        ? "interrupted（本标签页无活跃任务，可点击继续）"
        : state.status;
    elements.status.textContent = [
      `状态: ${displayedStatus}`,
      `命名空间: ${state.namespacePosition}/${state.namespaceIds.length}${namespaceId === undefined ? "" : ` (ns=${namespaceId})`}`,
      `批次: ${state.batchesCompleted}`,
      currentActivity ? `当前请求: ${currentActivity}` : "",
      `更新: ${state.updatedAt}`,
      state.lastError ? `错误: ${state.lastError}` : "",
    ]
      .filter(Boolean)
      .join("\n");
  }

  function installPanel() {
    if (document.getElementById("sts-wiki-export-panel")) return;
    const panel = document.createElement("section");
    panel.id = "sts-wiki-export-panel";
    panel.style.cssText = [
      "position:fixed",
      "right:16px",
      "bottom:16px",
      "z-index:2147483647",
      "width:340px",
      "max-height:70vh",
      "overflow:auto",
      "padding:12px",
      "border:1px solid #6b7280",
      "border-radius:8px",
      "background:#111827",
      "color:#f9fafb",
      "font:13px/1.4 system-ui,sans-serif",
      "box-shadow:0 8px 24px rgba(0,0,0,.35)",
    ].join(";");
    panel.innerHTML = `
      <strong style="display:block;margin-bottom:8px">STS Wiki 全量导出</strong>
      <label style="display:block;margin:6px 0">Source ID
        <input id="sts-wiki-source" value="${sourceIdForPage()}" style="width:100%;box-sizing:border-box" />
      </label>
      <label style="display:block;margin:6px 0">请求间隔（ms）
        <input id="sts-wiki-delay" type="number" min="200" step="100" value="${DEFAULT_DELAY_MS}" style="width:100%;box-sizing:border-box" />
      </label>
      <div style="display:flex;flex-wrap:wrap;gap:6px;margin:8px 0">
        <button data-action="new">新建并开始</button>
        <button data-action="resume">继续</button>
        <button data-action="pause">暂停</button>
        <button data-action="export">导出</button>
        <button data-action="clear">清除</button>
      </div>
      <pre id="sts-wiki-status" style="white-space:pre-wrap;margin:8px 0 0"></pre>
    `;
    document.body.appendChild(panel);
    panel.addEventListener("click", async (event) => {
      const action = event.target?.dataset?.action;
      if (!action) return;
      const elements = panelElements();
      const sourceId = elements.source.value.trim();
      const delayMs = Number(elements.delay.value);
      try {
        if (!sourceId) throw new Error("Source ID 不能为空");
        if (!Number.isInteger(delayMs) || delayMs < 200) {
          throw new Error("请求间隔必须是至少 200ms 的整数");
        }
        if (action === "new") {
          if (!confirm(`清除 ${sourceId} 的旧断点并重新开始？`)) return;
          const state = await createFreshState(sourceId, delayMs);
          renderStatus(state);
          await crawl(sourceId);
        } else if (action === "resume") {
          const state = await getState(sourceId);
          if (!state) throw new Error("没有可恢复的采集");
          state.delayMs = delayMs;
          await putState(state);
          await crawl(sourceId);
        } else if (action === "pause") {
          stopRequested = true;
        } else if (action === "export") {
          await exportBundle(sourceId);
        } else if (action === "clear") {
          if (!confirm(`永久清除浏览器中 ${sourceId} 的页面和断点？`)) return;
          if (crawlRunning) throw new Error("请先暂停并等待当前请求结束");
          await clearSource(sourceId);
          await renderStatus();
        }
      } catch (error) {
        console.error("STS Wiki exporter", error);
        alert(String(error?.stack || error));
        await renderStatus();
      }
    });
    renderStatus();
  }

  installPanel();
})();
