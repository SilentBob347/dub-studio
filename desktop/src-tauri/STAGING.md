# Bundle staging (`desktop/src-tauri/staging/`)

Каталог `staging/` — источник для `bundle.resources` в `tauri.conf.json`. Он **генерируется** перед
`npx tauri build` и **не коммитится** (в `.gitignore`). Собирает то, что должно лечь рядом с `.exe` в
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
# Бандл: VC++ runtime (delivery=Bundled). Все пять — из одного VC++ Redistributable (например,
# VC/Redist/MSVC/<версия>/x64/Microsoft.VC145.CRT и .OpenMP из Build Tools); MSVCP140_1.dll нужен onnxruntime 1.28.
cp models/higgs-engine/MSVCP140.dll models/higgs-engine/MSVCP140_1.dll models/higgs-engine/VCOMP140.DLL \
   models/higgs-engine/VCRUNTIME140.dll models/higgs-engine/VCRUNTIME140_1.dll \
   "$STAGE/models/higgs-engine/"
# Бандл: OCR-модели (delivery=Bundled)
cp models/ocr/det.onnx models/ocr/cls.onnx \
   models/ocr/rec_cyrillic.onnx models/ocr/rec_cyrillic.dict.txt \
   models/ocr/rec_ch.onnx models/ocr/rec_ch.dict.txt \
   "$STAGE/models/ocr/"
```

Проверка комплекта по манифесту (все файлы Bundled-компонентов, в том числе пять DLL VC++ runtime, на
своих местах). Без неё релиз без `MSVCP140_1.dll` запирает на «Первом запуске» всех, у кого нет VC++
Redistributable в системе: докачать Bundled-компонент нельзя. Тем же тестом проверяется распакованный
портатив — `DUB_RELEASE_STAGING` указывает на его каталог.

```bash
DUB_RELEASE_STAGING="$PWD/$STAGE" cargo test -p dub-server --lib setup::tests::the_release_staging_carries_every_bundled_file -- --ignored
```

Затем `cd desktop && npx tauri build` — NSIS (`-setup.exe`) и MSI (`_en-US.msi`) появятся в
`desktop/src-tauri/target/release/bundle/{nsis,msi}/`.

## Раскладка после установки

Установщик кладёт ресурсы **рядом с `Dub Studio.exe`** (та же раскладка, что портатив). Оболочка
(`desktop/src-tauri/src/lib.rs::resolve_repo_root`) видит `dub-server.exe` рядом и берёт этот каталог
за `DUB_STUDIO_ROOT`; `dub-server` находит `frontend/dist`, `fonts`, `models/higgs-engine`, `models/ocr`
относительно него. Остальное (модели/движки) докачивается в тот же каталог по кнопке «Первый запуск».
