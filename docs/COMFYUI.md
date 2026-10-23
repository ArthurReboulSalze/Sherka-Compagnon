# Conversation illustrations

Sherka can show an illustration beside the desktop companion. Choose **Illustrious** or **SDXL** as the checkpoint family; Sherka adapts the image prompt accordingly. Illustrations are independent of avatar expressions and are unavailable in Hermes mode.

## Setup

1. Start ComfyUI in server mode on your computer or private network.
2. Open **Companions → Conversation illustrations**, enter its address and refresh the catalog.
3. Select an installed checkpoint and its **Model family**. The catalog is not filtered by filename. Use **Model folder** and **LoRA folder** to narrow it to any relative folder exposed by your server. Choose the correct family yourself: filenames do not reliably identify a model. Describe the companion's physical appearance: silhouette, colors, clothing, mechanical parts or vehicle bodywork. Built-in companions provide editable starting descriptions. Optionally enable **Use the neutral portrait with IP-Adapter** to guide scenes that include the companion. Switch it off to use text descriptions alone; this per-companion choice is independent of character creation.
4. Add optional LoRAs and adjust each strength from 0 to 2. Choose compatible LoRAs and include any useful trigger words in the physical description.
5. Choose **1–8 images per exchange** and **1–60 seconds per slide**, then save. The widget's **Image** button toggles a transparent image cell attached to the avatar. Switching it off also disables subsequent generations. Its refresh button regenerates the latest completed exchange.

The highlighted **Refresh models and LoRAs** button rescans the full catalog, including samplers. A successful refresh replaces the saved list, so added, removed and moved files are reflected. Sherka remembers these filename lists on this device, per ComfyUI server, across companion changes and application restarts. You can select models and LoRAs immediately from the saved list; refresh whenever the server's files change. A failed refresh keeps the previous list usable. The cached list does not imply that the server is currently online, and generation still checks the live server.

Drag the image or its bottom bar to move the cell relative to the avatar; use its corner handle to resize it. The cell follows the companion when the widget moves. Multiple images loop with a half-second crossfade; reduced-motion preferences skip that fade. Text and image cells share the companion's transparent window.

The image cell can grow beyond its former 600-pixel limit, up to 1600 pixels within the display's available space. **Double-click the image for full screen** on the companion's current monitor. The companion stays in front and can still be moved and resized independently. Press **Esc**, double-click again or use the close button to return to the cell. Switching companions or hiding the widget also closes full screen. Enlarging the preview does not change the generation resolution.

When opening the generator, a visible notice explains the first-image wait: ComfyUI starts if necessary, then loads the selected checkpoint and LoRAs for generation. The notice remains visible until an image is available.

The built-in workflow is derived from the supplied Sherka graph. It requires the **rgthree Power LoRA Loader**, **tinyterra text** and **MNeMiC LoRA Tag Loader** nodes, an installed compatible Illustrious or SDXL checkpoint, and the Turbo LoRA when Turbo mode is enabled. The setup page reports missing nodes.

Turbo mode automatically resolves `PAseer-SDXL-LCM+Turbo.safetensors` from ComfyUI's LoRA catalog. Its filename is not exposed as an editable LoRA slot; optional character/style LoRAs are separate. In standard mode, disabling Turbo removes it, sets **both samplers to 16 steps**, and doubles each sampler’s original CFG. The supplied graph therefore uses **3.8 and 7.0**, with `euler_ancestral` and the normal scheduler. Custom workflows double their own CFG values; their saved values are not overwritten. Compatibility and image quality depend on the checkpoint and LoRAs you choose; no model files are bundled or downloaded by Sherka.

## Optional local startup

In **Settings → Local ComfyUI startup**, select the folder containing `main.py`, or the root of a portable installation, and save. You can also select the Python executable from your ComfyUI environment, such as Conda. Sherka detects `.venv`/`venv` and portable embedded Python when available; otherwise it uses Python from the system PATH. No Python packages or models are installed automatically. Enable **Start with Sherka** to use it; disable the switch or leave the folder empty for manual startup.

These are device settings, shared between companions and excluded from companion exports. Automatic startup applies only to plain HTTP loopback addresses (`localhost`, `127.x.x.x` or `::1`). Remote servers and HTTPS proxies are contacted normally, without starting a local process; the local startup paths do not apply to them.

