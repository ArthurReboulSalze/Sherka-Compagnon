# Web search

Sherka offers optional web search through **DuckDuckGo, Brave Search, Tavily or SearXNG**. Search planning uses the companion's conversation model; speech stays in VoiceStudio. Each companion has its own engine and search mode. Hermes always bypasses Sherka's search controls and uses its server's tools.

## Choose an engine

Open **Companions → Web search**. Choose the engine, maximum result count (1–10) and save. The default is DuckDuckGo without an account.

| Engine | Setup | Notes |
| --- | --- | --- |
| DuckDuckGo | No key | Lightweight HTML search; can be rate-limited or blocked by bot checks. |
| Brave Search | Your Brave Search API key | Official search API with titles, links and available extra excerpts. |
| Tavily | Your Tavily API key | Basic general search, without a generated answer or full raw-page payload. |
| SearXNG | Your instance URL | Enable JSON output in the instance settings; no Sherka API key is required. |

Enter Brave/Tavily keys in this section. They are stored in Windows Credential Manager or Linux Secret Service, separately from profiles and exports. A key is shared by companions using that search service. Removing a key does not erase the companion's engine choice. Keys are sent only to the service's fixed HTTPS endpoint. An external API account may incur usage charges.

SearXNG accepts HTTPS addresses, or HTTP on loopback/private IP addresses. Addresses cannot contain credentials, a query or a fragment. Sherka calls the instance's `/search` endpoint with `format=json`. If it refuses JSON, enable that format in SearXNG rather than supplying an unrelated API key.

## Choose when to search

- **Disabled** is the default. No search query is sent.
- **On request** searches when you check **Search the web for this message** in Conversation, type `/web your question`, or explicitly ask to search the web. Voice requests use the same path.
- **Automatic** asks the conversation model whether the current message needs public information, then runs at most one short search. Personal stories, feelings, greetings, creative writing and memory edits do not need search. If planning fails, ordinary conversation continues without claiming a search happened. Use an explicit request if a model misses a needed search.

Profiles without an individual mode inherit the old application preference until edited. A configuration change stops the current response. Switching companions, interruption, pause or quitting cancels work in progress. No engine silently falls back to another paid or unpaid service.

## Results and sources

Sherka supplies the titles, validated links and excerpts that fit the selected model's context budget, preserving space for its answer. Text streaming and speech preparation then continue normally. These are **search excerpts**, not proof that the complete pages have been read. Full-page reading remains future work.

Sources accompany that response and survive restart in the companion's own history. Click a source to open it in your browser. Source numbers and web URLs are removed from spoken passages. A model-invented URL is not added to the trusted source cards; small models can still misinterpret excerpts or cite them poorly.

## Privacy and limits

The chosen engine receives a short query, language preference where supported and normal network metadata. Sherka does not attach memories, recordings, image attachments or full conversation history. The question itself may contain personal information, so review what you ask to search.

Automatic planning sends a bounded structured request to the selected conversation provider. With a hosted model this is additional API usage. Search evidence is untrusted content: it cannot directly execute commands or edit memory. Requests have time/size limits, do not follow redirects, and filter unsafe or private source links.

DuckDuckGo's HTML interface can change or demand a challenge. Official API options avoid depending on that HTML parser, but still have their own quotas and availability. Sherka reports missing/refused keys, quota errors and SearXNG JSON refusals. For an explicit search with no usable evidence it uses a fixed localized notice rather than inventing a verification. It never bypasses challenges.

If a firewall blocks outbound traffic, allow the executable path you actually run. Rules for another installation or portable folder may not cover it. Sherka does not change firewall rules. There is no autonomous background browsing or polling.

Engine preferences are included in profile export version 4; keys and search receipts are excluded. Current development databases use schema 8. Keep installed and portable versions aligned, and use a separate backup/data folder when testing an older executable.

Use **Test search engine** beside the saved engine settings to send a fixed, non-personal test query. This tests the selected draft engine and its stored key directly, without asking the companion model to choose a search. It does not save the draft settings. Save the engine separately when satisfied. A Windows 10013 message means the executable's outbound connection was blocked locally, rather than a key refusal.

Automatic search decisions use the same bounded, reasoning-aware JSON path as memory and scene planning, including a retry without a JSON grammar when the model rejects it. Very small models can still misclassify intent; an explicit “search the web” request or the Conversation search button bypasses this decision.

## Development checks

`cargo test --workspace --locked` checks source parsing, unsafe links, provider request/response contracts, context budgets, isolation and migrations. Brave, Tavily and SearXNG contracts use loopback fixtures and synthetic keys; they do not spend a real API account's credits. The native scenarios cover search settings, explicit/automatic requests, source history, restart, failure and cancellation. No live Brave/Tavily account is required.

An optional live DuckDuckGo check is available through `cargo test -p sherka-core --test web_search live_duckduckgo_search -- --ignored --nocapture`. The debug-only `SHERKA_TEST_SEARCH_URL` override accepts HTTP on `127.0.0.1`; release builds use the fixed endpoint.

References: [DuckDuckGo Lite](https://lite.duckduckgo.com/lite/), [Brave Search API](https://api-dashboard.search.brave.com/documentation), [Tavily search](https://docs.tavily.com/documentation/api-reference/endpoint/search), [SearXNG search API](https://docs.searxng.org/dev/search_api.html).
