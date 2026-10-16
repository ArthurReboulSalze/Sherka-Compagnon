<p align="center">
  <img src="public/branding/sherka-logo.png" alt="Sherka Companion — blue-haired AI with starry eyes and futuristic headphones" width="200">
</p>

<h1 align="center">Sherka Companion</h1>
<p align="center"><strong>A voice. A personality. A place on your desktop.</strong></p>
<p align="center">Meet your AI companions. Talk naturally, interrupt freely, and pick up where you left off.</p>
<p align="center"><em>Your models. Your voices. Your companion.</em></p>

<p align="center">
  <img src="docs/images/features.svg" alt="Windows x64 · Local voice · 7 languages · GPLv3" width="492">
</p>

<p align="center">
  <a href="https://github.com/ArthurReboulSalze/Sherka-Compagnon/releases/latest"><strong>↓ Download for Windows</strong></a> &nbsp;·&nbsp;
  <a href="docs/GETTING_STARTED.md">Quick start</a> &nbsp;·&nbsp;
  <a href="docs/README.md">Documentation</a> &nbsp;·&nbsp;
  <a href="docs/ROADMAP.md">What's next</a>
</p>

---

<p align="center">
  <img src="docs/images/sherka.png" alt="Sherka's control panel: character portrait, personality and companion settings" width="1000">
</p>
<p align="center"><em>Your companion on the desktop. Everything you need in the control panel.</em></p>

## A conversation that keeps up with you

Sherka gives your AI a presence beyond a chat window: a transparent companion that lives on your desktop, with a voice and a personality of its own. Run conversations locally, connect your favorite model API, or talk directly to your Hermes agent.

| 🎙️ Talk naturally | ✨ Make it personal | 🪟 Keep it close |
| --- | --- | --- |
| Streamed replies, speech prepared ahead, and voice interruption. Keep the conversation flowing. | A personality, image, voice and provider for every companion. Each keeps its own history. | An independent transparent avatar. Move it anywhere; controls appear on hover or click. |
| **🧠 Memories you control** | **🌐 A little help from the web** | **💬 Seven interface languages** |
| Review, edit or correct what your companion remembers, in the interface or by asking. | Optional [DuckDuckGo search](docs/WEB_SEARCH.md) with source links. No search account or key required. | French, English, Spanish, Italian, German, Simplified Chinese and Japanese. |

The built-in Sherka is ready in every interface language. Custom companions keep the language you choose for them.

## Local voice. Your choice of intelligence.

**[VoiceStudio](https://github.com/debpalash/VoiceStudio) powers speech recognition, synthesis and voices.** Install it and your preferred speech models separately; Sherka starts the installed Windows application when needed.

| How you want to talk | What you connect |
| --- | --- |
| **Keep conversations local** | [LM Studio](https://lmstudio.ai/) and downloaded models. Conversation can work offline. |
| **Choose a hosted model** | OpenRouter, Z.ai API or Coding Plan, OpenAI, Anthropic, or a compatible API. [Provider setup](docs/PROVIDERS.md). |
| **Talk to your Hermes agent** | Your Hermes server and profile. Hermes keeps its personality, memory and tools; Sherka handles the avatar and voice. [Hermes setup](docs/HERMES.md). |

Voice processing stays local. Hosted model APIs receive conversation context and may charge for usage; Hermes follows your server's configuration. API keys are protected by Windows and excluded from exported profiles.

Sherka manages context budgets and offers reasoning controls where supported. Switching companions also handles local model loading and unloading.

## From download to your first conversation

1. **Pick your download:** the [Windows x64 installer or no-install ZIP](https://github.com/ArthurReboulSalze/Sherka-Compagnon/releases/latest).
2. **Connect voice and intelligence:** set up VoiceStudio, then choose LM Studio, an API provider or Hermes in Settings.
3. **Choose your companion and voice, turn on the microphone, and talk.** The avatar appears at startup; the control panel is there whenever you need it.

Follow the [getting-started guide](docs/GETTING_STARTED.md) for setup. WebView2 and the Visual C++ x64 runtime are required. Models and voices are separate downloads. Headphones are recommended for voice interruption; the microphone is off at startup.

Check for signed updates directly from **Settings**. Both editions keep profiles and memories in your Windows user folder.

## What's next

More ways to make companions feel alive: avatar customization, speaking and listening animations, expressions and moods. Hermes keeps a neutral avatar and stays outside the future emotion system.

Sherka is an early project. Speed depends on your hardware and model choices, and speaker echo handling still needs broader testing. See the [roadmap](docs/ROADMAP.md) and [current status](docs/PROJECT_STATE.md).

## Open source, made to grow

Created by [Arthur Reboul Salze](https://github.com/ArthurReboulSalze). Sherka's code is **[GPLv3](LICENSE)**; dependencies retain their [own licenses](THIRD_PARTY_NOTICES.md).

The retrofuturistic artwork is an original, freely inspired creation. No Ulysses 31 artwork is distributed, and the project is unaffiliated with the series.

Have an idea? [Open an issue](https://github.com/ArthurReboulSalze/Sherka-Compagnon/issues), explore the [documentation](docs/README.md), or [build and contribute](docs/DEVELOPMENT.md).
