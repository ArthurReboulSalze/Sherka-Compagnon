# User guide

## Clear conversations and manage saved facts

In **Conversation**, choose **Clear conversation**. The confirmation keeps saved facts by default: facts entered manually, requested explicitly, or approved with **Remember this** survive. Messages, source quotations, automatic suggestions, summaries and searchable archives are removed for that companion. Select **Also delete saved facts** only when you want those removed too. The companion's personality, images, voice and model settings stay intact.

You can also say **“Clear our conversation”** or type **`/clear`**. Sherka asks for confirmation: say **“confirm deletion”** or type **`/confirm-clear`** within three minutes. Use **`/cancel`** to cancel. Another ordinary message, a companion switch or restarting the app cancels the pending request. Confirmation applies only to the current companion and conversation. This preserves saved facts and runs locally without asking the language model to decide what to delete. The spoken acknowledgement does not create a new history entry. Requests must be explicit; quoted instructions and negated examples do not trigger deletion. Supported examples include “Efface notre conversation”, “Borra nuestra conversación”, “Cancella la nostra conversazione”, “Lösche unseren Chatverlauf”, “清空我们的对话” and “会話履歴を削除して”.

In **Memory**, **Delete this fact** removes an individual saved fact, including from future memory exports. Earlier exchanges are excluded from recall so the fact is not immediately reconstructed. Clear the conversation as well if you also want its historical messages removed. Existing exports or backups are not modified.

For **Hermes**, the panel action clears only Sherka's local history and resets its client conversation reference. Hermes owns its server conversations, memory and tools; requests spoken to Hermes continue to go to Hermes. For hosted model providers, local deletion does not remove data already received by the provider.

## On your desktop

Hover to reveal controls. Click to keep them visible; click again or press Escape to release them. Drag the portrait to move it and use its corner to resize. The panel and widget can be hidden independently.

The widget menu and selecting a companion card in the board both activate that companion. The notification icon restores the widget and clicks, opens the panel, pauses voice or exits.

In version 0.10, **Text** toggles a transparent input cell below the avatar's controls. Type or paste, press Enter to send and Shift+Enter for a new line. Its checkbox chooses whether to read the reply aloud. Drop text or a text file onto the companion to prepare a message; nothing is submitted until you send it. You can also attach up to four PNG, JPEG or WebP images when the selected model supports vision. Unknown model capabilities require explicit confirmation in the advanced connection settings; a model advertised as text-only cannot accept images. Attachments are sent for that turn only, without storing their pixels in conversation history or memory.

**Image** toggles the optional conversation illustration cell; configure it under Companions first. Drag it to change its position relative to the avatar and resize it from its corner. Choose the image count and slideshow duration in the illustration settings. Images start preparing after text generation, while speech continues. Pressing Image again hides the cell and disables subsequent generations. Both cells follow the widget without opening additional windows. Controls follow the visible avatar bounds, and the character's chosen color supplies its active halo. Application themes are selected in Settings.

## Characters and voices

Create, rename, duplicate or import a profile under **Compagnons**. Each keeps its personality, instructions, image, model, voice and history. A duplicate starts without conversations or memories. Deletion requires the character's name; activate another profile before deleting the active one.

The starter collection includes Sherka and four characters with seven images each: **Botsy**, a curious tinkerer; **Alba**, warm and playful; **Kit**, precise and loyal with dry, deadpan humor; and **Minerva**, irritable, impatient and sharp-tongued. Their presets are available in all seven interface languages. The four new characters use the interface language at creation and keep it afterwards; only the original, unmodified Sherka follows later interface language changes. Existing personal profiles stay intact, and deleted starter characters are not recreated on restart. Orion is no longer included as a starter example.

Open **Avatar & voice** on the character’s portrait to preview and assign its seven expressions: neutral, joy, sadness, anger, surprise, reflection and tenderness. Built-in Sherka includes a matching set. Choose its VoiceStudio engine and voice in this editor, or use the voice shortcut below the portrait. The Companions page also groups provider, model, context, reasoning, service address, transcription and per-companion Internet search settings. Settings contains application preferences, audio devices and updates. No personal voice profile is shipped. Switching characters cancels the previous response and manages the corresponding LM Studio model. Reasoning controls follow advertised capabilities.

### How expressions change

