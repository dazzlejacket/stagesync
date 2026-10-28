#!/usr/bin/env python3
"""Generate deterministic synthetic fixtures for StageSync.

Every fixture has a *known* ground truth, expressed in the same coordinate system the
product uses:

    reference_time_seconds = scale * source_time_seconds + offset_seconds
    drift_ppm               = (scale - 1) * 1_000_000

The source is constructed by sampling the reference's content at the exact affine
positions

    p(j) = scale * (j / out_rate) * ref_rate + offset * ref_rate

so the ground truth is exact rather than approximate: the source contains the whole
reference, relabelled onto its own (slightly fast) clock and shifted by `offset`.

Usage
-----
    python tools/make_fixtures.py --out-dir testdata [--duration 600]

Produces
--------
    reference.wav        48 kHz float32 mono, synthetic "performance"
    source_basic.wav     +37.5 s offset, +35 ppm drift, -20 dB, 44.1 kHz
    source_drift_hi.wav  +12.0 s offset, +1000 ppm drift
    source_drift_lo.wav  -120.0 s offset, -35 ppm drift
    source_stereo.wav    stereo, phase-coherent, +5.0 s offset
    source_gap.wav       a 10 m 22 s recorder gap
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
from scipy import signal

SR_REF = 48_000
SR_SRC = 44_100


def write_wav_float32(path: Path, data: np.ndarray, sample_rate: int) -> None:
    """Write a genuine 32-bit IEEE float WAV file.

    Python's `wave` module can only write integer PCM, so it is deliberately not used
    here: writing float bytes under a PCM header produces a file whose samples are
    nonsense (the decoder reads the bit patterns as integers), which silently turns the
    fixture into near-square-wave noise with a flat envelope.

    The format written here is RIFF/WAVE with `WAVE_FORMAT_IEEE_FLOAT` (format tag 3)
    and a `fact` chunk, matching what a modern soundboard recorder produces.
    """
    data = np.asarray(data, dtype=np.float32)
    channels = 1 if data.ndim == 1 else data.shape[1]
    frames = data.shape[0]
    payload = data.tobytes() if channels == 1 else data.reshape(-1).tobytes()

    block_align = channels * 4
    fmt_chunk = (
        b"fmt "
        + (16).to_bytes(4, "little")
        + (3).to_bytes(2, "little")  # WAVE_FORMAT_IEEE_FLOAT
        + channels.to_bytes(2, "little")
        + sample_rate.to_bytes(4, "little")
        + (sample_rate * block_align).to_bytes(4, "little")
        + block_align.to_bytes(2, "little")
        + (32).to_bytes(2, "little")
    )
    fact_chunk = b"fact" + (4).to_bytes(4, "little") + frames.to_bytes(4, "little")
    data_chunk = b"data" + len(payload).to_bytes(4, "little") + payload
    if len(payload) & 1:
        data_chunk += b"\x00"

    body = b"WAVE" + fmt_chunk + fact_chunk + data_chunk
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(b"RIFF" + len(body).to_bytes(4, "little") + body)


def synth_performance(duration: float, seed: int = 20250208) -> np.ndarray:
    """A deterministic pseudo-musical signal.

    It deliberately contains the things that break naive matchers: quiet passages,
    sharp transients, a repeating harmonic pattern and silence at the start and end.

    What it deliberately does *not* contain is an exactly repeated passage. A signal
    built from a fixed chord loop repeats whole blocks verbatim, which makes several
    positions in the recording genuinely indistinguishable — a property of that signal,
    not of any matcher. The chords, the section dynamics and the melody are therefore
    drawn from a deterministic pseudo-random sequence that does not repeat inside the
    file.
    """
    rng = np.random.default_rng(seed)
    n = int(duration * SR_REF)
    t = np.arange(n, dtype=np.float64) / SR_REF
    out = np.zeros(n, dtype=np.float64)

    scale = [98.0, 110.0, 123.47, 130.81, 146.83, 164.81, 174.61, 196.0]

    chord_len = 3.7
    bars = int(np.ceil(duration / chord_len)) + 1
    bar_rng = np.random.default_rng(seed ^ 0x5bf0_3635)
    bar_roots = bar_rng.choice(np.array(scale), size=bars, replace=True)
    # Smooth the chord changes a little so the result still sounds like music.
    bar_index = np.floor(t / chord_len).astype(int)
    root = bar_roots[np.clip(bar_index, 0, bars - 1)]

    # A slowly moving melody above the chords gives the envelope unique features.
    melody_hz = root * 2.0 * (1.0 + 0.02 * np.sin(2 * np.pi * 0.0713 * t))
    for harmonic, weight in enumerate([1.0, 0.55, 0.3, 0.18, 0.1], start=1):
        out += weight * np.sin(2 * np.pi * root * harmonic * t)
    out += 0.35 * np.sin(2 * np.pi * melody_hz * t)

    # Percussive hits on the beat, so the envelope has sharp transients.
    beat = (t % 0.5) < 0.05
    out += beat * rng.standard_normal(n) * 0.3

    # Section dynamics, drawn per section so that no two passages are alike.
    section_len = 37.0
    sections = int(np.ceil(duration / section_len)) + 1
    sec_rng = np.random.default_rng(seed ^ 0x1d2c_6f2b)
    section_gain = sec_rng.choice(np.array([1.0, 1.0, 0.8, 0.45, 0.12]), size=sections, replace=True)
    section_index = np.floor(t / section_len).astype(int)
    out *= section_gain[np.clip(section_index, 0, sections - 1)]

    out[: int(4.0 * SR_REF)] = 0.0
    out[int(max(duration - 5.0, 0.0) * SR_REF) :] = 0.0

    # Room noise floor.
    out += rng.standard_normal(n) * 1e-3
    peak = np.max(np.abs(out))
    return (out / peak * 0.5).astype(np.float64)


def fractional_shift(x: np.ndarray, delay: float, taps: int = 63) -> np.ndarray:
    """Shift a signal by a fractional number of samples with a windowed-sinc kernel.

    A positive `delay` delays the signal. The kernel is Kaiser-windowed so the shift
    does not audibly colour the result.
    """
    if abs(delay) < 1e-9:
        return x.copy()
    mid = taps // 2
    k = np.arange(taps, dtype=np.float64)
    h = np.sinc(k - mid - delay) * np.kaiser(taps, 8.6)
    h /= h.sum()
    # y[n] = sum_k x[k] * h[n + mid - k]
    full = np.convolve(x, h, mode="full")
    return full[mid : mid + len(x)]


def make_source(
    reference: np.ndarray,
    offset_secs: float,
    drift_ppm: float,
    level_db: float,
    noise_db: float,
    out_rate: int,
    seed: int,
) -> np.ndarray:
    """Build a source recording from the reference with a known offset and drift."""
    rng = np.random.default_rng(seed)
    scale = 1.0 + drift_ppm / 1e6

    # 1. Convert the reference to the source's sample rate (pure rate conversion).
    n_rate = int(round(len(reference) * out_rate / SR_REF))
    u = signal.resample(reference, n_rate)

    # 2. Apply the drift: the source labels the same material on a clock that runs at
    #    `scale` times the reference clock.
    n_drift = int(round(len(u) / scale))
    v = signal.resample(u, n_drift)

    # 3. Apply the placement offset by advancing the signal by offset * out_rate
    #    samples in the source's own (drifted) sample domain. The whole-sample part is
    #    a plain index shift; only the remainder needs the fractional-delay filter.
    shift = offset_secs * out_rate
    whole = int(np.floor(shift))
    frac = shift - whole
    if whole >= 0:
        base = v[whole:]
    else:
        base = np.concatenate([np.zeros(-whole), v])
    shifted = fractional_shift(base, -frac)

    # 4. Level change and an added noise floor.
    shifted *= 10.0 ** (level_db / 20.0)
    shifted += rng.standard_normal(len(shifted)) * 10.0 ** (noise_db / 20.0)
    return shifted


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, default=Path("testdata"))
    parser.add_argument("--duration", type=float, default=600.0)
    parser.add_argument(
        "--gap-start",
        type=float,
        default=200.0,
        help="source-time position at which the simulated recorder gap begins",
    )
    parser.add_argument(
        "--gap-seconds",
        type=float,
        default=622.0,
        help="length of the simulated recorder gap (622 s = 10 m 22 s)",
    )
    args = parser.parse_args()

    out = args.out_dir
    reference = synth_performance(args.duration)
    write_wav_float32(out / "reference.wav", reference, SR_REF)

    basic = make_source(reference, 37.5, 35.0, -20.0, -55.0, SR_SRC, seed=1)
    write_wav_float32(out / "source_basic.wav", basic, SR_SRC)

    hi = make_source(reference, 12.0, 1000.0, -20.0, -55.0, SR_SRC, seed=2)
    write_wav_float32(out / "source_drift_hi.wav", hi, SR_SRC)

    lo = make_source(reference, -120.0, -35.0, -20.0, -55.0, SR_SRC, seed=3)
    write_wav_float32(out / "source_drift_lo.wav", lo, SR_SRC)

    # A stereo source: identical content in both channels, so the inter-channel
    # relationship must survive the correction.
    stereo = make_source(reference, 5.0, 35.0, -20.0, -55.0, SR_SRC, seed=4)
    write_wav_float32(out / "source_stereo.wav", np.stack([stereo, stereo * 0.9], axis=1), SR_SRC)

    # A source with a recorder gap: the performance was simply not recorded for a
    # while. The gap has to fall inside the covered region, otherwise it would sit
    # beyond the end of the reference and test nothing.
    gap_source = make_source(reference, 0.0, 0.0, -20.0, -55.0, SR_SRC, seed=5)
    gap_start = max(0.0, min(args.gap_start, args.duration))
    gap_len = max(0.0, min(args.gap_seconds, args.duration - gap_start))
    if gap_len < args.gap_seconds:
        print(
            f"note: requested a {args.gap_seconds:.0f} s gap at {args.gap_start:.0f} s "
            f"but the fixture is only {args.duration:.0f} s long; using a {gap_len:.0f} s gap"
        )
    a = int(gap_start * SR_SRC)
    b = int((gap_start + gap_len) * SR_SRC)
    gap_source[a:b] = 0.0
    write_wav_float32(out / "source_gap.wav", gap_source, SR_SRC)

    for path in sorted(out.glob("*.wav")):
        print(f"{path}  ({path.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
