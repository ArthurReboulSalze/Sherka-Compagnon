# Conversation illustrations

Sherka can show a square illustration beside the desktop companion. Choose **Illustrious** or **SDXL** as the checkpoint family; Sherka adapts the image prompt accordingly. Illustrations are independent of avatar expressions and are unavailable in Hermes mode.

## Setup

1. Start ComfyUI in server mode on your computer or private network.
2. Open **Companions → Conversation illustrations**, enter its address and refresh the catalog.
3. Select an installed checkpoint and its **Model family**. The catalog is not filtered by filename. Use **Model folder** and **LoRA folder** to narrow it to any relative folder exposed by your server. Choose the correct family yourself: filenames do not reliably identify a model. Describe the companion's physical appearance: silhouette, colors, clothing, mechanical parts or vehicle bodywork. Built-in companions provide editable starting descriptions. There is no reference-image conditioning in this version.
4. Add optional LoRAs and adjust each strength from 0 to 2. Choose compatible LoRAs and include any useful trigger words in the physical description.
5. Choose **1–8 images per exchange** and **1–60 seconds per slide**, then save. The widget's **Image** button toggles a transparent image cell attached to the avatar. Switching it off also disables subsequent generations. Its refresh button regenerates the latest completed exchange.

Drag the image or its bottom bar to move the cell relative to the avatar; use its corner handle to resize it. The cell follows the companion when the widget moves. Multiple images loop with a half-second crossfade; reduced-motion preferences skip that fade. Text and image cells share the companion's transparent window.

The built-in workflow is derived from the supplied Sherka graph. It requires the **rgthree Power LoRA Loader**, **tinyterra text** and **MNeMiC LoRA Tag Loader** nodes, an installed compatible Illustrious or SDXL checkpoint, and the Turbo LoRA when Turbo mode is enabled. The setup page reports missing nodes.

Turbo mode automatically resolves `PAseer-SDXL-LCM+Turbo.safetensors` from ComfyUI's LoRA catalog. Its filename is not exposed as an editable LoRA slot; optional character/style LoRAs are separate. Disabling Turbo removes it, sets **both samplers to 16 steps**, and doubles each sampler’s original CFG. The supplied graph therefore uses **3.8 and 7.0**, with `euler_ancestral` and the normal scheduler. Custom workflows double their own CFG values; their saved values are not overwritten. Compatibility and image quality depend on the checkpoint and LoRAs you choose; no model files are bundled or downloaded by Sherka.

## Model paths and image prompts

The folder fields use **relative catalog paths** such as `My collection/Landscapes` or `Styles/Watercolor`. Checkpoints and LoRAs can be organized under any names. To add folders outside ComfyUI’s usual model directories, register them on the server in [`extra_model_paths.yaml`](https://docs.comfy.org/basic-concepts/models#adding-extra-model-paths), then refresh the catalog. Sherka does not silently rewrite a remote server’s filesystem configuration. The checkpoint must still be compatible with the selected workflow family; listing a Flux or SD1.5 file does not make it SDXL-compatible.

**Negative prompt** is editable per companion and is sent to the negative CLIP encoder exactly as saved, including an intentionally empty field. The language model cannot overwrite it. Add unwanted traits there when useful—for Minerva, for example, `human face, human skin`. **Positive prompt addition** is appended after the generated scene prompt, without model rewriting. Use tags such as `(warm light:1.2), watercolor` for Illustrious or a short visual description for SDXL. An empty field adds nothing. Leading/trailing whitespace is trimmed; internal line breaks and weighting syntax are preserved. The suffix is added once per image, after the generated text is validated, and is not sent back to the scene planner. It can intentionally add subjects or styles even to a landscape, so leave character-specific tags out if you only want them when the companion appears.

Model family, folder filters, both prompt fields and LoRAs are saved, duplicated and exported with the companion.

## How a scene is composed

Your visual request leads the scene: the requested subject, action, clothing, pose and setting take priority over the companion's reply. Its reply fills in compatible details. For example, “show yourself dancing in a ballet outfit” asks for the companion in that outfit and pose, even if its usual description mentions armor or a serious expression. A robot keeps its mechanical face and recognizable features while changing clothes. “Can you show me the Moon?” still illustrates the Moon alone. Requests are interpreted by meaning in the conversation's language; no special command or exact trigger phrase is needed. Results depend on how well the selected language model and image checkpoint follow these details.

The companion appears **once by default**. “Show me yourself” calls for a single companion; “you and me together” calls for distinct subjects, with the avatar's appearance assigned only to the companion. The planner first writes a short scene description inside the same request, then translates that description into the image prompt. Illustrious uses `solo` for single-character scenes, without imposing it on groups or empty landscapes. These instructions reduce accidental duplicate avatars; they cannot guarantee how every checkpoint renders a composition.

As soon as the reply's **text generation finishes**, a short decision using **only the latest user message and companion reply** determines whether the companion belongs in the illustration. A landscape or unrelated topic does not automatically include a foreground character. The physical description is supplied to the scene planner only when the companion belongs in the scene. It starts while VoiceStudio finishes speaking; speech completion is not a prerequisite. For **Illustrious**, the model composes English Danbooru-style tags and concise descriptive phrases. For **SDXL**, it writes visual prose describing the subject, environment, composition and lighting. It can invent useful descriptions and combinations; there is no fixed dictionary. When the companion is depicted, its appearance description guides the result; consistency still depends on the checkpoint and prompt. Pony score tags and embedded LoRA instructions are excluded. These auxiliary requests use their own short generation budget and disable optional reasoning when the provider supports it, independently of conversation thinking. They may incur API usage with a hosted provider.

The scene prompt is separate from the companion's dialogue and memory. It does not replace the reply or reach speech synthesis. The supplied graph's text/CLIP connections are repaired before use. Each image uses one 768×768 latent and a fresh seed. A batch renders sequentially, so choosing more images does not multiply the latent batch in VRAM. Completed previews appear while the remaining variants render. Sherka validates the square images, removes file metadata by re-encoding them, and keeps the previews in memory.

Only the newest eligible exchange is allowed to publish an image. New input, a profile/configuration change, conversation clearing or shutdown invalidates older work. Pending owned prompts are deleted by ID. An already running ComfyUI task may finish in the background; Sherka never sends a global interrupt that could stop another client's generation. ComfyUI may retain temporary images and workflow history on its server. Clearing Sherka's conversation removes its in-memory preview, not those server files.

## Custom workflows

Import **API-format JSON**, not ComfyUI's visual editor JSON. This first contract supports the same Illustrious/SDXL graph family as the bundled template: one checkpoint loader, one rgthree Power LoRA Loader, optional tinyterra/MNeMiC prompt nodes, CLIP encoders, latent sampling/decoding and an image output. The graph is bounded and rejects unrelated execution, download, file-loading and external-resource nodes. Images use ComfyUI's temporary preview output even when the imported graph contains SaveImage.

Sherka supplies the checkpoint, selected LoRAs, prompt text, square canvas and fresh seed. A negative encoder is identified by `%negative_prompt%` or a title containing `negative`; other text encoders receive the positive prompt. The default graph demonstrates this wiring. Keep the workflow file and any earlier exported images yourself if you need a persistent archive.

References: [ComfyUI server routes](https://docs.comfy.org/development/comfyui-server/comms_routes), [Illustrious author's model card](https://huggingface.co/OnomaAIResearch/Illustrious-XL-v1.0).