For a companion with expression images, the conversation model chooses one expression at the beginning of each reply. Sherka reads this small marker from the existing text stream and removes it before displaying, speaking or storing the answer. No second model request or microphone mood analysis is involved. The choice describes the tone of the reply; accuracy depends on the model following the instruction. It does not diagnose how you feel.

The expression lasts through the reply and speech playback. Images crossfade over half a second, including the return to neutral; the next image loads before the transition starts. Rapid changes keep only the latest pending expression. The system's reduced-motion preference disables this animation. Finishing, stopping, switching companions or restarting returns the avatar to neutral. Short examples help smaller models follow the protocol. Common alternative emotion names are accepted, and a conservative phrase-based fallback uses the visible reply when the model omits its marker. Leading thinking blocks are excluded. Unrecognized tone, or too little spare context for the expression instruction, uses neutral; a missing expression image falls back to the neutral portrait. **Hermes always stays neutral**, with its listening/speaking indicators. Its saved expression images remain available if you later choose another provider.

Choose local PNG/WebP files up to 10 MiB each, with dimensions up to 4096 × 4096. Sherka validates them, preserves transparency and stores normalized copies up to 1024 × 1024 in the character’s database. You can move the original files afterwards. Resetting an expression removes that slot and uses the neutral portrait; resetting the neutral portrait restores the bundled base image. Images never go to the conversation provider. Built-in images are bundled with the app and can be replaced individually.

Sherka 0.10 can generate separate [conversation illustrations through ComfyUI](COMFYUI.md). Avatar expression packs still use the local images selected here; automatic avatar generation is a later step.

## Voice and memory

Microphone activation is explicit; listening can continue during replies. **Arrêter la réponse** stops the response; **Pause vocale globale** stops input and output. The microphone is off after startup and updates.

With Hermes, use **Stop Hermes (/stop)** in the conversation or widget, or type **`/stop`**. This interrupts the companion's current remote task and local speech; it never shuts down or restarts the server. Sherka checks the server's task status before confirming the stop. If confirmation fails, retry the button before sending more work. Already completed actions are not undone. Speech recognition can mishear words; use the text box when an exact instruction matters.

Under **Mémoire & histoire**, add, correct or forget memories. Essential memories are recalled regularly; topical drawers are retrieved for relevant messages. Ask the companion to remember, correct or forget something: the database write is validated before confirmation. Automatically extracted facts are offered for review.

Use **Optimize memory** in **Settings** or **Memory**, or say **“optimize your memory”** / type **`/optimize-memory`**. The selected model prepares shorter wording and merges overlapping facts, preserving omitted facts. Review the before/after proposal, then choose **Apply optimization** or use **`/apply-memory`**. **`/cancel`** discards the proposal. Nothing is applied automatically. Source links and conversation history remain; new memory edits invalidate an older proposal. The model is instructed to retain names, dates, exceptions and uncertainty, but you should check its wording before applying. Hermes continues to manage its own memory on the server.

Context contains selected recent exchanges and memories, not the entire archive. **Conversation → Contexte & mémoire** distinguishes estimates from server usage.

## Storage and backups

Both distributions use `%LOCALAPPDATA%\fr.sherka.compagnon`: a settings database and one database per companion, including images, text and memories. “No installation” describes launching the program; data is not carried in its ZIP folder.

Transient audio stays in memory; Sherka does not save microphone recordings. Interrupted turns distinguish generated text from completed spoken passages. External applications have their own privacy settings.

For a full backup, exit normally and copy the data folder. Profile JSON exports (version 4, up to 48 MiB) contain custom expression images, the bundled preset reference, personality, connection references, illustration and search preferences, **without history, memories or keys**. Legacy versions 1, 2 and 3 can still be imported. Duplicating a character also keeps its images, voice, illustration and search settings. Memory exports are separate and may contain personal information.

## Updating

Open **Réglages → Mises à jour**, click **Vérifier les mises à jour**, then **Mettre à jour et redémarrer**. No conversations, voice profiles or memories accompany the GitHub request.

Downloads are verified against the embedded public key. Installed copies use the installer. Directly launched copies replace their executable in the same folder with a backup during replacement. Voice and the managed model stop before exit. Data stays in your user folder. Back up before major upgrades; avoid downgrading after a database migration.
