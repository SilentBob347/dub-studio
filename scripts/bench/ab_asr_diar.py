"""A/B распознавания и диаризации на эталонных данных.

ASR: образцы голосового пака с транскриптом (.txt/.lab рядом с .mp3/.wav) -> WER каждой конфигурации.
Диаризация: синтетический ролик из реплик разных голосов пака с известной разметкой -> доля времени речи,
отнесённой к верному спикеру (после лучшего сопоставления меток), и число найденных спикеров.

Конфигурация = бинарь примера dub-asr (crates/dub-asr/examples/asr.rs) + папка TDT + (для диаризации) ONNX
и, если нужно, имя флага модели диаризации (у старых сборок примера — --sortformer).
Пример:
  python scripts/bench/ab_asr_diar.py --voices "F:/AI/Dub Studio/voices" --work D:/Projects/TEMP/_rtest/ab \
    --ort D:/.../onnxruntime.dll \
    --asr "int8-0.3.7=D:/ab/asr-v037.exe|D:/models/tdt" --asr "ultra-0.3.8=D:/ab/asr-v038.exe|D:/models/tdt-ultra" \
    --diar "sortformer-v2=D:/ab/asr-v037.exe|D:/models/tdt|D:/models/sortformer/v2.onnx" \
    --diar "nemotron-3=D:/ab/asr-v038.exe|D:/models/tdt|D:/models/nemotron-diar/nemotron3_diar_v3.onnx"
"""
import argparse
import itertools
import json
import os
import re
import subprocess
import time
import wave
from pathlib import Path

SR = 16000
PARAKEET_PREFIXES = ("RU", "Ru", "EN", "English", "French", "German", "Spanish")


def ffmpeg_wav(src: Path, dst: Path, start=None, dur=None):
    args = ["ffmpeg", "-v", "error", "-y"]
    if start is not None:
        args += ["-ss", f"{start:.3f}"]
    args += ["-i", str(src)]
    if dur is not None:
        args += ["-t", f"{dur:.3f}"]
    args += ["-ac", "1", "-ar", str(SR), "-c:a", "pcm_s16le", str(dst)]
    subprocess.run(args, check=True)


def wav_seconds(p: Path) -> float:
    with wave.open(str(p)) as w:
        return w.getnframes() / w.getframerate()


def norm_words(text: str):
    t = text.lower().replace("ё", "е")
    t = re.sub(r"[^\w\s']", " ", t)
    return [w for w in t.split() if w]


def wer(ref, hyp) -> float:
    d = list(range(len(hyp) + 1))
    for i, r in enumerate(ref, 1):
        prev, d[0] = d[0], i
        for j, h in enumerate(hyp, 1):
            cur = min(d[j] + 1, d[j - 1] + 1, prev + (r != h))
            prev, d[j] = d[j], cur
    return d[len(hyp)] / max(1, len(ref))


def run_asr(binary, tdt, wav_path, env, extra=()):
    t0 = time.time()
    r = subprocess.run([binary, "--wav", str(wav_path), "--tdt", tdt, *extra], capture_output=True, env=env, timeout=900)
    dt = time.time() - t0
    if r.returncode != 0:
        raise RuntimeError(r.stderr.decode("utf-8", "replace")[-800:])
    return json.loads(r.stdout.decode("utf-8")), dt


def voice_samples(voices: Path):
    out = []
    for audio in sorted(voices.iterdir()):
        if audio.suffix.lower() not in (".mp3", ".wav"):
            continue
        if not audio.name.startswith(PARAKEET_PREFIXES):
            continue
        for ext in (".txt", ".lab"):
            ref = audio.with_suffix(ext)
            if ref.is_file():
                out.append((audio, ref.read_text(encoding="utf-8", errors="replace").strip()))
                break
    return out


def bench_asr(args, env, work: Path):
    samples = voice_samples(Path(args.voices))
    if args.max_per_lang:
        by_lang = {}
        for a, t in samples:
            lang = a.name.split("_")[0].upper()[:2]
            by_lang.setdefault(lang, []).append((a, t))
        samples = [x for v in by_lang.values() for x in v[: args.max_per_lang]]
    d = work / "asr"
    d.mkdir(parents=True, exist_ok=True)
    rows = []
    for audio, ref in samples:
        w = d / (audio.stem + ".wav")
        if not w.is_file():
            ffmpeg_wav(audio, w)
        rows.append((audio.stem, w, ref))
    results = {}
    for spec in args.asr:
        name, rest = spec.split("=", 1)
        binary, tdt = rest.split("|")
        tot_err = tot_ref = 0.0
        secs = audio_secs = 0.0
        per = []
        for stem, w, ref in rows:
            out, dt = run_asr(binary, tdt, w, env)
            hyp = " ".join(s.get("text", "") for s in out.get("segments", []))
            rw, hw = norm_words(ref), norm_words(hyp)
            e = wer(rw, hw)
            tot_err += e * len(rw)
            tot_ref += len(rw)
            secs += dt
            audio_secs += wav_seconds(w)
            words_ts = sum(len(s.get("words", [])) for s in out.get("segments", []))
            per.append({"file": stem, "wer": round(e, 4), "words_ts": words_ts, "hyp": hyp})
        results[name] = {"wer": round(tot_err / max(1, tot_ref), 4), "rtf": round(secs / max(1e-6, audio_secs), 3), "files": per}
    return results


