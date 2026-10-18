# Conversation illustrations

Sherka can show a square illustration beside the desktop companion. This first connector targets **Illustrious-family models**. Other SDXL checkpoints are a later phase. Illustrations are independent of avatar expressions and are unavailable in Hermes mode.

## Setup

1. Start ComfyUI in server mode on your computer or private network.
2. Open **Companions → Conversation illustrations**, enter its address and refresh the catalog.
3. Select an installed Illustrious checkpoint. Describe the companion's physical appearance: silhouette, colors, clothing, mechanical parts or vehicle bodywork. Built-in companions provide editable starting descriptions. There is no reference-image conditioning in this version.
4. Add optional LoRAs and adjust each strength from 0 to 2. Choose compatible LoRAs and include any useful trigger words in the physical description.
5. Choose **1–8 images per exchange** and **1–60 seconds per slide**, then save. The widget's **Image** button toggles a transparent image cell attached to the avatar. Switching it off also disables subsequent generations. Its refresh button regenerates the latest completed exchange.

Drag the image or its bottom bar to move the cell relative to the avatar; use its corner handle to resize it. The cell follows the companion when the widget moves. Multiple images loop with a half-second crossfade; reduced-motion preferences skip that fade. Text and image cells share the companion's transparent window.

The built-in workflow is derived from the supplied Sherka graph. It requires the **rgthree Power LoRA Loader**, **tinyterra text** and **MNeMiC LoRA Tag Loader** nodes, an installed Illustrious checkpoint, and the Turbo LoRA when Turbo mode is enabled. The setup page reports missing nodes.

Turbo mode automatically resolves `PAseer-SDXL-LCM+Turbo.safetensors` from ComfyUI's LoRA catalog. Its filename is not exposed as an editable LoRA slot; optional character/style LoRAs are separate. Disabling Turbo removes it and switches the supplied samplers to a normal step/guidance schedule. Compatibility and image quality depend on the checkpoint and LoRAs you choose; no model files are bundled or downloaded by Sherka.

## How a scene is composed

As soon as the reply's **text generation finishes**, a separate bounded request to the selected conversation model uses **only the latest user message, companion reply and physical description**. It starts while VoiceStudio finishes speaking; speech completion is not a prerequisite. The model composes English Danbooru-style tags and concise descriptive phrases for Illustrious. It can invent useful descriptions and combinations; there is no fixed dictionary. Appearance stays stable while pose, setting, lighting and composition follow the exchange. Pony score tags and embedded LoRA instructions are excluded. This extra model request may incur API usage when you use a hosted provider.

The scene prompt is separate from the companion's dialogue and memory. It does not replace the reply or reach speech synthesis. The supplied graph's text/CLIP connections are repaired before use. Each image uses one 768×768 latent and a fresh seed. A batch renders sequentially, so choosing more images does not multiply the latent batch in VRAM. Completed previews appear while the remaining variants render. Sherka validates the square images, removes file metadata by re-encoding them, and keeps the previews in memory.

Only the newest eligible exchange is allowed to publish an image. New input, a profile/configuration change, conversation clearing or shutdown invalidates older work. Pending owned prompts are deleted by ID. An already running ComfyUI task may finish in the background; Sherka never sends a global interrupt that could stop another client's generation. ComfyUI may retain temporary images and workflow history on its server. Clearing Sherka's conversation removes its in-memory preview, not those server files.

## Custom workflows

Import **API-format JSON**, not ComfyUI's visual editor JSON. This first contract supports the same Illustrious graph family as the bundled template: one checkpoint loader, one rgthree Power LoRA Loader, optional tinyterra/MNeMiC prompt nodes, CLIP encoders, latent sampling/decoding and an image output. The graph is bounded and rejects unrelated execution, download, file-loading and external-resource nodes. Images use ComfyUI's temporary preview output even when the imported graph contains SaveImage.

Sherka supplies the checkpoint, selected LoRAs, prompt text, square canvas and fresh seed. A negative encoder is identified by `%negative_prompt%` or a title containing `negative`; other text encoders receive the positive prompt. The default graph demonstrates this wiring. Keep the workflow file and any earlier exported images yourself if you need a persistent archive.

References: [ComfyUI server routes](https://docs.comfy.org/development/comfyui-server/comms_routes), [Illustrious author's model card](https://huggingface.co/OnomaAIResearch/Illustrious-XL-v1.0).
