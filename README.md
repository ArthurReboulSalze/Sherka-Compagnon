<p align="center">
  <img src="docs/images/sherka-logo-gold-v2.png" alt="Sherka Companion — golden orbital star emblem" width="200">
</p>

<h1 align="center">Sherka Companion</h1>
<p align="center"><strong>A voice. A personality. A place on your desktop.</strong></p>
<p align="center">Talk naturally. Interrupt freely. Give your AI a place on your desktop.</p>
<p align="center"><em>Your models. Your voices. Your companion.</em></p>

<p align="center">
  <img src="docs/images/features-gold.svg" alt="Windows &amp; Linux · Local voice · 7 languages · GPLv3" width="492">
</p>

<p align="center">
  <a href="https://github.com/ArthurReboulSalze/Sherka-Compagnon/releases/latest"><strong>↓ Download for Windows &amp; Linux</strong></a> &nbsp;·&nbsp;
  <a href="docs/GETTING_STARTED.md">Quick start</a> &nbsp;·&nbsp;
  <a href="docs/README.md">Documentation</a> &nbsp;·&nbsp;
  <a href="docs/ROADMAP.md">What's next</a>
</p>

---

<p align="center">
  <img src="docs/images/sherka-ecosystem-en.png" alt="Sherka ecosystem: local or hosted language models, VoiceStudio speech, and ComfyUI visuals" width="1200">
</p>
<p align="center"><em>Conversation illustrations are available now. ComfyUI avatar-pack generation is planned.</em></p>

<p align="center">
  <img src="docs/images/desktop-companion.png" alt="Independent Sherka desktop companion with inline text input and a movable ComfyUI conversation illustration" width="900">
</p>
<p align="center"><em>A companion that lives on your desktop — with text and image cells right beside it.</em></p>

<p align="center">
  <img src="docs/images/sherka-0.12.0.png" alt="Sherka 0.12.0 control panel: golden app logo, five companions and avatar settings" width="1000">
</p>
<p align="center"><em>Your companion on the desktop. Everything you need in the control panel.</em></p>

**New in 0.12.1:** illustrations follow **your scene, outfit and action** more closely. A single companion is requested by default to reduce accidental doubles, while group scenes keep distinct subjects. SDXL descriptions now reach ComfyUI without being cut into short tags. [Release notes](docs/releases/0.12.1.md).

## Meet your companions

Choose your company, then give each character its own voice and model. Five bundled companions, expressive avatars and all character settings together in **Companions**.

<table>
  <tr>
    <td align="center"><img src="public/assets/sherka.png" alt="Sherka" width="120"><br><strong>Sherka</strong><br>Calm, warm, attentive.</td>
    <td align="center"><img src="public/assets/botsy/neutral.png" alt="Botsy" width="120"><br><strong>Botsy</strong><br>Your curious little tinkerer.</td>
    <td align="center"><img src="public/assets/alba/neutral.png" alt="Alba" width="120"><br><strong>Alba</strong><br><em>White &amp; gold android.</em><br>Playful, lively, adventurous.</td>
    <td align="center"><img src="public/assets/kit/neutral.png" alt="Kit" width="120"><br><strong>Kit</strong><br>Precision and deadpan wit.</td>
    <td align="center"><img src="public/assets/minerva-v2/neutral.png" alt="Minerva" width="120"><br><strong>Minerva</strong><br><em>Armored android.</em><br>Fiery, impatient, sharp-tongued.</td>
  </tr>
</table>

**😐 Neutral · 😊 Joy · 😢 Sadness · 😠 Anger · 😮 Surprise · 🤔 Reflection · 🥰 Tenderness**

Expressions follow the tone of a reply and blend with a gentle **half-second crossfade**. Use the bundled images or assign your own in **Avatar & voice**, alongside that character's VoiceStudio voice. Short examples help small models choose an expression; a conservative fallback reads the visible reply when the marker is missing. [See how it works](docs/USER_GUIDE.md#how-expressions-change).

Hermes keeps a neutral avatar and its own personality, memory and tools. Its **/stop** button stops the current task while leaving the server running.

## A conversation that keeps up with you

Sherka gives your AI a presence beyond a chat window: a transparent companion that lives on your desktop, with a voice and a personality of its own. Run conversations locally, connect your favorite model API, or talk directly to your Hermes agent.

| 🎙️ Talk naturally | ✨ Make it personal | 🪟 Keep it close |
| --- | --- | --- |
| Interrupt a spoken reply, pause while talking, then continue. Streaming and speech prepared ahead keep the exchange flowing. | A personality, image, voice and provider for every companion. Each keeps its own history. | An independent transparent avatar. Move it anywhere; controls appear on hover or click. |
| **🧠 Memories you control** | **🌐 A little help from the web** | **💬 Seven interface languages** |
| Persistent memory for each companion. Review, edit or delete facts; preview built-in memory optimization before applying it. Clear a conversation with confirmation while keeping saved facts. | [Search the web with sources](docs/WEB_SEARCH.md). Use DuckDuckGo without an account, or Brave, Tavily or SearXNG. Drop text files and, with a vision-capable model, images onto your companion. | French, English, Spanish, Italian, German, Simplified Chinese and Japanese. |

The built-in Sherka is ready in every interface language. Custom companions keep the language you choose for them.

## Turn conversations into scenes

