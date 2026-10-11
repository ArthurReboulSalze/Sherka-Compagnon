# Web search

Sherka offers optional DuckDuckGo search, with no account or API key. Search planning uses the companion's selected conversation provider. Speech stays in VoiceStudio. Web search requires an Internet connection.

## Choose when to search

Open **Settings → Web search → Internet access**:

- **Disabled** is the default. Sherka does not send search requests.
- **On request** searches when you check **Search the web for this message** in Conversation, type `/web your question`, or explicitly ask to search the web. Voice requests work too; in French, say “cherche sur Internet…”.
- **Automatic** asks the conversation model whether the current message requires external public information. It then runs at most one short search before answering. Personal stories, daily activities, feelings, greetings, creative writing and memory edits do not need search. If the planning request fails or returns invalid JSON, Sherka continues the ordinary conversation without claiming a search happened. Reliability depends on the model; use an explicit request if it misses a needed search.

The setting applies across companions. Changing it stops the current response. Switching companions, interrupting, pausing or quitting cancels work in progress. The next response continues to use the companion's own personality, language, model and voice.

## Results and sources

Sherka retrieves up to five titles, links and snippets from DuckDuckGo's lightweight HTML interface. It supplies only the entries that fit the current model's context budget, with space still reserved for the answer. The final answer keeps the usual text streaming and speech preparation.

Source numbers can appear in the written answer. Sources are shown with that exchange and survive restart in the companion's own history. Click a source to open it in your default browser. Source numbers and web URLs are removed from speech passages for a web-backed answer.

**These are search snippets, not full page readings.** A result does not prove the whole article was read or that its claims are correct. Verify important information at the source. Reading full pages and more search providers are future work.

Local models can still misinterpret snippets or misattribute a citation, especially small models. The source cards contain the links actually retrieved; a URL invented in the written answer is not added as a trusted source.

## Privacy and limitations

Only the short search query and a language/region preference go to DuckDuckGo, along with normal connection metadata such as your IP address. Sherka does not attach stored memories, voice recordings or conversation history. A query can still contain personal information from the question you ask; keep that in mind when enabling search.

Automatic search decisions use a bounded structured JSON request to the selected conversation provider, separate from the spoken answer. A remote conversation provider also receives this planning request; VoiceStudio remains local. Search evidence is treated as untrusted text and cannot directly execute commands or edit memory. Links are validated in Rust and the model cannot fetch arbitrary URLs.

DuckDuckGo's HTML interface is not a guaranteed search API. It can change, rate-limit requests or demand a bot check. If a search fails or returns no usable evidence, Sherka speaks a fixed notice in the companion's language instead of letting the model fabricate a verification. It does not solve or bypass challenges, and does not silently switch to another provider. Try later or search in your browser.

With an outbound-blocking firewall such as Windows Firewall Control, allow Internet access for the Sherka executable you actually run, then try again. Rules for another installation path may not cover a development build or a new portable folder. Sherka does not modify firewall settings.

Search adds latency before the answer starts, especially in Automatic mode. There is no background polling or autonomous browsing. The provider request has a timeout and bounded response size.

Search receipts were introduced with database schema 4. Version 0.6.1 upgrades profiles to schema 5 to raise the former reply default once. Older binaries open startup recovery for these profiles so an update remains possible. Keep installed and no-install versions aligned; use a separate backup to run an older version with its original data.

## Development checks

`cargo test --workspace --locked` covers parsing, unsafe links, bounded evidence, context budgets, source isolation and the schema migration. The native `web search:` scenario covers settings, explicit and automatic requests, history, restart, provider errors and cancellation using loopback fixtures.

For an intentional live public request:

```powershell
cargo test -p sherka-core --test web_search live_duckduckgo_search -- --ignored --nocapture
```

`SHERKA_TEST_SEARCH_URL` is accepted only in debug builds and only for HTTP on `127.0.0.1`. Release builds use the fixed DuckDuckGo endpoint.

References: [DuckDuckGo Lite](https://lite.duckduckgo.com/lite/), [LM Studio structured output](https://lmstudio.ai/docs/developer/openai-compat/structured-output).
