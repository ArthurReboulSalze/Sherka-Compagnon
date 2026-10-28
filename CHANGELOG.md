# Changelog

## Unreleased

- When VoiceStudio refuses a request (HTTP 401, 403, 409 or 451), say to open VoiceStudio to accept a pending model or voice licence or check the selected model, instead of asking for an API key a local VoiceStudio does not use. Endpoints checked unchanged with VoiceStudio 0.5.7.
- Close the VoiceStudio that Sherka started, and only that one, on every way out: quitting, quitting without unloading, an update restart, and a Sherka crash (Windows job closed with Sherka; Linux process group and `PR_SET_PDEATHSIG`). The server is now placed in its job before it can start any child process, a server that died no longer leaves workers holding its port for the next try, and `audio-events.log` records `voicestudio_stopped`, `voicestudio_left_running` (a server Sherka did not start, left alone) or `voicestudio_stop_failed` when you quit.
- Warm VoiceStudio up once it is ready, so the first reply does not wait for its models to load: a short word in the companion's voice (and the guest's) is synthesized and discarded, and half a second of silence goes through transcription. Nothing is played, shown or saved. It repeats for another voice, language or model and after VoiceStudio restarts, and records a `voicestudio_warmup` line (duration, `ok` or `failed`) in `audio-events.log`.
- Tell the companion about the image generator: it knows whether it is on or off, the model family (Illustrious or SDXL) and the selected checkpoint, so it no longer promises pictures that will not come or ignores the ones that do. It is told nothing when no image model is set up, and for Hermes companions.
- Let you control the generator by voice or text, in all seven languages: "switch on / off the image generator" and "make me an image of…". Only you switch it (these words or the **Image** button); the companion never does, and it asks you to when it is off.
- Keep an image request made while the generator is off and draw it as soon as you switch the generator on, by voice or with **Image**, instead of illustrating the "switch it on" exchange. "Switch it on and make me…" does both in one message; switching on alone starts the images with your next message.
- Start a requested image by itself when ComfyUI is still starting: the request is no longer lost when you speak, type or the companion talks while it loads, and it is drawn as soon as ComfyUI is ready.
- Let the companion react to the **Image** button with one brief remark when you turn the generator on.
- Keep the generator unavailable in three-way conversations: a request is answered honestly and the switch is never touched.

## 1.4.0 — 2026-10-28

