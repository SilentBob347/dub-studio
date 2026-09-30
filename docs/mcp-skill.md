---
name: dub-studio
description: Drive Dub Studio on this computer through its MCP server - dub a video into another language with the speakers' own cloned voices, make a voice-over, translated or original-language subtitles, or a transcript; fix the translation, the timing and the speakers line by line; restyle the subtitles, add titles and blur boxes; cast the characters of a series and give them library voices; export the same video in several languages; export SRT or TXT; save the result to a folder. Use whenever the user asks for anything the studio does.
---

# Dub Studio through MCP

Dub Studio serves MCP at `http://127.0.0.1:8793/mcp` while it is open (Streamable HTTP,
stateless JSON-RPC). Every tool runs the same code as a button of the studio, through the
routes its window calls.

## If the studio is not running yet

1. It is a Windows desktop application. If it is not installed, download the installer or
   the portable archive from https://github.com/timoncool/dub-studio/releases/latest. It is
   made for an NVIDIA card; its first start offers to download the models.
2. Start it. The MCP server is up as soon as its window is: `http://127.0.0.1:8793/mcp`.
   Nothing else to install - no npx, no bridge.
3. Connect (below), then call `studio_status`. If models are missing, `models_status`
   names them and `models_download` fetches them.

## Connect

```bash
claude mcp add --transport http dub-studio http://127.0.0.1:8793/mcp
```

Other clients: `{ "mcpServers": { "dub-studio": { "type": "streamable-http", "url": "http://127.0.0.1:8793/mcp" } } }`.

The server also serves this skill (resource `studio://skill`, prompt `studio`), the
language codes (`studio://languages`), every edit of a project with its fields
(`studio://patch-ops`) and a prompt `dub_video` (a file, a language, a mode). It speaks MCP
`2026-07-28` (stateless: every request carries its version in `_meta`, `server/discover`
describes the server) and the handshake revisions `2025-11-25`, `2025-06-18` and
`2025-03-26` through `initialize`. Only agents on this computer and the studio's own window
may connect.

The user sees it in the studio too: Settings, **Agent (MCP)** shows whether an agent is
connected and the address to paste.

## Ground rules

- **Start with `studio_status`.** It tells what runs now, what finished last, and whether
  the required models are there.