def build_synthetic(args, work: Path, n_speakers: int):
    samples = voice_samples(Path(args.voices))
    picked, seen = [], set()
    order = ["RU_Male", "RU_Famale", "EN_Male", "English_Female", "French", "German", "Spanish", "RU_Female"]
    for pref in itertools.cycle(order):
        for a, _ in samples:
            if a.name.startswith(pref) and a.stem not in seen:
                picked.append(a)
                seen.add(a.stem)
                break
        if len(picked) >= n_speakers or len(seen) >= len(samples):
            break
    d = work / f"diar{n_speakers}"
    d.mkdir(parents=True, exist_ok=True)
    chunk, gap = 3.5, 0.3
    pieces, truth, t = [], [], 0.0
    rounds = 3
    for r in range(rounds):
        for si, a in enumerate(picked):
            p = d / f"s{si}_r{r}.wav"
            ffmpeg_wav(a, p, start=0.5 + r * chunk, dur=chunk)
            dur = wav_seconds(p)
            pieces.append(p)
            truth.append((t, t + dur, si))
            t += dur + gap
    silence = d / "gap.wav"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", f"anullsrc=r={SR}:cl=mono", "-t", str(gap), "-c:a", "pcm_s16le", str(silence)], check=True)
    lst = d / "list.txt"
    lst.write_text("".join(f"file '{p.as_posix()}'\nfile '{silence.as_posix()}'\n" for p in pieces), encoding="utf-8")
    out = d / "mix.wav"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", str(lst), "-c:a", "pcm_s16le", str(out)], check=True)
    return out, truth, [a.stem for a in picked]


def score_diar(turns, truth, n_true):
    # доля истинной речи с верной меткой при лучшем сопоставлении предсказанных спикеров истинным
    labels = sorted({t["speaker"] for t in turns})
    overlap = {}
    for s0, s1, g in truth:
        for tr in turns:
            a, b = max(s0, tr["start"]), min(s1, tr["end"])
            if b > a:
                overlap[(g, tr["speaker"])] = overlap.get((g, tr["speaker"]), 0.0) + (b - a)
    total = sum(s1 - s0 for s0, s1, _ in truth)
    best = 0.0
    pred = labels + [None] * max(0, n_true - len(labels))
    for perm in itertools.permutations(pred, n_true) if len(pred) <= 9 else []:
        v = sum(overlap.get((g, p), 0.0) for g, p in enumerate(perm) if p is not None)
        best = max(best, v)
    return round(best / total, 4), len(labels)


def bench_diar(args, env, work: Path):
    results = {}
    for n in args.speakers:
        mix, truth, names = build_synthetic(args, work, n)
        for spec in args.diar:
            name, rest = spec.split("=", 1)
            parts = rest.split("|")
            binary, tdt, onnx = parts[:3]
            flag = parts[3] if len(parts) > 3 else "--diar"
            t0 = time.time()
            r = subprocess.run([binary, "--wav", str(mix), "--tdt", tdt, "--diarize", flag, onnx], capture_output=True, env=env, timeout=1800)
            dt = time.time() - t0
            if r.returncode != 0:
                results[f"{name}@{n}"] = {"error": r.stderr.decode("utf-8", "replace")[-600:]}
                continue
            out = json.loads(r.stdout.decode("utf-8"))
            turns = [{"start": x["start"], "end": x["end"], "speaker": x["speaker"]} for x in out.get("turns", [])]
            acc, found = score_diar(turns, truth, n)
            results[f"{name}@{n}"] = {"true_speakers": n, "found_speakers": found, "accuracy": acc, "seconds": round(dt, 2), "voices": names}
    return results


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--voices", required=True)
    ap.add_argument("--work", required=True)
    ap.add_argument("--ort", required=True, help="путь к onnxruntime.dll (ORT_DYLIB_PATH)")
    ap.add_argument("--path-add", default="", help="каталоги с CUDA-DLL, через ;")
    ap.add_argument("--asr", action="append", default=[])
    ap.add_argument("--diar", action="append", default=[])
    ap.add_argument("--speakers", type=int, nargs="*", default=[3, 6])
    ap.add_argument("--max-per-lang", type=int, default=6)
    args = ap.parse_args()
    env = dict(os.environ, ORT_DYLIB_PATH=args.ort)
    if args.path_add:
        env["PATH"] = args.path_add + os.pathsep + env.get("PATH", "")
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    report = {"asr": bench_asr(args, env, work) if args.asr else {}, "diar": bench_diar(args, env, work) if args.diar else {}}
    (work / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    for name, r in report["asr"].items():
        print(f"ASR {name}: WER {r['wer']:.2%}, RTF {r['rtf']}")
    for name, r in report["diar"].items():
        print(f"DIAR {name}: {r}")


if __name__ == "__main__":
    main()
