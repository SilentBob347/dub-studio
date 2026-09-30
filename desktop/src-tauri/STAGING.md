# Bundle staging (`desktop/src-tauri/staging/`)

Каталог `staging/` — источник для `bundle.resources` в `tauri.bundle.conf.json`. Этот конфиг подключается
только при сборке установщика (`--config`), поэтому `tauri dev`, `npm run build` (`--no-bundle`) и
`cargo build` оболочки работают без staging. Каталог **генерируется** перед
сборкой установщика и **не коммитится** (в `.gitignore`). Собирает то, что должно лечь рядом с `.exe` в
NSIS/MSI-установщике и в портативной раскладке: нативный сервер, SPA, шрифты и **бандл-компоненты**
(VC++-рантайм + OCR-модели). Модели/движки/CUDA/ffmpeg сюда **не** кладутся — они качаются при первом
запуске (см. `crates/dub-server/src/setup.rs`, `delivery: Download`).

## Как собрать staging

Из корня репозитория, после `cargo build --release -p dub-server` и `cd frontend && npm run build`:

```bash
STAGE=desktop/src-tauri/staging
rm -rf "$STAGE"
mkdir -p "$STAGE/models/higgs-engine" "$STAGE/models/ocr" "$STAGE/frontend"
cp target/release/dub-server.exe          "$STAGE/dub-server.exe"
cp -r frontend/dist                       "$STAGE/frontend/dist"
cp -r fonts                               "$STAGE/fonts"
# Бандл: VC++ runtime (delivery=Bundled)
cp models/higgs-engine/MSVCP140.dll models/higgs-engine/VCOMP140.DLL \
   models/higgs-engine/VCRUNTIME140.dll models/higgs-engine/VCRUNTIME140_1.dll \
   "$STAGE/models/higgs-engine/"
# Бандл: OCR-модели (delivery=Bundled)
cp models/ocr/det.onnx models/ocr/cls.onnx \
   models/ocr/rec_cyrillic.onnx models/ocr/rec_cyrillic.dict.txt \
   models/ocr/rec_ch.onnx models/ocr/rec_ch.dict.txt \
   "$STAGE/models/ocr/"
```

Затем `cd desktop && npx tauri build --config src-tauri/tauri.bundle.conf.json` (то же: `npm run bundle`) — NSIS (`-setup.exe`) и MSI (`_en-US.msi`) появятся в
`desktop/src-tauri/target/release/bundle/{nsis,msi}/`.

## Раскладка после установки

Установщик кладёт ресурсы **рядом с `Dub Studio.exe`** (та же раскладка, что портатив). Оболочка
(`desktop/src-tauri/src/lib.rs::resolve_repo_root`) видит `dub-server.exe` рядом и берёт этот каталог
за `DUB_STUDIO_ROOT`; `dub-server` находит `frontend/dist`, `fonts`, `models/higgs-engine`, `models/ocr`
относительно него. Остальное (модели/движки) докачивается в тот же каталог по кнопке «Первый запуск».
