# Dub Studio guidelines

This is a native Windows desktop app for AI video dubbing: a Tauri 2 shell, a Rust/axum service
(`crates/dub-server`) and native C++/CUDA and ONNX engines, with a React 19 + Vite + Tailwind
single-page UI (`frontend/`).

## Stack

- Do not add Python or Node.js to the runtime path. Node is for building the UI only; nothing of
  the shipped app may need it. The Python code of the first era (`backend/`, `dub-engine/`) is
  legacy and is not a reference for behaviour.
- The model stack is fixed: Parakeet-TDT v3 (int8 ONNX) and Whisper-Faster for recognition,
  Nemotron 3 Diarization (Streaming Sortformer v3) for diarization, Gemma-4 12B (GGUF) through llama-server for translation and
  vision, Higgs Audio v3 through `audiocpp_engine.dll` for voice, Mel-Band Roformer through
  BSRoformer.cpp for separation, PP-OCR (ONNX) for on-screen text, SCRFD / LVFace / anime_face /
  CCIP / WeSpeaker for casting. Do not swap a model, a quantization or an engine without the
  maintainer's agreement; a proposal goes into an issue.
- One engine per stage. When a stage fails it degrades (diarization becomes a single speaker) and
  says so; there is no silent second engine behind it.
- Each stage picks its device on its own (`sep_backend`, `diar_backend`, `asr_backend`: GPU, CPU,
  or OpenRouter where a cloud path exists). Keep stages independent; no code may assume one
  global device.
- Audio chain: `normalize_voice`, then `mix_ducked`, then `loudnorm`. Do not simplify it.
- Icons come from `lucide-react`. Do not hand-draw SVG paths.

## Code

- No silent fallbacks: no swallowed errors, no empty `catch`, no "try X, then Y" without saying
  so. When something breaks, return an error with the cause and let the UI show it.
- Comments say only what the code cannot: a non-obvious invariant. No change history, no
  restating the code, no comments in CSS.
- The REST and SSE map is `docs/PORT-CONTRACT.md`; the router is `build_router` in
  `crates/dub-server/src/lib.rs`. Change the document with the route.
- Every external file the app downloads is one entry of `manifest()` in
  `crates/dub-server/src/setup.rs` with its URL, size and markers. The README download table is
  checked against it (see below); after a change to `manifest()` run
  `node scripts/readme-downloads.mjs --write` to regenerate the tables of all six READMEs.

## Interface text

- No string that a user reads may be written in a `.tsx` file. Every text goes through
  `t("key")` and the key exists in all six files of `frontend/src/locales`: `en`, `ru`, `zh`,
  `es`, `fr`, `pt`. The translations are real translations, never a copy of the English text.
- Dates and numbers are formatted with the interface language, not a fixed locale.

## Build and check

```bash
cargo build --workspace          # the server and engines' bindings
cargo test --workspace           # the test gate, must pass
cd frontend && npm ci
npm run build                    # the frontend gate: tsc -b and vite build; `tsc --noEmit` alone is weaker
npm run lint                     # code you add must be clean
npm test                         # where the script exists
node scripts/readme-downloads.mjs   # README download tables against setup.rs
```

- Stop a running `dub-server.exe` before rebuilding it: Windows keeps the `.exe` locked, the
  link fails with `os error 5` and cargo still prints `Finished` with the old binary. Check the
  modification time of the new `dub-server.exe`.
- Run the server for a manual check with `DUB_STUDIO_ROOT=<folder>` and `DUB_STUDIO_PORT=<port>`.
- Both `Cargo.lock` files (the root and `desktop/src-tauri/`) and `frontend/package-lock.json`
  are committed. Change them only together with the dependency change.
- The desktop shell is built on its own (`desktop/src-tauri`, `npx tauri build`); see
  `desktop/src-tauri/STAGING.md`.

## Changelog

- A change that a user can see is written to `CHANGELOG.md` in the same commit, in the
  Unreleased section: `### Added`, `### Changed` or `### Fixed`, each item starting with a bold
  phrase. Internal work (refactors, tests, tooling) is not listed.
- A release bumps the version in `desktop/src-tauri/tauri.conf.json`, dates the changelog section,
  adds a `frontend/src/data/news.json` entry in all six languages, and updates the README files.

## Do not commit

- Model weights, engines and DLLs (`models/`, `*.dll`), projects, caches, test media
  (`test_media/`, `frontend/public/*.mp4`).
- API keys, proxy credentials and signing keys, in any file, including `models/active.json`.
- Local agent notes and handoff files.