- Add three-way conversations: invite a second companion from the widget toolbar with **Add a companion**. The active companion keeps the controls; the guest gets its own transparent window with its seven expressions, speaking halo, independent drag and resize, and saved position. Its controls follow the portrait's visible pixels.
- Choose who answers: name a companion within the first three words ("Minerva, tell me a joke") and that companion replies, with case, accent and one-letter tolerance. Otherwise one companion chosen at random answers. Exactly one companion answers your message, and it cannot pass; the other one reacts later (see below).
- Speak with two voices in turn, never overlapping. Speaking over them, **Stop** or pausing interrupts both; the heard part of an interrupted reply is shared with the other companion.
- Prepare the other companion's reaction while the answerer is still speaking: its text is generated in the background once the answerer's text is complete, and up to two passages are synthesized once the answerer's speech is. Nothing is shown, saved or stored until it is its turn, and the preparation does not hold the turn open, so you can speak during the wait. Any activity of yours drops it. Still one voice at a time.
- Let the companions keep the conversation going when you stay silent. Once the answerer has finished, they wait for the delay (off, 5, 8, 10 or 15 seconds; 8 by default, in Settings). If you say nothing, the other companion reacts to your message and the answer using the prepared reply, then after a quick silence of one second the answerer reacts to that reaction, and they chain replies with a 600 ms pause until you speak or type, press **Stop** or pause, 20 replies have been said since your last message, or two passes in a row. A companion that passes is asked again only after a further delay. Speaking, typing, **Stop**, pausing, changing the guest or master, hiding the widget or muting the microphone ends the exchange and the cycle starts over with a single answer. With the delay off, only one companion answers. With a paid cloud provider every reaction is billed, including prepared reactions that get discarded. The spontaneous idle trio still has two speakers back to back.
- Add **Favor quick replies** (off by default): in a trio, replies are limited to one or two short spoken sentences so speech starts sooner.
- Start the silence relay after the master's arrival remark too: the master speaks alone, the guest prepares its reaction during the remark and reacts after the delay if you say nothing, then the quick silence of one second applies and the companions chain their replies.
- Stop the companions from mistaking you for each other: in a three-way conversation, and whenever the history holds a companion who has left, your lines are labelled "User:" in what the models read, and their instructions name the three participants. Add an optional **Your first name** in Settings to use your name as the label instead.
- Tell a companion when the other one in its history has left the conversation, so it no longer addresses it as present.
- Let the master react to a new guest's arrival with one short spoken remark in character, introducing the guest first when the master has never talked with it. The remark is kept in both histories but never counts as your message.
- Share the master's model: the guest borrows its provider, model and context settings, so only one LM Studio model stays loaded, while keeping its own personality, voice, memory and mood. Spontaneous conversation also works in a trio.
- Keep the whole thread in each companion's history with the speaker recorded (database schema 10, mirrored messages; downgrading after the migration is not supported). Memory extraction still reads only your own messages.
- Remember the last three guests for each companion and offer **Recent** combinations in the picker, with **Resume the last conversation**. End a conversation from the picker, the guest's window or a banner in the panel.
- Exclude Hermes companions and companions without a voice from three-way conversations, and disable conversation illustrations while a guest is present.
- Keep the silence relay alive when you only click: a click on the widget or panel, or a key press outside a text field, no longer cancels the companions' reaction to your silence. Typing in a text field (the widget text cell or the panel composer), speaking, sending, **Stop** or pausing still do.
- Add `trio_relay_armed`, `trio_relay_wait` (`cooldown` or `quick_silence`), `trio_relay_fired`, `trio_relay_ended` and `trio_relay_skipped` lines to `audio-events.log` to diagnose a relay that does not fire. They hold fixed codes only, never names or text.
- Stop reporting a slow VoiceStudio start as an error: while its process is alive or it answers "not ready" (HTTP 5xx, a loading health check, a dropped connection), the status stays on loading with the elapsed time. The startup budget is now ten minutes, and a process that exits is reported at once with its exit code instead of after the wait.
- Show only the Sherka logo with a loading animation and the VoiceStudio status in the widget until the voice service is ready; the companion then fades in (immediately if the service is switched off, already running or failed to start).
- Start every companion with its image generator off: when Sherka starts and whenever you switch companion, **Images during conversation** is reset to off, so the image cell stays closed and ComfyUI is not started until you press **Image**.
- Keep companions learning about you in a trio: memory extraction for your message used to be cancelled by the companions' own replies (the relay fires seconds after the answer). Their reactions, chained replies, arrival remark and idle invitations no longer cancel it, and when companions are still talking after the 30 seconds it waits for them to fall silent instead of giving up. Only your next message, Stop, pausing, clearing the conversation or changing the guest or master cancel it.
- Retry the voice catalog check briefly (up to about 20 seconds) when turning the microphone on right after VoiceStudio reports ready, instead of showing an error while it finishes loading. An unknown voice or any definitive HTTP error is still reported at once.
- Translate the new controls into all seven interface languages.

## 1.3.4 — 2026-10-27

- Ship the bundled companion images as lossless WebP instead of PNG: 49.8 MB of artwork becomes 31.9 MB with identical visible pixels, and the Windows executable shrinks from 99 MB to 83 MB.
- Store new and edited companion images (character creation, retouching, your own picture and avatar editing) as lossless WebP, which is smaller than PNG and keeps every pixel and alpha value. Images stored by earlier versions stay PNG and keep working.
- Keep sending PNG with the same pixels to ComfyUI and vision models, and keep PNG inside `.sherka.zip` archives so earlier Sherka versions can still import them.
- Publish Windows installer/portable and Debian packages with signed Windows update assets.

## 1.3.3 — 2026-10-23

- Turning the microphone off no longer interrupts the companion: it finishes generating and speaking its reply, and the microphone device closes once nothing is left to play. Only an utterance you were still speaking is dropped. **Stop** and **Pause all voice activity** still interrupt the reply.
- Remove the large Hermes stop overlay drawn over the desktop companion; the small stop button in the widget toolbar and the typed `/stop` command remain.
- Publish Windows installer/portable and Debian packages with signed Windows update assets.

## 1.3.2 — 2026-10-23

