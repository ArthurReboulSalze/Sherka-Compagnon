# Local connections

Adapters were exercised with **LM Studio 0.4.24** and **VoiceStudio 0.5.6**. Catalogs are refreshed; engine, weights and voice are separate availability checks. Provider changes may require adaptation.

## LM Studio

Default: `http://127.0.0.1:1234`. Enable its server separately; Sherka does not start this application.

| Endpoint | Purpose |
| --- | --- |
| `GET /api/v1/models` | Keys, instances, maximum/loaded context, quantization and reasoning |
| `GET /v1/models` | Compatible identifiers |
| `POST /api/v1/models/load` | Activate a companion's model |
| `POST /api/v1/models/unload` | Unload the tracked instance |
| `POST /v1/chat/completions` | SSE conversation and usage; structured memory requests |

Budgets use loaded context, bounded by model maximum and profile cap. Reply space and margin are reserved before selecting memories and exchanges. Preliminary estimation is conservative, distinct from server usage. Oversized messages are rejected explicitly.

Reasoning controls follow advertised capabilities. Binary `off` maps to `reasoning_effort: none`, `on` to `medium`; advertised levels pass through. Reflection stays out of speech and memories.

Normal shutdown waits for loads, unloads the tracked instance and verifies the catalog while preserving foreign instances. Forced termination cannot guarantee cleanup.

References: [REST](https://lmstudio.ai/docs/developer/rest), [conversation](https://lmstudio.ai/docs/developer/openai-compat/chat-completions), [structured output](https://lmstudio.ai/docs/developer/openai-compat/structured-output).

## VoiceStudio

Default: `http://127.0.0.1:3900`. Install separately. TTS and voice belong to the companion; no personal voice identifier is supplied in presets.

| Endpoint | Purpose |
| --- | --- |
| `GET /health` | Readiness |
| `GET /.well-known/voicestudio-speech` | `voicestudio.speech.v1` and STT capabilities |
| `GET /engines/tts`, `/models`, `/v1/models` | Engines, weights and aliases |
| `GET /v1/audio/voices` | Voice profiles |
| `GET /dictation/models`, `/dictation/prefs` | Models and dictation compatibility |
| `POST /v1/audio/speech` | TTS, PCM16 mono 24 kHz for the tested engine |
| `POST /v1/audio/transcriptions` | HTTP fallback, WAV16 mono 16 kHz |
| WebSocket `/v1/audio/transcriptions/stream` | Direct STT, PCM16 mono 16 kHz |

VoiceStudio 0.5.6 synthesizes a passage before returning PCM. Sherka prepares the next during playback with a bounded queue and one TTS request at a time; SSE text is independent. This is not token-by-token synthesis. Some WebSocket engines follow dictation preferences despite `model`; Sherka checks and reports HTTP fallback.

### Automatic startup

Sherka reuses a ready instance or launches installed `VoiceStudio.exe` from standard Windows locations or the registry. The selected port passes through `OMNIVOICE_PORT`. An already-starting instance is not duplicated. New windows are minimized; existing ones retain their state.

Only local root HTTP URLs trigger launch. Remote/HTTPS services must be running. Maximum wait: five minutes; probes: five seconds and 64 KiB. Access/protocol errors do not launch a second process. The panel offers **Réessayer VoiceStudio**.

The API and protocol must be ready, not all preloaded models. Voice waits are cancellable. The microphone is off at startup; VoiceStudio remains open after exit.

References: [VoiceStudio](https://github.com/debpalash/VoiceStudio), [speech platform](https://github.com/debpalash/VoiceStudio/blob/main/docs/speech-platform.md), [router](https://github.com/debpalash/VoiceStudio/blob/main/backend/api/routers/openai_compat.py).

## Limitations

Remote conversation providers use separate OpenAI Chat Completions and Anthropic Messages adapters with protected API keys. See [providers](PROVIDERS.md). External software and models are neither embedded nor redistributed. VoiceStudio token authentication is not implemented.