With illustrations enabled, Sherka reuses an existing compatible server or starts the selected installation with `--listen` bound to the configured loopback address, `--port` matching the server URL and `--disable-auto-launch`. It waits for the ComfyUI API before preparing a generation, with a bounded five-minute startup attempt. To configure an offline local server, first save its paths, check **Images during conversation**, then refresh the model list before selecting a checkpoint and saving the illustration settings.

The panel and image cell show a startup notice: the first images may take longer while ComfyUI starts and loads its models. Concurrent requests share the startup attempt. Switching off illustrations cancels Sherka’s wait; ComfyUI stays available until Sherka quits. On exit, Sherka closes the server and its children only if it launched them; a pre-existing or remote server remains independent. Launch output is appended to `logs/comfyui.log` in Sherka’s data directory. This launcher supports Python and portable installations, not the ComfyUI Desktop application executable.

## Advanced sampling and live mode

Choose a recommended **starting resolution** with its aspect ratio, then select **×1 or ×1.5 latent scaling between the two samplers** and an optional **final ×2 image upscale**. Sherka shows all three dimensions. For example: 768 × 1024 → 1152 × 1536 → 2304 × 3072. Legacy configurations keep their saved size until edited. The default conversation format remains 768 × 768.

The old bundled graph contained a disconnected ×1.5 node. The explicit latent control now wires the selected scale into the second sampler. Final upscale models come from `models/upscale_models` or configured external paths. A lightweight ×2 image model is the most direct option. ×4 image models are also usable, with their result resized to the promised ×2 dimensions. Unsupported model architectures may still be rejected by ComfyUI. Output is limited to 4096 pixels per side; larger sizes require more VRAM and time. The image cell preserves the output aspect ratio.

Enable **Advanced mode** to set steps (1–200) and CFG (0–100) independently for each pass. The shared sampler comes from the ComfyUI catalog. These settings override Turbo's standard sampling defaults. A custom graph must contain exactly two connected KSampler nodes; dependency order determines first and second pass.

**Live mode** keeps producing one fresh image at a time from the latest completed exchange. It keeps only the newest preview, with a crossfade and no image counter; batch size and slideshow duration apply to standard mode. The scene prompt is reused until a new exchange or settings change, so each variation does not call the language model again. Switching off the Image button, switching companions or quitting cancels the loop. An error stops it instead of repeatedly submitting failing jobs.

Conversation has priority. When you speak or send text, Sherka cancels its own queued image and interrupts its own running image. It requests that ComfyUI release cached GPU models while its queue is idle, then gives speech recognition, text generation and speech synthesis time to work. It resumes images after text and speech synthesis complete; already prepared audio can continue playing. Interrupted images restart as new generations, rather than resuming at the same sampling step. ComfyUI stays running.

Live mode requires **ComfyUI 0.3.57 or newer**, which supports a prompt-specific interrupt. Sherka does not send a global interrupt or stop unrelated clients' jobs. If another client is using ComfyUI, Sherka cannot reserve the whole GPU for conversation. GPU memory offloading is handled by ComfyUI; its timing depends on the running nodes and hardware.

## Model paths and image prompts