- Start a new character from your own picture: it becomes the neutral portrait (scaled to 1024 px and flattened onto the portrait gray), the written idea becomes optional, and a vision-capable model also writes the physical description from the image. An imported portrait can also replace the neutral portrait later.
- Show and edit the exact ComfyUI prompts of each creation image with a **Prompt** button; edits are sent verbatim and invalidate only their own image.
- Show and edit the current conversation scene prompt in an editor attached below the desktop image cell, regenerate with it verbatim, keep it for variants and live mode until the next reply, or return to the automatic prompt.
- Reorganize illustration settings into remembered collapsible sections, with the LoRA folder filter and a new LoRA search placed directly above the LoRA rows and a save button that stays reachable.
- Regenerate an existing companion's images from its page with **Regenerate images**: the studio loads its current portrait and seven expressions, lets you regenerate any emotion or the whole portrait, then applies the images back to that companion while its personality, voice, history and memories stay unchanged.
- Move **Character creation** right below **Companions** in the sidebar.
- Translate the new controls into all seven interface languages, and publish Windows installer/portable and Debian packages with signed Windows update assets.

## 1.3.1 — 2026-10-21

- Record one `turn_timing` line per reply in the local audio diagnostics log: milliseconds from the end of speech to the accepted transcript, prepared request, first token, first passage sent to speech synthesis, first audio and complete text. No conversation content is logged.
- Start speaking sooner: the first spoken passage now needs 25 characters instead of 55 and may end at a clause comma; the second passage is medium-sized before passages grow to their previous length.
- Fade interrupted speech out over 30 ms instead of cutting it mid-waveform, removing the click when you talk over the companion. The faded tail is never counted as heard.
- Ignore a breath, click or lip noise after the last word instead of restarting the end-of-turn silence countdown; only sustained speech extends a turn.
- Stop cutting long streamed replies after 180 seconds; replies now fail only when the provider sends nothing for 180 seconds.
- Keep provider connections alive between turns through one shared connection pool, avoiding a new connection (and TLS handshake for hosted APIs) on every turn.
- Coalesce streamed text updates to the control panel and widget (at most one every 40 ms, expression changes immediately) and avoid rebuilding unchanged microphone buttons.
- Document response-time measurement in the user guide and README, and publish Windows installer/portable and Debian packages with signed Windows update assets.

## 1.3.0 — 2026-10-21

- Create a companion from an idea with an editable LLM-generated personality and appearance, without using existing conversations or memories.
- Generate a front-facing portrait with SDXL or Illustrious, edit six emotions with Flux Klein 4B, then apply fixed RMBG-2.0 background removal to all seven images.
- Approve the neutral portrait, generate or retry emotions individually, inspect images in a large preview and add the finished companion with fresh history, memory and voice selection.
- Preserve completed stage outputs when retrying, invalidate only affected images and remember selected models and scanned catalogs.
- Add optional IP-Adapter guidance, recommended image formats, explicit latent scaling and selectable final ×2 upscaling for conversation illustrations.
- Move the web-search provider and API configuration into global Settings, while keeping each companion's Internet access choice.
- Add Help & about with ComfyUI requirements, voice guidance and privacy information, translated with the new controls into all seven interface languages.
- Publish Windows installer/portable and Debian packages with signed Windows update assets.

## 1.2.0 — 2026-10-20

- Fix desktop freezes caused by native window queries waiting on locks held by the activity scheduler or geometry saver; handle activity notifications off the UI thread.
- Enlarge image cells and open illustrations full screen with a double-click, keeping the companion independently movable and resizable in front.
- Add character-specific negative appearance prompts, applied only when the character appears, for both Illustrious and SDXL.
- Accept image pastes in the widget text cell and mention image input in its placeholder.
- Explain first-image startup and selected model/LoRA loading directly in the image generator; translate the new controls into all seven interface languages.

## 1.1.1 — 2026-10-20

- Keep desktop widget controls visible when the pointer crosses transparent gaps between the avatar, toolbar and resize handle, including maximum-size Minerva.
- Preserve automatic hiding when leaving the avatar region and keep overlapping image cells interactive.
- Add a native regression covering maximum-size controls and real pointer resizing; wait for native window resizing in the text-cell test.

## 1.1.0 — 2026-10-20