Connect [ComfyUI](docs/COMFYUI.md), choose an **Illustrious or SDXL model**, describe the character's appearance and add your favorite LoRAs. Each companion keeps its model family, folders, editable negative prompt and LoRAs. A **positive prompt addition** appends your style, lighting or composition preferences to every generated scene, exactly as written. Landscapes can stand alone; the avatar appears when it belongs in the scene. Images start preparing as soon as the reply text is ready, while the voice continues.

**Your request sets the scene.** Ask your robot companion to dance in a ballet outfit, and its usual armor or serious expression should not freeze the result into the same portrait. The planner prioritizes your requested action, clothes and setting, uses the reply for compatible details, and distinguishes one companion from a scene involving both of you. Prompt and character fidelity still depend on your chosen models and LoRAs.

The **Image** button reveals a movable, resizable cell that follows your companion. Generate up to eight variations and let them loop with gentle crossfades at your chosen pace. The **Text** button reveals a transparent input just beneath the controls: type, paste or drop text, and attach images when your model supports vision.

These scenes illustrate the conversation; the avatar's seven expressions remain separate. Hermes uses its own tools and stays outside local scene generation.

## Create, duplicate, share

Make a copy of a companion to try a new personality, model or voice with an empty history. Export it as a **.sherka.zip** pack: personality, instructions, settings and all seven expression images travel together. Importing creates an independent companion; conversations, memories and API keys stay private.

The [open archive format](docs/COMPANION_PACKS.md) uses ordinary ZIP, JSON and PNG files. Older Sherka JSON profiles still import. Model and voice references are included; their weights and audio files are separate downloads. Check connection settings on the receiving machine.

## Local voice. Your choice of intelligence.

**[VoiceStudio](https://github.com/debpalash/VoiceStudio) powers speech recognition, synthesis and voices.** Install it and your preferred speech models separately; Sherka starts the installed Windows application when needed.

| How you want to talk | What you connect |
| --- | --- |
| **Keep conversations local** | [LM Studio](https://lmstudio.ai/) and downloaded models. Conversation can work offline. |
| **Choose a hosted model** | OpenRouter, Z.ai API or Coding Plan, OpenAI, Anthropic, or a compatible API. [Provider setup](docs/PROVIDERS.md). |
| **Talk to your Hermes agent** | Your Hermes server and profile. Hermes keeps its personality, memory and tools; Sherka handles the avatar and voice. [Hermes setup](docs/HERMES.md). |

Voice processing stays local. Hosted model APIs receive conversation context and may charge for usage; Hermes follows your server's configuration. API keys use Windows Credential Manager or Linux Secret Service and are excluded from exported profiles.

Sherka manages context budgets and offers reasoning controls where supported. Switching companions also handles local model loading and unloading.

## Responsive without a huge setup

With suitable local language and speech models, Sherka can feel very responsive: text streams in, speech is prepared ahead, and new image scenes start while the voice is still speaking. There is no fixed latency promise — model size, context, voice engine and concurrent image generation all matter.

Development and real-service testing used an **NVIDIA RTX 3090**. You do not need that much VRAM to start: **8 GB, and potentially less with small quantized models**, can suit a voice-companion setup. Running a large language model, speech and ComfyUI together needs more headroom; hosted conversation providers are also available.

## From download to your first conversation

1. **Pick your download:** the [Windows x64 installer, no-install ZIP or Debian x64 package](https://github.com/ArthurReboulSalze/Sherka-Compagnon/releases/latest).
2. **Connect voice and intelligence:** set up VoiceStudio, then choose LM Studio, an API provider or Hermes in Companions.
3. **Choose your companion and voice, turn on the microphone, and talk.** The avatar appears at startup; the control panel is there whenever you need it.

Follow the [getting-started guide](docs/GETTING_STARTED.md) for Windows or the [Linux guide](docs/LINUX.md) for Debian. Windows requires WebView2 and the Visual C++ x64 runtime. Models and voices are separate downloads. Headphones are recommended for voice interruption; the microphone is off at startup.

Check for signed Windows updates directly from **Settings**. Installed and no-install copies share your Windows user data. Linux uses its own user data folder and manual package updates; start external voice/model services separately. This first Linux port has compilation and headless startup validation; physical audio and desktop behavior need broader testing.

## What's next

Next come speaking/listening animations, richer character customization and consistent avatar packs through **ComfyUI**. Conversation illustrations are available now; generating the avatar's seven expressions is a separate future step. Hermes remains outside the emotion and illustration systems.

Sherka is an early project. Speed depends on your hardware and model choices, and speaker echo handling still needs broader testing. See the [roadmap](docs/ROADMAP.md) and [current status](docs/PROJECT_STATE.md).

## Open source, made to grow

Created by [Arthur Reboul Salze](https://github.com/ArthurReboulSalze). Sherka's code is **[GPLv3](LICENSE)**; dependencies retain their [own licenses](THIRD_PARTY_NOTICES.md).

The retrofuturistic artwork is an original, freely inspired creation. No Ulysses 31 artwork is distributed, and the project is unaffiliated with the series.

Have an idea? [Open an issue](https://github.com/ArthurReboulSalze/Sherka-Compagnon/issues), explore the [documentation](docs/README.md), or [build and contribute](docs/DEVELOPMENT.md).
