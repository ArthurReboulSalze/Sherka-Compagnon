# Embedded speech detection

`silero_vad.onnx` is official Silero VAD **v6.2**, licensed under MIT (`LICENSE.silero`). It confirms local speech, without transcription or text generation.

- [Source](https://github.com/snakers4/silero-vad/tree/v6.2/src/silero_vad/data).
- [Model](https://raw.githubusercontent.com/snakers4/silero-vad/v6.2/src/silero_vad/data/silero_vad.onnx).
- Size: 2,327,524 bytes.
- SHA-256: `1a153a22f4509e292a94e67d6f9b85e8deb25b4988682b7e174c65279d8788e3`.
- 512 samples at 16 kHz, 64 context samples, recurrent state `[2, 1, 128]`.

The model is embedded, with no runtime download or network request. ONNX Runtime 1.22.0 through `ort = 2.0.0-rc.10` runs on CPU with one thread. MSVC binaries are linked statically by `ort-sys`; initial build download checksum: `540d19b3379fda6fb8f7280d8c15efde20ed225a67a357a6dae38c4300fe190d`. Windows imports include DirectML, but no GPU provider is enabled.

Silero and ONNX Runtime licenses and notices accompany both distributions. Diagnostic speech fixtures are synthetic, stay in ignored `artifacts/`, and are not shipped.