- Add optional per-companion spontaneous conversation with random idle intervals, activity/voice guards and one invitation until a real reply; exclude Hermes.
- Move VoiceStudio connection/transcription and VoiceStudio/ComfyUI startup paths into global Settings, retaining companion-specific voices and speech delivery.
- Start VoiceStudio's prepared Python backend without Electron, with automatic runtime detection and configurable runtime/Python paths.
- Use one Start with Sherka option per service and close only locally owned process trees on exit, preserving pre-existing and remote servers.
- Persist avatar and image-cell geometry per companion across switching and restart; render overlapping illustrations behind the avatar.
- Keep machine speech settings, startup paths and desktop geometry out of companion exports; migrate the active companion's legacy speech connection once.
- Update all seven interfaces, documentation and Windows/Debian distributions, including signed Windows update assets.

## 1.0.0 — 2026-10-19

- Add optional per-companion persistent moods with bounded scores, turn-based decay, de-escalation, restart persistence and an independent reset. Exclude Hermes, copies and exports from accumulated mood state; advance the profile database to schema 9.
- Apply the selected board theme to widget controls, menus and inline cells while preserving each companion's accent for the active halo.
- Add continuous live ComfyUI illustrations, prompt-specific cancellation and conversation priority, without global interruption or custom scheduling nodes.
- Add advanced square output size (default 768), independent steps/CFG for both samplers and a shared sampler; Turbo toggles preserve advanced values.
- Export the active API workflow and remember full model/LoRA/sampler catalogs per server on this device, with a highlighted full-refresh button and offline cache fallback.
- Optionally start a configured local Python/portable ComfyUI installation on demand, reuse existing servers and wait for readiness; never launch for remote addresses.
- Improve thinking-model search planning, expose a search connection test, remove emojis from spoken replies and broaden confirmed conversation/fact deletion commands.
- Document the new controls in all seven interfaces and publish Windows installer/portable and Debian packages with signed Windows update artifacts.

## 0.12.1 — 2026-10-19

- Prioritize the user's visual request over the companion's reply for Illustrious and SDXL scenes, distinguishing stable identity from adaptable clothing, pose and expression.
- Recognize requests to depict the companion performing an action while keeping unrelated topics and explicit character exclusions separate.
- Plan one coherent scene before composing image-model syntax, keeping one companion by default and avoiding copies caused by interpreting the dialogue speakers as duplicate avatars.
- Preserve complete SDXL prompt sentences instead of truncating each comma-separated phrase as a short tag.

## 0.12.0 — 2026-10-19

- Put Companions first in the control-panel navigation and make duplication more visible, with localized copy names.
- Add a per-companion positive prompt suffix, appended once without rewriting for Illustrious and SDXL scenes.
- Export self-contained `.sherka.zip` archives with a versioned JSON manifest and seven PNG expressions, including built-in images and neutral fallbacks.
- Import archives into new empty profiles, with bounded in-memory ZIP/image validation and no filesystem extraction. Continue accepting legacy JSON versions 1–5.
- Document the archive format and update the README with the desktop screenshot, voice flow, memory, web/file capabilities and hardware guidance.


## 0.11.0 — 2026-10-19

- Choose Illustrious or SDXL per companion, with image prompts adapted to each family and an editable negative prompt sent exactly as saved.
- Browse all ComfyUI checkpoints and filter model/LoRA catalogs by relative folder, independently of filenames.
- Use 16 steps on both samplers when Turbo is off, doubling their original CFG (3.8 and 7.0 in the supplied workflow).
- Decide whether the companion belongs in each scene; landscapes and unrelated subjects can be illustrated without a foreground avatar.
- Give Minerva a robotic-face appearance description, preserving customized descriptions.
- Improve short structured requests for thinking models, including NeoHorse's system-message ordering, bounded output and validated fallback parsing.
- Help smaller models choose avatar expressions with short examples, accepted aliases and a conservative fallback based on the visible reply.
- Propose memory optimization from Settings, Memory or conversation commands; preview shorter facts and notes before confirming, preserving source links and untouched facts.
- Require confirmation before conversation clearing; keep saved facts and reject expired or stale confirmations.
- Activate the widget companion when selecting its card in the control panel.
- Refresh the English README with the supplied ecosystem diagram and document the new image and memory settings in all seven interfaces.

## 0.10.0 — 2026-10-18

