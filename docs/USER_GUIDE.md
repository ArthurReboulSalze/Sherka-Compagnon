# User guide

## On your desktop

Hover to reveal controls. Click to keep them visible; click again or press Escape to release them. Drag the portrait to move it and use its corner to resize. The panel and widget can be hidden independently.

The widget menu switches companions. The notification icon restores the widget and clicks, opens the panel, pauses voice or exits.

## Characters and voices

Create, rename, duplicate or import a profile under **Compagnons**. Each keeps its personality, instructions, image, model, voice and history. A duplicate starts without conversations or memories. Deletion requires the character's name; activate another profile before deleting the active one.

The starter collection includes Sherka and four characters with seven images each: **Botsy**, a curious tinkerer; **Candis**, warm and playful; **Kit**, precise and loyal with dry, deadpan humor; and **Minerva**, irritable, impatient and sharp-tongued. Their presets are available in all seven interface languages. The four new characters use the interface language at creation and keep it afterwards; only the original, unmodified Sherka follows later interface language changes. Existing personal profiles stay intact, and deleted starter characters are not recreated on restart. Orion is no longer included as a starter example.

Open **Avatar & voice** on the character’s portrait to preview and assign its seven expressions: neutral, joy, sadness, anger, surprise, reflection and tenderness. Built-in Sherka includes a matching set. Choose its VoiceStudio engine and voice in this editor, or use the voice shortcut below the portrait. The Companions page also groups provider, model, context, reasoning, service address, transcription and per-companion Internet search settings. Settings contains application preferences, audio devices and updates. No personal voice profile is shipped. Switching characters cancels the previous response and manages the corresponding LM Studio model. Reasoning controls follow advertised capabilities.

### How expressions change

For a companion with expression images, the conversation model chooses one expression at the beginning of each reply. Sherka reads this small marker from the existing text stream and removes it before displaying, speaking or storing the answer. No second model request or microphone mood analysis is involved. The choice describes the tone of the reply; accuracy depends on the model following the instruction. It does not diagnose how you feel.

The expression lasts through the reply and speech playback. Images crossfade over half a second, including the return to neutral; the next image loads before the transition starts. Rapid changes keep only the latest pending expression. The system's reduced-motion preference disables this animation. Finishing, stopping, switching companions or restarting returns the avatar to neutral. Missing or invalid markers, or too little spare context for the expression instruction, use neutral; a missing expression image falls back to the neutral portrait. **Hermes always stays neutral**, with its listening/speaking indicators. Its saved expression images remain available if you later choose another provider.

Choose local PNG/WebP files up to 10 MiB each, with dimensions up to 4096 × 4096. Sherka validates them, preserves transparency and stores normalized copies up to 1024 × 1024 in the character’s database. You can move the original files afterwards. Resetting an expression removes that slot and uses the neutral portrait; resetting the neutral portrait restores the bundled base image. Images never go to the conversation provider. Built-in images are bundled with the app and can be replaced individually.

ComfyUI generation is planned for a later stage. For now, create images yourself and select them here.

## Voice and memory

Microphone activation is explicit; listening can continue during replies. **Arrêter la réponse** stops the response; **Pause vocale globale** stops input and output. The microphone is off after startup and updates.

With Hermes, use **Stop Hermes (/stop)** in the conversation or widget, or type **`/stop`**. This interrupts the companion's current remote task and local speech; it never shuts down or restarts the server. Sherka checks the server's task status before confirming the stop. If confirmation fails, retry the button before sending more work. Already completed actions are not undone. Speech recognition can mishear words; use the text box when an exact instruction matters.

Under **Mémoire & histoire**, add, correct or forget memories. Essential memories are recalled regularly; topical drawers are retrieved for relevant messages. Ask the companion to remember, correct or forget something: the database write is validated before confirmation. Automatically extracted facts are offered for review.

Context contains selected recent exchanges and memories, not the entire archive. **Conversation → Contexte & mémoire** distinguishes estimates from server usage.

## Storage and backups

Both distributions use `%LOCALAPPDATA%\fr.sherka.compagnon`: a settings database and one database per companion, including images, text and memories. “No installation” describes launching the program; data is not carried in its ZIP folder.

Transient audio stays in memory; Sherka does not save microphone recordings. Interrupted turns distinguish generated text from completed spoken passages. External applications have their own privacy settings.

For a full backup, exit normally and copy the data folder. Profile JSON exports (version 2, up to 48 MiB) contain custom expression images, the bundled preset reference, personality and connection references, **without history, memories or keys**. Legacy version 1 profiles can still be imported. Duplicating a character also keeps its images and voice. Memory exports are separate and may contain personal information.

## Updating

Open **Réglages → Mises à jour**, click **Vérifier les mises à jour**, then **Mettre à jour et redémarrer**. No conversations, voice profiles or memories accompany the GitHub request.

Downloads are verified against the embedded public key. Installed copies use the installer. Directly launched copies replace their executable in the same folder with a backup during replacement. Voice and the managed model stop before exit. Data stays in your user folder. Back up before major upgrades; avoid downgrading after a database migration.