- **Long work is a job**: `project_analyze`, `project_dub_audio`, `project_render`,
  `project_export_lang`, `project_retranslate`, `project_remix`, `project_resume`,
  `voices_download_pack`. Each answers a `job_id`; then `studio_wait`
  with it (or `until: analyze | dub_audio | render | export_lang | retranslate | remix |
  download | voices_pack | idle`) instead of polling. It returns within a minute (30 s by
  default, 55 at most) with how far the work got; call it again. `job_get` and `jobs_list`
  read jobs (`jobs_list` with a `pid` also shows the project's last stored job),
  `job_cancel` stops one, `project_resume` starts a project's interrupted or failed job
  again where it stopped.
- **One job holds the graphics card at a time**, and a preview frame is made there too:
  `project_frame` answers "busy" while a job runs. `studio_wait until: idle`, then take
  the frame.
- **Edits are instant and saved.** A line whose words, timing, speaker or voice changed is
  *dirty*; `project_dub_audio` and `project_render` voice only the dirty lines again, the
  rest comes from the cache. `segment_regen` marks one line, `segments_regen_all` every
  line. Changing the mode, the target language, the translation's tone or the voices makes
  every line dirty.
- **How the dub sounds**: a voiced line loses the silence around it before it is fitted to its
  slot (long pauses inside shrink only when it would not fit), and a cloned voice's reference is
  cut from the separated vocals in full band, at word boundaries - so is a `voice_from_speaker`
  voice, which is refused with `no_separation` when the project has no separated vocals and no
  separation engine is installed, and fails with the reason when its line cannot be separated.
- **Answers are short by default**: `project_get` leaves out word timings and the vision
  context, an edit answers what it changed and how many lines are dirty. Pass
  `response_format: detailed` for everything - and only take a project for `project_put`
  from a detailed `project_get`, or the fields left out are lost.
- **Look ids up, never guess them**: `projects_list`, `project_get` (line ids, title and
  blur box idx), `voices_list`, `casting_get`, `casting_library_list`, `models_status`,
  `engine_presets_get`.
- **Files on this computer are passed by path**: `project_create` (a video, and subtitles
  to import), `models_import`, `project_save_output` and `project_export_text` (a folder).
  `project_files` names the project's own files.
- **Destructive tools say so**: deleting, cancelling, replacing a project, analyzing again
  (it replaces the lines), retranslating, remixing, and edits that overwrite text are marked
  destructive, so your client asks the user first.
- **The OpenRouter key never comes back**: `settings_get` and `studio_capabilities` show
  only `or_key_set`, `openrouter_status` where the key comes from.

## Recipes

**Dub a video**

1. `studio_status`; if models are missing, `models_download` and `studio_wait`.
2. `project_create` with the video's `path`; keep its `project_id`.
3. `project_analyze` with `tgt_lang` (a code of `studio://languages`) and `mode: dub`
   (`voiceover` for a voice-over, `nodub` for subtitles only, `transcribe` for a
   transcript); `studio_wait` with its `job_id`.
4. `project_get`: read the translation line by line and fix it with `segment_update`.
5. `project_render`; `studio_wait` with its `job_id`.
6. `project_frame` at a moment with speech to see the burned-in subtitles.
7. `project_save_output` into the folder the user wants; tell them the path.

**Fix the translation and render again**

1. `project_get` with `from`/`to` around the moment, or `ids`.
2. `segment_update` with `tgt_text` (and `start`/`end` or `speaker` when they are wrong);
   `segments_hide` for a line that should be neither voiced nor subtitled,
   `segments_keep_original` where the original voice should stay.
3. `project_dub_audio` to hear it quickly, or `project_render` for the video; either voices
   only the dirty lines. `studio_wait`, then `project_frame` to check.

**Only subtitles, or a transcript**

1. `project_analyze` with `mode: nodub` (translated subtitles over the original audio) or
   `mode: transcribe` (the transcript in the original language).
2. `project_export_text` with `format: srt` (or `txt`) and `dir` - `text: src` for the
   original words, `tgt` for the translation; `name` goes with `dir` only (without `dir`
   the file lands in the project's folder as subtitles.srt, transcript.srt, translation.txt
   or transcript.txt). `project_render` burns the subtitles in
   instead, `subtitles_burn_set` with `on: false` leaves the picture clean.
3. Subtitles the user already has: `project_create` with `subtitles_path` (.srt, .ass,
   .ssa); `project_analyze` takes their text and timing instead of recognising speech, and
   `import_translated: true` when they are already in `tgt_lang`.

**The same video in several languages**

1. Finish and check the first language (the layout, the style, the titles and the blur
   carry over).
2. `project_export_lang` with `lang`, one language at a time: each answers a new
   `project_id` and a `job_id`; `studio_wait` with the job before the next.
3. `project_save_output` for each new project.

**Cast the characters**

1. `project_analyze` with `casting: true` (`content_type: real` for live action, `anime`
   for drawn characters; look at a frame with `project_frame source: original` if unsure).
2. `casting_get`: the characters with their speakers and lines; `casting_avatar` shows a
   face.
3. `casting_update` to name them, set their gender, a `dub_voice` from `voices_list`
   (null clones their own voice) and a `speech_note` for the translation's tone.
   `voice_slots_assign` deals library voices by gender instead; `voice_from_speaker` makes
   a library voice of a speaker.
4. `casting_library_save` keeps the cast; the next episode's `project_analyze` with
   `casting: true` and `casting_ref` (a slug of `casting_library_list`) applies it.

**A batch into one folder**

For each file: `project_create`, `project_analyze`, `studio_wait`, `project_render`,
`studio_wait`, `project_save_output` with the same `dir` and the file's own name - a name
already there gets (2), (3).

**The cloud instead of the graphics card**

`openrouter_set_key` stores a key (it is checked first; `openrouter_verify` only checks);
`openrouter_models` lists the models of a stage and `openrouter_voices` a speech model's
voices. `settings_set` switches the stages: `or_llm_on`/`or_llm` (translation),
`or_vision_on`/`or_vision`, `or_tts_on`/`or_tts_model`/`or_tts_voice`, `or_asr_on`/`or_asr`.
`engine_presets_get` and `engine_preset_apply` set everything for this computer's card or
for the cloud at once. `proxy_test` checks a proxy before `proxy_settings_set` stores it
(`proxy_settings_get` shows it, the password hidden).

## Tools by area

- **studio**: `studio_status`, `studio_wait`, `studio_system` (card, video memory, RAM),
  `studio_capabilities`.
- **jobs**: `jobs_list`, `job_get`, `job_cancel`.
- **models**: `models_status`, `models_download` (runs in the background beside the jobs: wait with
  `studio_wait` `until: download`), `models_cancel_download` (a pause: the same ids continue it),
  `models_remove` (frees disk space), `models_import` (files already on disk), `models_select`
  (a quantisation or a recogniser).
- **settings**: `settings_get`, `settings_set`, `engine_presets_get`,
  `engine_preset_apply`, `proxy_test`, `proxy_settings_get`, `proxy_settings_set`,
  `fonts_list`, `caption_presets_list`, `launch_defaults_get`, `launch_defaults_set` (what the start
  screen's form opens with), `studio_paths` (where the data, projects and models are).
- **openrouter**: `openrouter_status`, `openrouter_set_key`, `openrouter_delete_key`,
  `openrouter_verify`, `openrouter_models`, `openrouter_voices`, `openrouter_catalog`,
  `openrouter_catalog_refresh`.
- **local server** (Ollama, LM Studio, vLLM, llama-server for translation and vision):
  `local_server_models`, `local_server_key_status`, `local_server_key_set`, `local_server_key_delete`;
  the provider of each stage is `settings_set` `llm_provider` / `vision_provider` (local, server, openrouter).
- **project**: `projects_list` (query, since, until), `project_create`, `project_get`,
  `project_analyze`, `project_resume`, `project_retranslate`, `project_remix`,
  `project_align`, `project_dub_audio`, `project_render`, `project_export_lang`,
  `project_put`, `project_patch` (any edit by its op), `project_delete`,
  `project_waveform`, `project_frame`.
- **files**: `project_files`, `project_export_text` (SRT, TXT), `project_save_output`,
  `project_open_output`, `project_reveal`.
- **lines**: `segment_update`, `segment_add`, `segments_delete`, `segments_hide`,
  `segments_keep_original`, `segments_reorder`, `segment_regen`, `segments_regen_all`.
- **what the project makes**: `project_mode_set`, `audio_output_set`,
  `subtitles_content_set`, `subtitles_burn_set`, `subtitles_position_set`,
  `translation_target_set`, `translation_style_set`, `rewrite_set`, `voice_set`,
  `gain_set`, `voiceover_gain_set`, `original_track_set`.
- **subtitles, titles, blur**: `caption_style_set` (all lines or one), `caption_preset_set`,
  `title_add`, `title_update`, `titles_delete`, `blur_add`, `blur_update`, `blurs_delete`,
  `blur_enable`.
- **casting**: `casting_get`, `casting_update`, `casting_avatar`, `casting_library_list`,
  `casting_library_save`, `casting_library_delete`, `casting_library_avatar`.
- **voices**: `voices_list`, `voices_catalog`, `voice_download`, `voices_download_pack`,
  `voice_rename`, `voice_delete`, `voice_from_speaker`, `voice_slots_assign`.