- Keep widget controls and its resize handle close to the visible portrait, including wide avatars; use the character's chosen color for its active halo.
- Add Gold, Sage, Ocean, Iris, Rose and Mono application themes.
- Automatically grow and shrink the transparent widget when its text cell is resized; keep the chosen text height through refreshes and toggles.
- Toggle a transparent text cell below the widget controls to type, paste or drop text, with optional spoken replies. It follows the avatar without opening another window.
- Add optional square conversation illustrations through ComfyUI, configured per companion with an Illustrious checkpoint, physical description, additional LoRAs and strength sliders.
- Prepare each scene from the latest user/companion exchange using freely composed Illustrious tags and English descriptions, independently of speech and memory.
- Supply the provided workflow, automatic Turbo LoRA resolution and a bounded API-workflow import. Keep only the newest eligible preview; exclude Hermes.
- Start scene preparation as soon as the reply text is complete, while speech continues. Generate 1–8 images per exchange, with a configurable looping slideshow and half-second crossfades in a movable, resizable inline cell.
- Preserve confirmed transcription segments and the original audio when speech resumes during recognition. Filter specific subtitle-credit/non-speech hallucinations without discarding real short answers.
- Show brief, non-interactive widget errors above the controls.
- Accept dropped PNG, JPEG and WebP images when the selected model advertises vision or the user confirms an undocumented capability. Image pixels are transient and excluded from history, memory and exports.
- Add per-companion Brave Search, Tavily and SearXNG options alongside DuckDuckGo. Store API keys in the OS vault; keep Hermes search on its server.
- Add Linux credential storage, browser integration, Debian packaging and CI checks. Linux updates remain manual and external voice services are started separately.
- Advance profile/settings schema to 8 and profile export format to 4; retain legacy imports and scope isolated native instances to their data directory.

## 0.9.0 — 2026-10-18

- Clear a companion’s local conversation, archives, sources and summaries while keeping explicitly saved or approved facts by default; optionally clear saved facts too.
- Accept explicit conversation-reset requests in seven languages and `/clear`, with a spoken local acknowledgement and no extra language-model request.
- Delete individual facts from the Memory page and exported memory data.
- Cancel stale generation/extraction jobs and reset capture/session references so deleted history cannot be reintroduced. Hermes server data remains under Hermes control.
- Keep spoken replies and microphone consent active through temporary capture-buffer congestion; discard damaged utterances and resume after a pause.
- Keep transcription failures local to the affected turn instead of disabling the microphone.
- Avoid unused echo-reference work in headphone and playback-only modes.
- Preserve bounded local audio diagnostics across restarts, without recordings, conversation content or credentials.

## 0.8.1 — 2026-10-18

- Regenerate the Windows, installer, tray and in-app icons from the tightly cropped golden logo.
- Replace Candis with Alba's seven supplied images and display name, keeping the same personality traits in all seven languages.
- Update Minerva's neutral portrait and six expressions with the new armored design, preserving her irritable personality.
- Upgrade the original Candis identity to Alba while retaining profile IDs, models, voices, history and custom images. Keep edited names and personality text intact.
- Refresh the README portraits, descriptions and control-panel screenshot with new image URLs to avoid stale branding caches.

## 0.8.0 — 2026-10-18

- Add Botsy, Candis, Kit and Minerva alongside Sherka, with localized personalities and seven supplied expressions each; retire the Orion starter example.
- Group provider, model, reasoning, context, transcription and per-companion Internet search controls under Companions.
- Associate each character's VoiceStudio voice and images in the Avatar & voice editor.
- Select neutral, joy, sadness, anger, surprise, reflection or tenderness through the existing reply stream, without an extra model request. Remove expression metadata before display, speech and history.
- Crossfade images over half a second after loading, including the return to neutral. Coalesce rapid changes and respect reduced-motion preferences.
- Preserve per-character image overrides through restart, duplication and profile import/export (schema 6, export format 2; legacy imports remain supported).
- Replace application branding with the orbital-star logo, independently of companion portraits, and refresh the English README and screenshot.
- Add a visible Hermes /stop control and typed shortcut, confirm remote task termination and keep failed stops retryable without overlapping work. The server remains running.
- Keep local search, memory, personality and expressions bypassed for Hermes. Its avatar remains neutral.

## 0.7.1 — 2026-10-16

- Give the application its own blue-haired, starry-eyed logo across Windows icons, the tray, sidebar and favicon; companion portraits remain independent.
- Rebuild Windows icon resources whenever branding assets change, avoiding stale icons in incremental builds.
- Refresh the English README with the new logo, cyan accents, a feature overview and an updated control-panel screenshot.

