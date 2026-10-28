# Local connections

Adapters were exercised with **LM Studio 0.4.24** and **VoiceStudio 0.5.6**; the endpoints below answer unchanged with **VoiceStudio 0.5.7**. Catalogs are refreshed; engine, weights and voice are separate availability checks. Provider changes may require adaptation.

After a VoiceStudio update, model or voice licences may wait to be accepted in its own interface; until then VoiceStudio refuses the request, and Sherka says so (open VoiceStudio, accept the licence or check the model) instead of asking for an API key.

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

VoiceStudio's server address and transcription model are configured once in **Settings → VoiceStudio and transcription**. Each companion keeps its own TTS engine, voice, language, speaking speed and volume. Existing installations migrate the active companion's speech connection into these global settings once.

**Start with Sherka** launches the prepared VoiceStudio Python server without opening Electron. Leave the paths empty for automatic detection, or select a custom installation/runtime folder and Python executable. Open VoiceStudio once to complete its initial setup and install engines and voices. Sherka does not install packages or models. Startup logs are stored in Sherka's local `logs/voicestudio.log`.

Sherka reuses a ready service. Only local root HTTP addresses may start a process; remote and HTTPS endpoints must already be running. Startup waits for the health endpoint and speech protocol, for up to ten minutes, because a cold start can load several large speech and transcription models. While the server process is alive, still refusing connections, or answering "not ready" (HTTP 5xx, a loading `/health`, a dropped connection), Sherka only reports that VoiceStudio is loading, with the elapsed time. It reports an error as soon as the process exits (with its exit code), the budget is exhausted, or the endpoint gives a definitive answer (for example HTTP 401 or 404, or another service). A spoken or typed turn made meanwhile waits for readiness instead of failing. The microphone remains off until you enable it.

On exit, Sherka closes only server processes started by this session, including all their child processes, whichever way it quits (tray, widget or panel, quit without unloading, an update restart). Servers that were already running, including one a previous session started and left behind after a crash, and remote services, are left alone. On Windows the server runs in a job that is closed with Sherka, so it also ends if Sherka crashes or is killed; on Linux the server's process group is killed on exit, and the server itself ends if Sherka dies. `audio-events.log` gets one fixed line when Sherka quits: `voicestudio_stopped` (started by Sherka, closed), `voicestudio_left_running` (it was not Sherka's) or `voicestudio_stop_failed`.

Once VoiceStudio is ready, Sherka warms it up in the background so that the first reply is not slowed by model loading: the active companion's voice (and the guest's) says a short word that is discarded, and the transcription model receives half a second of silence. Nothing is played, shown or saved, and it does not delay a turn beyond VoiceStudio's own queue. It runs again for another engine, voice, language or transcription model, and after VoiceStudio restarts; it is skipped when the speech service is off or a companion has no voice. `audio-events.log` gets a `voicestudio_warmup` line with the duration in milliseconds and `ok` or `failed`.

References: [VoiceStudio](https://github.com/debpalash/VoiceStudio), [speech platform](https://github.com/debpalash/VoiceStudio/blob/main/docs/speech-platform.md), [router](https://github.com/debpalash/VoiceStudio/blob/main/backend/api/routers/openai_compat.py).

## Limitations

Remote conversation providers use separate OpenAI Chat Completions and Anthropic Messages adapters with protected API keys. See [providers](PROVIDERS.md). External software and models are neither embedded nor redistributed. VoiceStudio token authentication is not implemented.