The folder fields use **relative catalog paths** such as `My collection/Landscapes` or `Styles/Watercolor`. Checkpoints and LoRAs can be organized under any names. To add folders outside ComfyUI’s usual model directories, register them on the server in [`extra_model_paths.yaml`](https://docs.comfy.org/basic-concepts/models#adding-extra-model-paths), then refresh the catalog. Sherka does not silently rewrite a remote server’s filesystem configuration. The checkpoint must still be compatible with the selected workflow family; listing a Flux or SD1.5 file does not make it SDXL-compatible.

**Negative prompt** applies to every image. **Unwanted character traits**, beside the physical description, is a separate negative prompt appended only when the companion appears in the scene—for Minerva, for example, `human face, human skin`. Landscape-only scenes do not receive these character exclusions. Both fields preserve your text, weights and intentionally empty values; the language model cannot overwrite them. **Positive prompt addition** is appended after the generated scene prompt, without model rewriting. Use tags such as `(warm light:1.2), watercolor` for Illustrious or a short visual description for SDXL. An empty field adds nothing. Leading/trailing whitespace is trimmed; internal line breaks and weighting syntax are preserved. The suffix is added once per image, after the generated text is validated, and is not sent back to the scene planner. It can intentionally add subjects or styles even to a landscape, so leave character-specific tags out if you only want them when the companion appears.

Model family, folder filters, appearance description, all prompt fields and LoRAs are saved, duplicated and exported with the companion.

## How a scene is composed

Your visual request leads the scene: the requested subject, action, clothing, pose and setting take priority over the companion's reply. Its reply fills in compatible details. For example, “show yourself dancing in a ballet outfit” asks for the companion in that outfit and pose, even if its usual description mentions armor or a serious expression. A robot keeps its mechanical face and recognizable features while changing clothes. “Can you show me the Moon?” still illustrates the Moon alone. Requests are interpreted by meaning in the conversation's language; no special command or exact trigger phrase is needed. Results depend on how well the selected language model and image checkpoint follow these details.

The companion appears **once by default**. “Show me yourself” calls for a single companion; “you and me together” calls for distinct subjects, with the avatar's appearance assigned only to the companion. The planner first writes a short scene description inside the same request, then translates that description into the image prompt. Illustrious uses `solo` for single-character scenes, without imposing it on groups or empty landscapes. These instructions reduce accidental duplicate avatars; they cannot guarantee how every checkpoint renders a composition.

As soon as the reply's **text generation finishes**, a short decision using **only the latest user message and companion reply** determines whether the companion belongs in the illustration. A landscape or unrelated topic does not automatically include a foreground character. The physical description is supplied to the scene planner only when the companion belongs in the scene. It starts while VoiceStudio finishes speaking; speech completion is not a prerequisite. For **Illustrious**, the model composes English Danbooru-style tags and concise descriptive phrases. For **SDXL**, it writes visual prose describing the subject, environment, composition and lighting. It can invent useful descriptions and combinations; there is no fixed dictionary. When the companion is depicted, its appearance description guides the result; consistency still depends on the checkpoint and prompt. Pony score tags and embedded LoRA instructions are excluded. These auxiliary requests use their own short generation budget and disable optional reasoning when the provider supports it, independently of conversation thinking. They may incur API usage with a hosted provider.

The scene prompt is separate from the companion's dialogue and memory. It does not replace the reply or reach speech synthesis. The supplied graph's text/CLIP connections are repaired before use. Each image uses one latent at the configured dimensions and a fresh seed. A batch renders sequentially, so choosing more images does not multiply the latent batch in VRAM. Completed previews appear while the remaining variants render. Sherka validates the output dimensions, removes file metadata by re-encoding images, and keeps previews in memory at the generated resolution. ComfyUI also retains its temporary output files.

Only the newest eligible exchange is allowed to publish an image. New input, a profile/configuration change, conversation clearing or shutdown invalidates older work. Pending owned prompts are deleted by ID. On ComfyUI 0.3.57 or newer, a running owned task is interrupted by its prompt ID. Older or unrecognized servers drain running tasks and cannot enable live mode. Sherka never sends a global interrupt that could stop another client's generation. ComfyUI may retain temporary images and workflow history on its server. Clearing Sherka's conversation removes its in-memory preview, not those server files.

## Custom workflows

In **Companions → Conversation illustrations → ComfyUI workflow**, click **Export API workflow** to save the active template. It exports your imported API graph when present, otherwise the [bundled API template](../config/workflows/illustrious.json). Open that JSON in ComfyUI, adapt it, export in API format, then import it back into Sherka. Export works without connecting to ComfyUI. The template contains substitution fields; the actual model, LoRAs, sampler settings and scene text are injected at generation time. It does not contain conversation history or API keys.

Import **API-format JSON**, not ComfyUI's visual editor JSON. This first contract supports the same Illustrious/SDXL graph family as the bundled template: one checkpoint loader, one rgthree Power LoRA Loader, optional tinyterra/MNeMiC prompt nodes, CLIP encoders, latent sampling/decoding and an image output. The graph is bounded and rejects unrelated execution, download, file-loading and external-resource nodes. Images use ComfyUI's temporary preview output even when the imported graph contains SaveImage.

Sherka supplies the checkpoint, selected LoRAs, prompt text, canvas dimensions and fresh seed. A negative encoder is identified by `%negative_prompt%` or a title containing `negative`; other text encoders receive the positive prompt. The default graph demonstrates this wiring. Keep the workflow file and any earlier exported images yourself if you need a persistent archive.

References: [ComfyUI targeted interrupt](https://github.com/Comfy-Org/ComfyUI/commit/464ba1d6140eda6a0173703ac00c69f7fddab6ba), [ComfyUI server routes](https://docs.comfy.org/development/comfyui-server/comms_routes), [Illustrious author's model card](https://huggingface.co/OnomaAIResearch/Illustrious-XL-v1.0).

## View and edit the current prompt

The pencil button in the desktop image cell opens, just below the image and attached to it, the exact prompts of the latest scene: the planned prompt with the positive addition, the negative prompt and the companion's unwanted traits when it appears. Edit them and choose **Generate** to render that exchange again with your text, sent verbatim. The edited prompt is reused for further variants and live mode until the next reply, which plans a new scene. **Automatic** discards the edit and plans the scene again. Hermes mode has no illustrations.

## Settings layout

Illustration settings are grouped into collapsible sections: connection and model, companion appearance, prompts, LoRA, format and quality, visual reference, sampling, generation during conversation and the workflow. Collapsed sections are remembered on this device. The LoRA section has its own folder filter and a search field right above the LoRA rows; both filters only narrow the lists, and an already selected LoRA stays selected.

## IP-Adapter reference

Enable **Use neutral portrait with IP-Adapter** on a companion and select an SDXL adapter and its matching CLIP Vision encoder. Sherka uses the neutral image only when the scene planner includes the companion; unrelated landscapes do not receive it. The portrait is normalized, flattened onto a neutral background for CLIP, and uploaded to ComfyUI's temporary directory. The saved avatar retains its transparency.

Disable the same switch for text-only image guidance. The model selections and strength stay saved for later, while Sherka stops uploading the portrait and removes its IP-Adapter conditioning from the generated workflow. Character creation uses a separate Klein image-editing workflow without IP-Adapter.

Install [ComfyUI_IPAdapter_plus](https://github.com/cubiq/ComfyUI_IPAdapter_plus). Put adapters in `models/ipadapter` and vision weights in `models/clip_vision`. Standard `ip-adapter_sdxl.safetensors` uses CLIP ViT-bigG; use ViT-H for adapters designed for ViT-H. Sherka excludes FaceID and SD1.5 adapters from this SDXL workflow. Refresh the catalog after adding files. Model filenames and strength are saved per companion. Lower strength allows larger pose/expression changes; higher values favor similarity. Identity remains probabilistic, especially with stylized checkpoints or unusual anatomy.

## Character creation

Open **Companions → Create with AI**, or **Character creation** in the sidebar. Select a classic companion whose connected LLM will prepare the character. Only your design concept is sent; existing conversations and memories are not included. The generated name, personality and instructions use the chosen character language. Image descriptions use English tags for Illustrious or visual prose for SDXL, and remain editable.

Creation runs three bundled API workflows with **fixed settings and no Turbo or IP-Adapter**. Select the portrait checkpoint and family, then the Klein diffusion model, compatible text encoder and VAE. Conversation illustration settings remain independent.

Portraits use a **straight-on front view of the head and shoulders / upper chest**, or a head-only portrait when specified, with the entire head visible. Neutral and emotional portraits share this framing to make expressions easier to read. Legacy full-body and side-view clauses in the portrait field are ignored during generation; physical traits remain intact. Creation adds framing exclusions to the negative prompt, including full-body, cropped-head, profile and three-quarter views. Regenerate and approve the neutral portrait before recreating its emotions. These constraints do not restrict conversation illustrations.

1. Generate the profile and refine personality, physical traits, unwanted traits, framing and expression descriptions.
2. **Neutral portrait:** the [neutral workflow](../config/workflows/character-neutral.json) renders on solid dark gray at 1024², runs the supplied 20-step base/refiner split (15 + 5), enlarges the latent by ×1.5, performs the 25-step detail pass, then resizes to 1024². The checkpoint supplies its text encoder and VAE. Explicitly approve this image before proceeding.
3. **Emotions:** the [Klein workflow](../config/workflows/character-mood.json) edits the approved neutral with reference latents, 20 steps, Euler and CFG 5. Generate or retry each emotion separately, or fill in missing emotions together. English editing instructions preserve the character, materials, clothing, front view, framing and gray background. Every edit starts from the neutral portrait, avoiding cumulative alterations.
4. **Transparency:** once all seven portraits look right, apply the [fixed RMBG workflow](../config/workflows/character-transparency.json). It uses RMBG-2.0 at 1024 processing resolution, sensitivity 1, no mask blur, mask offset −2, foreground refinement and the gray color preset. The resulting mask supplies the alpha channel of the original image; no diffusion regeneration takes place here. There are no settings for this stage.
5. Click any original or transparent portrait to inspect it in a large preview. Review all seven transparent images and choose **Add to companions**. The character starts with an empty conversation and memory; select its own VoiceStudio voice in Companions. Conversation illustrations and IP-Adapter start disabled.

### Start from your own image

Choose **Start from an image** next to your idea, or **Use my image** in the neutral portrait step, to supply a PNG, JPEG or WebP picture under 10 MiB. Sherka scales it to fit 1024 pixels, flattens any transparency onto the same plain dark gray as generated portraits and uses it as the neutral portrait to approve. The written idea becomes optional. When the connected model supports images, it also receives the picture to write the physical description and unwanted traits from what it actually shows; otherwise the sheet comes from your text and a notice asks you to check the description. An imported portrait stays valid when you later edit the sheet or change the portrait checkpoint; the Klein emotions and transparency stages then work as usual.

### Regenerate an existing companion's images

On a companion's page, **Regenerate images** opens the studio with that companion's current portrait and seven expressions, bundled artwork included. The current portrait counts as already approved, so you can regenerate a single emotion right away, or generate a new neutral portrait (which resets the emotions). Apply transparency to the new images, then **Apply to ⟨name⟩** replaces the companion's seven images. Its personality, voice, history and memories are not changed; the sheet's identity fields are only shown for reference.

### Edit the exact prompts

Each image card has a **Prompt** button. It shows the exact positive and negative prompts ComfyUI will receive for that image: for the neutral portrait, the composed SDXL or Illustrious prompt; for an emotion, the Klein editing instruction. Edit them and choose **Use this prompt** to send your text verbatim for that image only, or **Back to the automatic prompt** to compose it from the sheet again. An edited prompt no longer follows later sheet edits, and changing it invalidates only the image it belongs to. Workflow sampling settings stay fixed.

Changing the neutral model, physical description, unwanted traits or framing requires a new portrait and approval. Editing the name or personality does not. Changing Klein models invalidates dependent emotions; changing one expression or regenerating it invalidates only that expression's transparent result. Background removal retries only missing images after a partial failure. Regenerating the neutral resets all dependent images.

The draft survives navigation during the current app session. It is not a saved companion until the final step. **Stop** cancels Sherka's job; a conversation also preempts creation. Completed draft images remain available to review. This does not stop ComfyUI or unrelated clients' work.

Required nodes and models:

- Neutral: **rgthree**, **tinyterraNodes**, **ComfyUI-MNeMiC-Nodes**, **ComfyUI-Easy-Use**, and an SDXL-family checkpoint.
- Emotions: a ComfyUI version with **Flux 2 Klein** support, including `ReferenceLatent` and `Flux2Scheduler`. Place diffusion weights in `models/diffusion_models`, the text encoder in `models/text_encoders`, and the VAE in `models/vae`. The supplied setup uses `flux-2-klein-4b-fp8.safetensors`, `qwen3vl_4b_fp8_scaled.safetensors` and `flux2-vae.safetensors`; choose compatible files from your own catalog.
- Transparency: **[ComfyUI-RMBG](https://github.com/1038lab/ComfyUI-RMBG)** and its **RMBG-2.0** weights. The node manages these weights and may download them on first use if absent. There is no model picker for this stage.

The model scan reports missing node classes. Model choices and the scanned catalog are cached locally across restarts; refresh to discover additions. Sherka does not install custom nodes or model weights itself.

**View workflow** shows each built-in API graph read-only, with model/prompt/image placeholders. Creation currently exposes model choices and editable descriptions only, with no sampler, size or mask controls. Conversation illustrations retain their separate custom-workflow import/export tools.

Intermediate portraits may be opaque. Final avatars require usable alpha; an opaque or empty result is rejected. A uniform background helps removal but does not guarantee perfect extraction. Image quality, alpha edges and expression fidelity still require visual review.

**Help & about**, at the bottom of the sidebar, summarizes requirements and links to upstream installation instructions.