## 0.7.0 — 2026-10-11

- Connect to Hermes as a voice client, with per-server profile selection and protected profile-specific API keys.
- Keep conversation, personality, memory, model selection and tools on Hermes; reuse Sherka’s avatar and VoiceStudio audio pipeline.
- Stream replies, relay one-time tool approvals, and stop server work when interrupted.
- Preserve separate sessions per companion, server and Hermes profile, including server-side context compression.
- Start a new Hermes conversation from the widget or `/new`, preserving server memory and previous conversations.
- Include connection/key setup help in seven languages. Remote-agent mode is explicitly excluded from future emotional-avatar behavior.

### Conversation fixes (prepared in 0.6.1)

- Continue ordinary conversation when automatic web search planning fails, instead of replacing the reply with a search error.
- Clarify that personal stories, daily activities and feelings do not require web search; allow more room for structured planning on thinking API models.
- Raise the default reply reserve to 8,192 tokens and upgrade the former 1,024-token profile default once, preserving other settings and history.
- Use a configurable 64,000-token context fallback when a remote provider omits its limit; keep advertised context/output limits and the actual LM Studio context authoritative.
- Explain the context fallback in all seven interface languages.

## 0.6.0 — 2026-10-11

- Per-companion OpenRouter, Z.ai API and Coding Plan, OpenAI, Anthropic and compatible API providers alongside LM Studio.
- OpenAI Chat Completions and Anthropic Messages adapters for streaming, usage and structured memory operations.
- Windows Credential Manager keys scoped to provider and endpoint, separate from profiles and exports.
- Remote context/output budgets, mandatory thinking controls and local model cleanup on cloud handover.
- Provider settings and feedback in all seven interface languages, with privacy and setup documentation.

## 0.5.1 — 2026-10-11

- Startup recovery screen replaces silent exits when settings or profile databases cannot be opened, including data last used by a newer executable.
- Signed GitHub update checks and installation remain available in recovery, without initializing voice, loading a model or resetting data.
- Read-only compatibility checks run before settings writes or profile migrations; recovery follows the saved interface language where readable.
- A local startup diagnostic records the version and error; documentation explains that installed and no-install copies share data and how to repair older releases.
- Native recovery coverage for newer settings, newer profile schemas and malformed settings; installer validation guidance now requires an isolated Windows account or VM.

## 0.5.0 — 2026-10-11

- Optional DuckDuckGo web search without an account or API key, disabled by default.
- Explicit text/voice searches and automatic local search decisions, with cancellable requests.
- Bounded search snippets within the loaded model's context budget; normal answer streaming continues.
- Source links attached to each response, retained in profile-local history and omitted from character exports.
- Search settings, progress and errors translated into all seven interface languages.
- Fixed localized notices for failed/empty searches prevent fabricated verifications; safe source links and speech without source markers or URLs.
- History schema 4 stores source receipts; upgraded data must not be opened with older binaries.
- Widget controls and companion picker now dismiss when the pointer leaves, focus is lost or the control panel opens; clicking no longer leaves controls permanently pinned.
- README showcases the independent transparent desktop widget alongside the optional control panel.

## 0.4.0 — 2026-10-11

- Seven interface languages: French, English, Spanish, Italian, German, Simplified Chinese and Japanese.
- Saved interface preference, synchronized widget controls and translated tray/window labels.
- Ready-to-use Sherka personality and instructions in all seven languages, following the interface language until personalized.
- Companion language chosen once at creation, independent of the interface; model and voice choices remain user-configured.
- Custom profile contents, histories and memories preserved when switching interface languages.
- Unicode sentence boundaries for Chinese and Japanese speech text.
- Shared catalog checks and isolated native coverage for all seven interfaces.
- Consistent dark form fields, aligned audio selections and styled sliders with live percentage values.

## 0.3.3 — 2026-10-11

First public release.

- Continuous local voice conversations with streamed text, prepared speech and interruption.
- Transparent widget and independent character models, voices and memories.
- Context budgets, reasoning controls and safe model switching/shutdown.
- Automatic startup and readiness checks for installed VoiceStudio.
- Manual signed GitHub updates for installed and directly launched copies.
- Windows installer and no-install distribution, English documentation, GPLv3 and dependency notices.

VoiceStudio is required. LM Studio is currently the only conversation provider. The application UI is in French. Avatar animation, expressions and moods remain planned.
