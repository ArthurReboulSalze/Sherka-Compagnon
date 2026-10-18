# Conversation providers

Each companion saves its own provider, model, context ceiling and thinking preference. Its VoiceStudio voice and local memories remain separate from those choices.

In **Companions**, select a character, then use **Voices & models** to choose its provider, save a key if required, and **Test and refresh**. Pick a catalog model or enter its exact ID, set a context ceiling when the API does not report one, and save the companion’s choices. Voice selection is in the character's **Avatar & voice** editor.

| Provider | Default API base | Format |
| --- | --- | --- |
| LM Studio | `http://127.0.0.1:1234` | Local Chat Completions and native model management |
| OpenRouter | `https://openrouter.ai/api/v1` | OpenAI Chat Completions |
| Z.ai API | `https://api.z.ai/api/paas/v4` | OpenAI Chat Completions, API credit |
| Z.ai Coding Plan | `https://api.z.ai/api/coding/paas/v4` | OpenAI Chat Completions, coding subscription |
| OpenAI | `https://api.openai.com/v1` | OpenAI Chat Completions |
| Anthropic | `https://api.anthropic.com/v1` | Anthropic Messages |
| Compatible API | Your full API base | OpenAI Chat Completions or Anthropic Messages |

The base URL includes its API path. Do not append `/chat/completions` or `/messages`, put a key in the URL, or omit a required path such as `/api/v1`. Public endpoints require HTTPS; loopback HTTP is accepted for local compatible servers. Redirects are not followed with credentials. An OpenAI API key is separate from a ChatGPT subscription; this is not a ChatGPT login integration. The Responses API is not implemented.

## Keys and privacy

Windows Credential Manager protects API keys for the current Windows account. A key is scoped to its provider and exact API base, shared by companions using that connection, and excluded from SQLite, profile exports and releases. The interface shows only whether a key exists. Removing a key affects all companions using that connection. Imported profiles require the recipient’s own key.

With a remote provider, the companion prompt, selected conversation history, recalled memories and memory processing requests go to that provider. VoiceStudio keeps recognition and synthesis local. No audio recordings, avatar files or complete memory databases are uploaded by the conversation adapter. Provider billing and data policies apply. LM Studio remains the default for offline conversations; remote provider catalogs are refreshed on request.

## Context and thinking

LM Studio budgets against the context actually loaded. Remote budgets use advertised context and output limits, bounded by the companion ceiling. When context metadata is absent, Sherka uses a configurable **64,000-token fallback**. This is a budgeting default, not a claim about the model's actual capacity. A documented provider limit always takes precedence over a larger configured ceiling. Initial estimates include a safety margin and are distinct from API usage counts.

The default reply reserve is **8,192 tokens**. This is a maximum, not a request to make every answer long. Small contexts and advertised output limits can reduce it. Upgrading from 0.6.0 raises the former 1,024-token default once on existing profiles; other saved reserves remain unchanged.

OpenRouter thinking controls follow the catalog, including mandatory reasoning: disable is hidden when the model requires thinking. Z.ai uses its documented GLM thinking controls; for GLM 5.3 and Flash, thinking is mandatory and the accepted efforts are low, high and max. Choose low for shorter voice turns. Anthropic uses advertised adaptive thinking and effort when available, or a manual budget for explicit enabled thinking. Manual thinking needs more than 1,024 output tokens. Compatible APIs that omit capabilities offer explicit overrides; check your model’s documentation before using them. Defaults send no thinking override.

Only final text is displayed and spoken. Thinking deltas never enter the voice queue. Server length limits and incomplete streams are reported. Lower output reserves reduce cost but can leave no visible answer when mandatory reasoning consumes the reserve.

## Memory, speech and switching

Structured memory operations and automatic search planning use the same selected provider. Constrained JSON is used when supported; otherwise the schema is supplied in the prompt, with Z.ai JSON mode where applicable. Invalid or incomplete proposals never change memory. Existing provenance, revision and profile-isolation guards still apply. Automatic memory consolidation may make an additional API call while idle; automatic web search may add a planning call.

Switching from LM Studio to a remote provider unloads the previous managed local instance. Remote providers receive no load or unload requests and allocate no local VRAM. Switching back loads the chosen LM Studio model. Unrelated model instances remain untouched.

VoiceStudio is required for microphone input and spoken replies with every provider. Text generation remains independent of TTS, so a voice service failure does not erase a completed written answer.

## Compatibility and troubleshooting

- Catalog unavailable: use an exact model ID and the context limit documented for that model. Some endpoints do not expose `/models`; other catalogs return identifiers without limits.
- HTTP 401/403: check the key, its provider and access rights. Keys are never copied to a different endpoint automatically.
- HTTP 402/429: check credit, subscription eligibility and rate limits. Documented credit errors (including Z.ai code 1113) are distinguished from temporary overload, without exposing raw error bodies. Sherka does not automatically retry billable generations or switch to another model.
- Unsupported thinking or JSON settings: leave thinking on the provider default or select supported options. Custom gateways may support only part of an advertised format.
- Z.ai: choose **Z.ai API** for API credit or **Z.ai Coding Plan** for your coding subscription. Their keys are scoped separately; save the matching key for each connection. Coding Plan has separate model eligibility and [supported-tool rules](https://docs.z.ai/devpack/usage-policy). Check that your subscription permits your use; Sherka does not select or retry on the other endpoint automatically.

See the official [OpenAI Chat Completions reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create), [Anthropic Messages](https://platform.claude.com/docs/en/api/messages/create), [OpenRouter catalog](https://openrouter.ai/docs/api/api-reference/models/list-all-models-and-their-properties), [OpenRouter reasoning](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens) and [Z.ai Chat Completions](https://docs.z.ai/api-reference/llm/chat-completion).

## Hermes remote agent

Choose **Hermes · remote agent** to use Sherka as a voice client for a server-owned agent. Select the server and profile and save that profile’s Hermes API key. See [Hermes setup](HERMES.md). Local personality, memory, search, token budgets and reasoning controls do not apply in this mode.
