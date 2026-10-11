# Languages

Choose **Settings → Interface language** to use French, English, Spanish, Italian, German, Simplified Chinese or Japanese. The choice is saved and applies to the control panel, widget controls and tray menu.

The original Sherka is ready to use in all seven languages. Her personality and additional instructions automatically follow the interface language, with no rewriting needed. Selected models and voices stay as you configured them.

This uses built-in character presets, not automatic translation of your content. Conversations, memories and custom companions remain unchanged. Changing the active default Sherka's language stops the current reply and pauses the microphone; turn it back on to continue in the new language.

## Companion language

Choose a custom companion's language when creating it. It stays fixed afterwards, including when copied or exported. Write its personality and additional instructions in that language. To create a character in another language, create a separate companion.

Renaming Sherka or editing her personality or additional instructions makes her a custom companion. She then keeps her current language. Copies and imported presets also keep their own language.

Sherka tells the conversation model to use the companion's language independently of the interface. Explicit translation requests remain possible. Results depend on the selected model and the instructions you give it.

## Your models and voices

Select and configure your own conversation model in LM Studio, and your speech recognition, speech engine and voice in VoiceStudio. Check that these choices support the language you want to use. Changing Sherka's interface does not choose or install models or voices.

The seven interface languages are available in **Sherka 0.4.0 and later**.

## Translation contributions

The shared catalog is `config/locales/ui.json`. French text is the source key; each entry contains English, Spanish, Italian, German, Simplified Chinese and Japanese translations, in that order. Keep numbered placeholders intact and do not put HTML in translations.

The built-in character texts are in `config/presets/sherka.json`, with a personality and additional instructions for each language code. Keep the character's tone and honesty rules consistent across languages.

Run `npm run test:i18n` after editing it. This checks coverage and placeholders and tests that translated controls never rewrite user content.
