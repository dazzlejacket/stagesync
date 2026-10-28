#!/usr/bin/env python3
"""Independent verification of a StageSync corrected export against its reference.

This tool is deliberately *not* part of the Rust pipeline.  It re-derives the
residual alignment from the audio itself, with a different language, a different
code path and a different DSP library, so that it can act as an external check
on the numbers the Rust pipeline reports.

Coordinate convention (identical to the Rust pipeline's, see
`crates/stagesync-core/src/export/mod.rs`):

    reference_time_seconds = scale * source_time_seconds + offset_seconds
    drift_ppm               = (scale - 1) * 1_000_000

The corrected file is expected to be a **source-aligned render**: its frame `n`
corresponds to reference time `offset + n / sample_rate`.  A *position* given on
the command line is a time on the corrected file's own timeline (identical to
source-clock time for a source-aligned render); the reference time that position
should correspond to is `offset + position`.

Sign convention for every residual reported here:

    positive  ->  the corrected audio is AHEAD of the reference
    negative  ->  the corrected audio is BEHIND the reference

Usage
-----
    python tools/verify_exports.py \
        --corrected corrected.wav --reference reference.wav \
        --offset 317.391152 --scale 1.0000129454 \
        --positions 600 1500 2600 3700 4900 \
        --analysis-rate 16000 --json out.json --markdown out.md

Only numpy and scipy are used; no network access and no other dependencies.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from dataclasses import dataclass, field
from fractions import Fraction
from pathlib import Path
from typing import Any, Optional, Sequence

import numpy as np
from scipy import signal

TOOL_NAME = "verify_exports.py"
TOOL_VERSION = "1.0.0"
COORDINATE_SYSTEM = (
    "reference_time_seconds = scale * source_time_seconds + offset_seconds; "
    "drift_ppm = (scale - 1) * 1_000_000"
)
POSITION_SEMANTICS = (
    "seconds on the corrected file's own timeline "
    "(== source-clock time for a source-aligned render); the reference time it "
    "should correspond to is offset + position"
)


class VerifyError(Exception):
    """A user-facing error; printed as a message, never as a traceback."""


# ---------------------------------------------------------------------------
# WAV reading (pure numpy, no soundfile)
# ---------------------------------------------------------------------------


@dataclass
class WavFile:
    """A decoded WAV file, read lazily so that only the needed frames are touched."""

    path: Path
    sample_rate: int
    channels: int
    frames: int
    bits_per_sample: int
    format_tag: int
    data_offset: int
    data_bytes: int
    _map: Optional[np.ndarray] = None
    _raw: Optional[bytes] = None

    @property
    def duration(self) -> float:
        return self.frames / self.sample_rate if self.sample_rate else 0.0

    def read_mono(self, start_frame: int, count: int) -> np.ndarray:
        """Read `count` frames starting at `start_frame` as float64 mono.

        Stereo is mixed down by averaging the channels; the analysis is mono
        anyway and averaging preserves the timing information exactly.
        """
        start_frame = max(0, start_frame)
        count = min(count, self.frames - start_frame)
        if count <= 0:
            return np.zeros(0, dtype=np.float64)
        if self._raw is not None:  # 24-bit: the bytes live in a plain buffer
            stride = self._bytes_per_frame()
            blob = self._raw[start_frame * stride :][: count * stride]
            return self._decode_24(blob, count)
        block = self._map[start_frame : start_frame + count]
        values = _to_float(block, self.format_tag, self.bits_per_sample)
        if self.channels > 1:
            values = values.reshape(count, self.channels).mean(axis=1)
        return values

    def _bytes_per_frame(self) -> int:
        return (self.bits_per_sample // 8) * self.channels

    def _decode_24(self, blob: bytes, count: int) -> np.ndarray:
        raw = np.frombuffer(blob, dtype=np.uint8)
        usable = (raw.size // (3 * self.channels)) * (3 * self.channels)
        raw = raw[:usable].reshape(-1, 3).astype(np.int32)
        values = raw[:, 0] | (raw[:, 1] << 8) | (raw[:, 2] << 16)
        # Sign-extend the 24-bit values into int32, then normalise.
        values = np.where(values & 0x800000, values - 0x1000000, values)
        values = values.astype(np.float64) / 8388608.0
        if self.channels > 1:
            values = values.reshape(-1, self.channels).mean(axis=1)
        return values[:count]


def _parse_fmt(body: bytes) -> tuple[int, int, int, int, int]:
    """Return (format_tag, channels, sample_rate, bits_per_sample, block_align)."""
    if len(body) < 16:
        raise VerifyError("WAV 'fmt ' chunk is too short to be valid")
    fmt_tag = int.from_bytes(body[0:2], "little")
    channels = int.from_bytes(body[2:4], "little")
    sample_rate = int.from_bytes(body[4:8], "little")
    block_align = int.from_bytes(body[12:14], "little")
    bits = int.from_bytes(body[14:16], "little")
    if fmt_tag == 0xFFFE:  # WAVE_FORMAT_EXTENSIBLE: the real tag is in the GUID.
        if len(body) < 26:
            raise VerifyError("extensible 'fmt ' chunk is too short to be valid")
        fmt_tag = int.from_bytes(body[24:26], "little")
    return fmt_tag, channels, sample_rate, bits, block_align


def open_wav(path: Path) -> WavFile:
    """Open a RIFF/WAVE file and describe it; the samples are read on demand."""
    if not path.exists():
        raise VerifyError(f"file not found: {path}")
    if path.stat().st_size < 12:
        raise VerifyError(f"file is too small to be a WAV file: {path}")
    with path.open("rb") as fh:
        header = fh.read(12)
        if header[:4] != b"RIFF" or header[8:12] != b"WAVE":
            raise VerifyError(f"not a RIFF/WAVE file: {path}")
        fmt: Optional[tuple[int, int, int, int, int]] = None
        data_offset: Optional[int] = None
        data_size = 0
        while True:
            chunk_header = fh.read(8)
            if len(chunk_header) < 8:
                break
            chunk_id = chunk_header[:4]
            size = int.from_bytes(chunk_header[4:8], "little")
            if chunk_id == b"fmt ":
                fmt = _parse_fmt(fh.read(size))
                if size & 1:
                    fh.seek(1, 1)
            elif chunk_id == b"data":
                data_offset = fh.tell()
                data_size = size
                fh.seek(size + (size & 1), 1)
            else:
                fh.seek(size + (size & 1), 1)
            if fmt is not None and data_offset is not None:
                break
    if fmt is None:
        raise VerifyError(f"no 'fmt ' chunk found in {path}")
    if data_offset is None:
        raise VerifyError(f"no 'data' chunk found in {path}")
    fmt_tag, channels, sample_rate, bits, block_align = fmt
    if channels < 1:
        raise VerifyError(f"invalid channel count ({channels}) in {path}")
    if sample_rate < 1:
        raise VerifyError(f"invalid sample rate ({sample_rate}) in {path}")
    if fmt_tag == 1 and bits not in (8, 16, 24, 32):
        raise VerifyError(f"unsupported PCM bit depth ({bits}) in {path}")
    if fmt_tag == 3 and bits not in (32, 64):
        raise VerifyError(f"unsupported IEEE-float bit depth ({bits}) in {path}")
    if fmt_tag not in (1, 3):
        raise VerifyError(
            f"unsupported WAV format tag {fmt_tag} in {path} "
            "(only PCM integer and IEEE float are supported; convert AAC/M4A first)"
        )
    bytes_per_sample = bits // 8
    frame_bytes = bytes_per_sample * channels
    if frame_bytes == 0:
        raise VerifyError(f"invalid sample layout in {path}")
    frames = data_size // frame_bytes
    if frames == 0:
        raise VerifyError(f"WAV file contains no audio frames: {path}")

    wav = WavFile(
        path=path,
        sample_rate=sample_rate,
        channels=channels,
        frames=frames,
        bits_per_sample=bits,
        format_tag=fmt_tag,
        data_offset=data_offset,
        data_bytes=data_size,
    )
    if bits == 24:
        # 24-bit has no native numpy dtype: pull the bytes into a buffer once.
        with path.open("rb") as fh:
            fh.seek(data_offset)
            wav._raw = fh.read(data_size)
        return wav
    dtype = {
        (1, 8): np.uint8,
        (1, 16): "<i2",
        (1, 32): "<i4",
        (3, 32): "<f4",
        (3, 64): "<f8",
    }[(fmt_tag, bits)]
    wav._map = np.memmap(
        path, dtype=dtype, mode="r", offset=data_offset, shape=(frames * channels,)
    )
    return wav


def _to_float(block: np.ndarray, fmt_tag: int, bits: int) -> np.ndarray:
    if fmt_tag == 1:
        if bits == 8:
            return (np.asarray(block, dtype=np.float64) - 128.0) / 128.0
        scale = float(1 << (bits - 1))
        return np.asarray(block, dtype=np.float64) / scale
    return np.asarray(block, dtype=np.float64)


# ---------------------------------------------------------------------------
# Signal conditioning
# ---------------------------------------------------------------------------


def to_analysis_rate(x: np.ndarray, src_rate: int, dst_rate: int) -> np.ndarray:
    """Resample a measurement copy to the common analysis rate.

    `scipy.signal.resample_poly` with the exact rational ratio is used: it is a
    polyphase FIR interpolation/decimation that is time-aligned (verified: an
    impulse at t stays at t), so window positions keep their meaning.  Only the
    copies used for measurement are resampled; the files on disk are untouched.
    """
    if src_rate == dst_rate:
        return np.asarray(x, dtype=np.float64)
    ratio = Fraction(int(dst_rate), int(src_rate)).limit_denominator(10000)
    return signal.resample_poly(np.asarray(x, dtype=np.float64), ratio.numerator, ratio.denominator)


def band_limit(x: np.ndarray, fs: int, lo: float, hi: float) -> np.ndarray:
    """Band-limit to roughly `lo`..`hi` Hz with a zero-phase filter.

    Zero phase (`sosfiltfilt`) matters: a causal filter would shift the signal
    by a frequency-dependent delay and bias the very lag we are measuring.
    """
    nyquist = fs / 2.0
    lo = max(float(lo), 20.0)
    hi = min(float(hi), nyquist * 0.95)
    if hi <= lo:
        return np.asarray(x, dtype=np.float64)
    sos = signal.butter(4, [lo, hi], btype="bandpass", fs=fs, output="sos")
    if len(x) < 3 * (2 * len(sos) + 1):
        return np.asarray(x, dtype=np.float64)
    return signal.sosfiltfilt(sos, np.asarray(x, dtype=np.float64))


def rms_envelope(x: np.ndarray, rate: int, hop_secs: float) -> tuple[np.ndarray, np.ndarray]:
    """RMS envelope on a fixed hop, with the centre time of every frame."""
    hop = max(1, int(round(hop_secs * rate)))
    n = len(x) // hop
    if n == 0:
        return np.zeros(0), np.zeros(0)
    frames = np.asarray(x[: n * hop], dtype=np.float64).reshape(n, hop)
    rms = np.sqrt(np.mean(frames * frames, axis=1))
    times = (np.arange(n) + 0.5) * hop / rate
    return rms, times


def active_fraction(
    x: np.ndarray, rate: int, background: float, hop_secs: float = 0.010
) -> float:
    """Fraction of the window that carries signal rather than the background.

    `background` is the recording's own background level (see
    [`background_level`]); a frame counts as active when it is 1.5 times that.
    A window that is mostly background cannot support a trustworthy alignment
    measurement, and this is how the tool says so.
    """
    rms, _ = rms_envelope(x, rate, hop_secs)
    if rms.size < 8:
        return 0.0
    if background <= 0.0:
        return 1.0
    return float(np.mean(rms > 1.5 * background))


def background_level(wav: WavFile, samples: int = 400) -> float:
    """Estimate a recording's background level from short windows over the file.

    The estimate has to come from the whole file, not from the window being
    measured: a window that is entirely signal would otherwise be judged
    against its own quietest frame and appear to contain no signal at all.
    """
    win = max(1, int(round(0.010 * wav.sample_rate)))
    if wav.frames <= win:
        return 0.0
    step = max(1, wav.frames // samples)
    values: list[float] = []
    for i in range(samples):
        start = i * step
        if start + win > wav.frames:
            break
        block = wav.read_mono(start, win)
        values.append(float(np.sqrt(np.mean(block * block))))
    if not values:
        return 0.0
    # The second percentile lands on the recording's noise floor rather than on
    # its quiet passages, which is what a background estimate has to mean.
    return float(np.percentile(values, 2))


# ---------------------------------------------------------------------------
# Measurements
# ---------------------------------------------------------------------------


def gcc_phat(
    ref: np.ndarray,
    corr: np.ndarray,
    max_lag: int,
    eps_fraction: float = 1e-3,
    guard: int = 80,
) -> tuple[float, float, float, float]:
    """GCC-PHAT delay estimate between a reference window and a corrected window.

    The cross-spectrum is phase-transformed (`R = X * conj(Y) / |R|`), which
    whitens the magnitude so that the estimate is dominated by phase, not by
    the programme's spectral balance.  This is a genuinely different estimator
    from the RMS-envelope correlation the Rust pipeline fits its model with.

    The epsilon is *not* infinitesimal: it is a small fraction of the median
    cross-spectrum magnitude.  A pure phase transform gives bins that carry no
    signal (noise-only bins) the same weight as bins that carry the programme,
    and on noisy material that produces a spurious peak at zero lag.  The floor
    keeps the magnitude weighting where there is no signal, while leaving the
    phase transform intact where there is.

    Returns `(lag_samples, peak, zscore, peak_ratio)` where **positive lag
    means the corrected audio is ahead of the reference**.
    """
    n = 1
    while n < 2 * max(len(ref), len(corr)):
        n <<= 1
    x = np.fft.rfft(ref, n)
    y = np.fft.rfft(corr, n)
    cross = x * np.conj(y)
    mag = np.abs(cross)
    eps = eps_fraction * float(np.median(mag)) + 1e-30
    cc = np.fft.irfft(cross / (mag + eps), n)

    kmax = int(min(max_lag, n // 2 - 1))
    if kmax < 1:
        raise VerifyError("analysis window is too short for a lag search")
    positive = cc[: kmax + 1]
    negative = cc[n - kmax :]  # lags -kmax .. -1
    best_pos = int(np.argmax(positive))
    best_neg = int(np.argmax(negative))
    if negative[best_neg] > positive[best_pos]:
        lag = -(kmax - best_neg)
        peak = float(negative[best_neg])
    else:
        lag = best_pos
        peak = float(positive[best_pos])

    # Sub-sample refinement by parabolic interpolation over the peak.
    idx = lag if lag > 0 else n + lag
    if 0 < idx < n - 1:
        y0, y1, y2 = float(cc[idx - 1]), float(cc[idx]), float(cc[idx + 1])
        denom = y0 - 2.0 * y1 + y2
        if abs(denom) > 1e-15:
            lag += float(np.clip(0.5 * (y0 - y2) / denom, -0.5, 0.5))

    # Reliability: how far does the peak stand above the rest of the searched
    # lag range, and how much better is it than the runner-up?  A peak that is
    # only marginally better than a competitor is not a trustworthy measurement.
    searched = np.concatenate([cc[: kmax + 1], cc[n - kmax :]])
    lag_of = np.concatenate(
        [np.arange(0, kmax + 1), np.arange(-kmax, 0)]
    )
    far = np.abs(lag_of - lag) > guard
    if far.any():
        others = searched[far]
        mean = float(np.mean(others))
        std = float(np.std(others))
        runner_up = float(np.max(others))
    else:
        mean, std, runner_up = 0.0, 0.0, 0.0
    zscore = (peak - mean) / std if std > 1e-12 else 0.0
    peak_ratio = peak / runner_up if runner_up > 1e-12 else float("inf")
    return lag, peak, zscore, peak_ratio


def gcc_phat_consensus(
    ref: np.ndarray,
    corr: np.ndarray,
    max_lag: int,
    fs: int,
    segment_secs: float,
    guard: int,
    eps_fraction: float,
    corr_background: float,
    ref_background: float,
) -> dict[str, Any]:
    """GCC-PHAT delay estimate, taken as the consensus of several sub-windows.

    A single long correlation curve is ambiguous: on real music the same
    passage a few milliseconds away correlates almost as well as the passage
    itself, so the tallest peak is not always the right one.  Splitting the
    window into overlapping segments and taking the **median** of the segment
    peaks makes the estimate depend on what most of the window agrees on, and
    the fraction of segments that agree becomes an honest reliability figure.

    Segments that are mostly background are skipped: they cannot align
    anything.

    Returns a dict with the lag (positive = the corrected audio is ahead), the
    peak height, the z-score, the peak ratio and the agreement fraction.
    """
    seg = int(round(segment_secs * fs))
    if seg < 8 * guard or len(corr) < seg:
        seg = len(corr)
    step = max(1, seg // 2)
    lags: list[float] = []
    peaks: list[float] = []
    zscores: list[float] = []
    ratios: list[float] = []
    skipped = 0
    for start in range(0, max(1, len(corr) - seg + 1), step):
        if start + seg > len(corr):
            break
        c_seg = corr[start : start + seg]
        r_seg = ref[start : start + seg]
        if (
            active_fraction(c_seg, fs, corr_background) < 0.5
            or active_fraction(r_seg, fs, ref_background) < 0.5
        ):
            skipped += 1
            continue
        lag, peak, zscore, ratio = gcc_phat(
            r_seg, c_seg, max_lag, eps_fraction=eps_fraction, guard=guard
        )
        lags.append(lag)
        peaks.append(peak)
        zscores.append(zscore)
        ratios.append(ratio)
    if not lags:
        raise VerifyError("no usable segment: the window carries no alignable signal")
    lag_array = np.asarray(lags, dtype=np.float64)
    median_lag = float(np.median(lag_array))
    # Agreement: how many segments agree with the median, within 1 ms.
    tolerance = max(1.0, 0.001 * fs)
    agreement = float(np.mean(np.abs(lag_array - median_lag) <= tolerance))
    return {
        "lag_samples": median_lag,
        "peak": float(np.median(peaks)),
        "zscore": float(np.median(zscores)),
        "peak_ratio": float(np.median(ratios)),
        "agreement": agreement,
        "segments": len(lags),
        "segments_skipped": skipped,
        "segment_lags": [float(x) for x in lag_array],
    }


def envelope_ncc(
    corr_env: np.ndarray,
    corr_times: np.ndarray,
    ref_env: np.ndarray,
    ref_times: np.ndarray,
    offset: float,
    hop: float,
    max_lag_frames: int,
) -> tuple[float, float]:
    """Normalised cross-correlation of two RMS envelopes, parabolic peak refined.

    `corr_times` are absolute times on the corrected file's timeline and
    `ref_times` absolute times on the reference timeline.  The model says that
    corrected time `t` belongs at reference time `offset + t`, so for a lag of
    `lag` frames the corrected envelope is compared with the reference envelope
    at reference time `offset + t - lag * hop`.

    Returns `(lag_frames, peak)`; a positive lag means the corrected audio is
    ahead of the reference.
    """
    if corr_env.size < 8 or ref_env.size < 8:
        raise VerifyError("envelope is too short to correlate")
    lags = np.arange(-max_lag_frames, max_lag_frames + 1)
    values = np.empty(lags.size, dtype=np.float64)
    for i, lag in enumerate(lags):
        # Reference time that the corrected sample at this lag claims.
        t_ref = offset + corr_times - lag * hop
        ref_values = np.interp(t_ref, ref_times, ref_env, left=np.nan, right=np.nan)
        mask = np.isfinite(ref_values) & np.isfinite(corr_env)
        if mask.sum() < 8:
            values[i] = 0.0
            continue
        a = corr_env[mask]
        b = ref_values[mask]
        a = a - a.mean()
        b = b - b.mean()
        na = float(np.linalg.norm(a))
        nb = float(np.linalg.norm(b))
        values[i] = float(np.dot(a, b) / (na * nb)) if na > 1e-12 and nb > 1e-12 else 0.0

    best = int(np.argmax(values))
    lag = float(lags[best])
    peak = float(values[best])
    if 0 < best < values.size - 1:
        y0, y1, y2 = values[best - 1], values[best], values[best + 1]
        denom = y0 - 2.0 * y1 + y2
        if abs(denom) > 1e-15:
            lag += float(np.clip(0.5 * (y0 - y2) / denom, -0.5, 0.5))
    return lag, peak


def estimate_f0(
    x: np.ndarray, fs: int, fmin: float, fmax: float, gate: float
) -> tuple[Optional[float], float, float]:
    """Autocorrelation pitch estimate.

    Returns `(f0_hz, normalised_autocorrelation_peak, rms)`.  `f0_hz` is `None`
    when the window is unvoiced or too quiet to measure.
    """
    rms = float(np.sqrt(np.mean(np.square(x)))) if x.size else 0.0
    if x.size < 16 or rms <= 0.0:
        return None, 0.0, rms
    y = x - x.mean()
    n = 1
    while n < 2 * y.size:
        n <<= 1
    spectrum = np.fft.rfft(y, n)
    ac = np.fft.irfft(np.abs(spectrum) ** 2, n)[: y.size]
    if ac[0] <= 1e-20:
        return None, 0.0, rms
    ac = ac / ac[0]
    min_lag = max(2, int(fs / fmax))
    max_lag = min(int(fs / fmin), ac.size - 1)
    if max_lag <= min_lag:
        return None, 0.0, rms
    window = ac[min_lag : max_lag + 1]
    best = int(np.argmax(window)) + min_lag
    peak = float(ac[best])
    if peak < gate:
        return None, peak, rms
    lag = float(best)
    if 0 < best < ac.size - 1:
        y0, y1, y2 = ac[best - 1], ac[best], ac[best + 1]
        denom = y0 - 2.0 * y1 + y2
        if abs(denom) > 1e-15:
            lag += float(np.clip(0.5 * (y0 - y2) / denom, -0.5, 0.5))
    return fs / lag, peak, rms


def log_spectral_distance(
    a: np.ndarray, b: np.ndarray, fs: int, lo: float = 100.0, hi: float = 5000.0
) -> tuple[float, float]:
    """Shape-only log-spectral distance between two windows, in dB.

    The median level difference is removed before the distance is computed, so
    that a simple level change is not mistaken for a change of spectral
    character.  The removed level offset is returned as well.

    Only bins where the *reference* carries signal are compared: bins that sit
    at the noise floor of one recording but not the other would otherwise
    dominate the result and say nothing about the alignment.
    """
    nperseg = min(2048, max(256, len(a) // 4), len(b) // 4)
    if nperseg < 64:
        raise VerifyError("window too short for a spectral comparison")
    fa, pa = signal.welch(a, fs=fs, nperseg=nperseg, noverlap=nperseg // 2)
    fb, pb = signal.welch(b, fs=fs, nperseg=nperseg, noverlap=nperseg // 2)
    band = (fa >= lo) & (fa <= hi)
    if band.sum() < 8:
        raise VerifyError("no spectral bins in the comparison band")
    reference_level = float(np.median(pa[band]))
    signal_bins = band & (pa > 10.0 * reference_level)
    if signal_bins.sum() < 8:
        signal_bins = band
    eps = 1e-20
    da = 10.0 * np.log10(np.maximum(pa[signal_bins], eps))
    db = 10.0 * np.log10(np.maximum(pb[signal_bins], eps))
    diff = da - db
    level = float(np.median(diff))
    shape = diff - level
    return float(10.0 * np.sqrt(np.mean(shape * shape))), level


# ---------------------------------------------------------------------------
# Position handling
# ---------------------------------------------------------------------------


@dataclass
class Config:
    analysis_rate: int = 16000
    window_secs: float = 20.0
    max_lag_secs: float = 0.5
    margin_secs: float = 0.25
    band_lo: float = 200.0
    band_hi: float = 3400.0
    envelope_hop_secs: float = 0.010
    envelope_max_lag_secs: float = 5.0
    min_zscore: float = 4.0
    min_peak: float = 0.05
    min_peak_ratio: float = 1.10
    phat_guard_ms: float = 5.0
    phat_eps_fraction: float = 0.01
    segment_secs: float = 4.0
    min_agreement: float = 0.6
    min_envelope_peak: float = 0.5
    disagree_ms: float = 10.0
    min_trusted: int = 5
    f0_min: float = 70.0
    f0_max: float = 800.0
    f0_gate: float = 0.45
    f0_window_secs: float = 2.0
    min_active_fraction: float = 0.5
    rms_floor: float = 1e-4
    default_positions: int = 8


@dataclass
class PositionResult:
    index: int
    source_time_s: float
    reference_time_s: float
    problems: list[str] = field(default_factory=list)
    waveform: Optional[dict[str, Any]] = None
    envelope: Optional[dict[str, Any]] = None
    pitch: Optional[dict[str, Any]] = None
    spectral: Optional[dict[str, Any]] = None
    disputed: bool = False
    trusted: bool = False
    status: str = "ERROR"

    def to_dict(self) -> dict[str, Any]:
        return {
            "index": self.index,
            "source_time_s": self.source_time_s,
            "reference_time_s": self.reference_time_s,
            "status": self.status,
            "trusted": self.trusted,
            "disputed": self.disputed,
            "problems": list(self.problems),
            "waveform_gcc_phat": self.waveform,
            "envelope_ncc": self.envelope,
            "pitch": self.pitch,
            "spectral": self.spectral,
        }


def default_positions(
    corrected: WavFile, reference: WavFile, offset: float, cfg: Config
) -> list[float]:
    """Spread measurement positions over the region where both files have audio."""
    half = cfg.window_secs / 2.0 + cfg.margin_secs
    # The corrected file covers reference times [offset, offset + duration].
    lo = max(0.0, -offset)
    hi = min(corrected.duration, reference.duration - offset)
    lo = max(lo, half)
    hi = min(hi, corrected.duration - half)
    if hi <= lo:
        return []
    count = max(2, cfg.default_positions)
    return [float(x) for x in np.linspace(lo, hi, count)]


def _window_bounds(
    wav: WavFile, center: float, half: float
) -> tuple[int, int, Optional[str]]:
    """Frame range of a window centred at `center`, plus a problem note."""
    start = int(round((center - half) * wav.sample_rate))
    end = int(round((center + half) * wav.sample_rate))
    if start < 0:
        return start, end, (
            f"window starts {(-start) / wav.sample_rate:.3f} s before the start of "
            f"{wav.path.name} (needs {half:.1f} s of headroom)"
        )
    if end > wav.frames:
        return start, end, (
            f"window ends {(end - wav.frames) / wav.sample_rate:.3f} s after the end of "
            f"{wav.path.name}"
        )
    return start, end, None


def measure_position(
    index: int,
    source_time: float,
    corrected: WavFile,
    reference: WavFile,
    offset: float,
    scale: float,
    cfg: Config,
    source: Optional[WavFile],
    do_waveform: bool,
    do_envelope: bool,
    do_pitch: bool,
    backgrounds: dict[str, float],
) -> PositionResult:
    """Measure one position.  Every failure is reported, never invented."""
    reference_time = offset + source_time
    result = PositionResult(
        index=index,
        source_time_s=source_time,
        reference_time_s=reference_time,
    )
    half = cfg.window_secs / 2.0
    pad = cfg.margin_secs

    # ---- waveform (GCC-PHAT) -------------------------------------------------
    corr_wave: Optional[np.ndarray] = None
    ref_wave: Optional[np.ndarray] = None
    if do_waveform or do_pitch:
        c_start, c_end, c_problem = _window_bounds(corrected, source_time, half + pad)
        r_start, r_end, r_problem = _window_bounds(reference, reference_time, half + pad)
        if c_problem:
            result.problems.append(f"corrected: {c_problem}")
        if r_problem:
            result.problems.append(f"reference: {r_problem}")
        if c_problem is None and r_problem is None:
            corr_raw = corrected.read_mono(c_start, c_end - c_start)
            ref_raw = reference.read_mono(r_start, r_end - r_start)
            corr_wave = to_analysis_rate(corr_raw, corrected.sample_rate, cfg.analysis_rate)
            ref_wave = to_analysis_rate(ref_raw, reference.sample_rate, cfg.analysis_rate)
            trim = int(round(pad * cfg.analysis_rate))
            if corr_wave.size > 2 * trim and ref_wave.size > 2 * trim:
                corr_wave = corr_wave[trim:-trim]
                ref_wave = ref_wave[trim:-trim]
            else:
                corr_wave = None
                ref_wave = None
                result.problems.append("window is shorter than the filter margin")

    if do_waveform and corr_wave is not None and ref_wave is not None:
        corr_rms = float(np.sqrt(np.mean(np.square(corr_wave))))
        ref_rms = float(np.sqrt(np.mean(np.square(ref_wave))))
        corr_active = active_fraction(corr_wave, cfg.analysis_rate, backgrounds["corrected"])
        ref_active = active_fraction(ref_wave, cfg.analysis_rate, backgrounds["reference"])
        if corr_rms < cfg.rms_floor or ref_rms < cfg.rms_floor:
            result.problems.append(
                f"silent window (corrected rms {corr_rms:.2e}, reference rms {ref_rms:.2e})"
            )
        elif min(corr_active, ref_active) < cfg.min_active_fraction:
            result.problems.append(
                f"window is mostly quiet (active fraction {corr_active:.2f} corrected, "
                f"{ref_active:.2f} reference)"
            )
        else:
            corr_bp = band_limit(corr_wave - corr_wave.mean(), cfg.analysis_rate, cfg.band_lo, cfg.band_hi)
            ref_bp = band_limit(ref_wave - ref_wave.mean(), cfg.analysis_rate, cfg.band_lo, cfg.band_hi)
            max_lag = int(round(cfg.max_lag_secs * cfg.analysis_rate))
            guard = int(round(cfg.phat_guard_ms * cfg.analysis_rate / 1000.0))
            try:
                consensus = gcc_phat_consensus(
                    ref_bp,
                    corr_bp,
                    max_lag,
                    cfg.analysis_rate,
                    cfg.segment_secs,
                    guard,
                    cfg.phat_eps_fraction,
                    backgrounds["corrected"],
                    backgrounds["reference"],
                )
            except VerifyError as exc:
                result.problems.append(f"waveform: {exc}")
                consensus = None
            if consensus is not None:
                lag = consensus["lag_samples"]
                residual_ms = lag / cfg.analysis_rate * 1000.0
                peak = consensus["peak"]
                zscore = consensus["zscore"]
                peak_ratio = consensus["peak_ratio"]
                trusted = (
                    zscore >= cfg.min_zscore
                    and abs(peak) >= cfg.min_peak
                    and peak_ratio >= cfg.min_peak_ratio
                    and consensus["agreement"] >= cfg.min_agreement
                    and abs(residual_ms) <= cfg.max_lag_secs * 1000.0
                )
                reason = ""
                if zscore < cfg.min_zscore:
                    reason = f"peak separation z={zscore:.2f} < {cfg.min_zscore:.2f}"
                elif abs(peak) < cfg.min_peak:
                    reason = f"peak {peak:.3f} < {cfg.min_peak:.3f}"
                elif peak_ratio < cfg.min_peak_ratio:
                    reason = f"peak only {peak_ratio:.2f}x the runner-up < {cfg.min_peak_ratio:.2f}"
                elif consensus["agreement"] < cfg.min_agreement:
                    reason = (
                        f"only {consensus['agreement'] * 100:.0f}% of the segments agree "
                        f"< {cfg.min_agreement * 100:.0f}%"
                    )
                result.waveform = {
                    "method": "gcc_phat_bandpassed_waveform",
                    "residual_ms": residual_ms,
                    "lag_seconds": lag / cfg.analysis_rate,
                    "lag_samples": lag,
                    "peak": peak,
                    "zscore": zscore,
                    "peak_ratio": peak_ratio,
                    "agreement": consensus["agreement"],
                    "segments": consensus["segments"],
                    "segments_skipped": consensus["segments_skipped"],
                    "segment_lags_samples": consensus["segment_lags"],
                    "analysis_rate": cfg.analysis_rate,
                    "band_hz": [cfg.band_lo, cfg.band_hi],
                    "corrected_rms": corr_rms,
                    "reference_rms": ref_rms,
                    "corrected_active_fraction": corr_active,
                    "reference_active_fraction": ref_active,
                    "trusted": trusted,
                    "reason": reason,
                }

    # ---- envelope (the method the Rust pipeline fits with) -------------------
    if do_envelope:
        c_start, c_end, c_problem = _window_bounds(corrected, source_time, half)
        if c_problem:
            result.problems.append(f"corrected: {c_problem}")
        else:
            corr_raw = corrected.read_mono(c_start, c_end - c_start)
            corr_env, corr_times = rms_envelope(
                corr_raw, corrected.sample_rate, cfg.envelope_hop_secs
            )
            # Absolute times on the corrected file's own timeline.
            corr_times = corr_times + (source_time - half)
            hop_corr = cfg.envelope_hop_secs
            # The reference needs room on both sides for the +-5 s lag search;
            # it is clipped to what the file actually contains.
            want_start = reference_time - half - cfg.envelope_max_lag_secs - 1.0
            want_end = reference_time + half + cfg.envelope_max_lag_secs + 1.0
            ref_start = max(0.0, want_start)
            ref_end = min(reference.duration, want_end)
            if ref_end - ref_start < 2.0:
                result.problems.append(
                    "envelope: not enough reference audio around this position "
                    f"({ref_end - ref_start:.2f} s available)"
                )
            else:
                ref_raw = reference.read_mono(
                    int(round(ref_start * reference.sample_rate)),
                    int(round((ref_end - ref_start) * reference.sample_rate)),
                )
                ref_env, ref_times = rms_envelope(
                    ref_raw, reference.sample_rate, cfg.envelope_hop_secs
                )
                ref_times = ref_times + ref_start
                try:
                    lag_frames, peak = envelope_ncc(
                        corr_env,
                        corr_times,
                        ref_env,
                        ref_times,
                        offset,
                        hop_corr,
                        int(round(cfg.envelope_max_lag_secs / hop_corr)),
                    )
                    residual_ms = lag_frames * hop_corr * 1000.0
                    result.envelope = {
                        "method": "rms_envelope_ncc",
                        "residual_ms": residual_ms,
                        "peak": peak,
                        "hop_seconds": hop_corr,
                        "search_seconds": cfg.envelope_max_lag_secs,
                        "trusted": peak >= cfg.min_envelope_peak,
                    }
                except VerifyError as exc:
                    result.problems.append(f"envelope: {exc}")

    # ---- pitch ---------------------------------------------------------------
    if do_pitch:
        # A dedicated, short window: the autocorrelation of a long window is
        # diluted as soon as the material changes key, which is exactly what
        # happens in live music.
        f0_half = cfg.f0_window_secs / 2.0
        c_start, c_end, c_problem = _window_bounds(corrected, source_time, f0_half)
        r_start, r_end, r_problem = _window_bounds(reference, reference_time, f0_half)
        if c_problem or r_problem:
            if c_problem:
                result.problems.append(f"pitch: corrected: {c_problem}")
            if r_problem:
                result.problems.append(f"pitch: reference: {r_problem}")
        else:
            corr_pitch = to_analysis_rate(
                corrected.read_mono(c_start, c_end - c_start),
                corrected.sample_rate,
                cfg.analysis_rate,
            )
            ref_pitch = to_analysis_rate(
                reference.read_mono(r_start, r_end - r_start),
                reference.sample_rate,
                cfg.analysis_rate,
            )
            f0_corr, gate_corr, rms_corr = estimate_f0(
                corr_pitch, cfg.analysis_rate, cfg.f0_min, cfg.f0_max, cfg.f0_gate
            )
            f0_ref, gate_ref, rms_ref = estimate_f0(
                ref_pitch, cfg.analysis_rate, cfg.f0_min, cfg.f0_max, cfg.f0_gate
            )
            cents = None
            if f0_corr and f0_ref and f0_corr > 0 and f0_ref > 0:
                cents = 1200.0 * math.log2(f0_corr / f0_ref)
            result.pitch = {
                "f0_corrected_hz": f0_corr,
                "f0_reference_hz": f0_ref,
                "cents_difference": cents,
                "autocorr_peak_corrected": gate_corr,
                "autocorr_peak_reference": gate_ref,
                "rms_corrected": rms_corr,
                "rms_reference": rms_ref,
                "window_seconds": cfg.f0_window_secs,
                "voiced": bool(f0_corr and f0_ref),
            }

    # ---- log-spectral distance ----------------------------------------------
    if corr_wave is not None and ref_wave is not None:
        try:
            lsd, level = log_spectral_distance(corr_wave, ref_wave, cfg.analysis_rate)
            result.spectral = {
                "corrected_vs_reference_lsd_db": lsd,
                "corrected_vs_reference_level_db": level,
            }
            if source is not None:
                # The uncorrected source is compared at the *same* nominal
                # position: its material is really at reference time
                # scale*u + offset, so the accumulated drift smears the
                # comparison.  A correction must not make that worse.
                s_start, s_end, s_problem = _window_bounds(source, source_time, half)
                if s_problem:
                    result.spectral["uncorrected_vs_reference_lsd_db"] = None
                    result.spectral["uncorrected_note"] = s_problem
                else:
                    src_raw = source.read_mono(s_start, s_end - s_start)
                    src_wave = to_analysis_rate(src_raw, source.sample_rate, cfg.analysis_rate)
                    lsd_src, level_src = log_spectral_distance(
                        src_wave, ref_wave, cfg.analysis_rate
                    )
                    result.spectral["uncorrected_vs_reference_lsd_db"] = lsd_src
                    result.spectral["uncorrected_vs_reference_level_db"] = level_src
                    # Where the uncorrected material really sits, versus the
                    # nominal position it was compared at.
                    result.spectral["uncorrected_actual_reference_time_s"] = (
                        scale * source_time + offset
                    )
        except VerifyError as exc:
            result.problems.append(f"spectral: {exc}")

    # ---- verdict -------------------------------------------------------------
    wave_ms = result.waveform["residual_ms"] if result.waveform else None
    env_ms = result.envelope["residual_ms"] if result.envelope else None
    if wave_ms is not None and env_ms is not None:
        diff = abs(wave_ms - env_ms)
        result.disputed = diff > cfg.disagree_ms
        if result.disputed:
            result.problems.append(
                f"methods disagree by {diff:.2f} ms (> {cfg.disagree_ms:.0f} ms)"
            )
    wave_trusted = bool(result.waveform and result.waveform["trusted"])
    result.trusted = wave_trusted and not result.disputed
    if result.trusted:
        result.status = "TRUSTED"
    elif result.disputed:
        result.status = "DISPUTED"
    elif wave_trusted:
        result.status = "TRUSTED"
    elif result.waveform is not None:
        result.status = "UNTRUSTED"
    else:
        result.status = "ERROR"
    return result



# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------


def percentile(values: Sequence[float], q: float) -> float:
    if not values:
        return float("nan")
    return float(np.percentile(np.asarray(values, dtype=np.float64), q))


def summarise(results: list[PositionResult], cfg: Config) -> dict[str, Any]:
    trusted = [r for r in results if r.trusted]
    wave_all = [r.waveform["residual_ms"] for r in results if r.waveform]
    wave_trusted = [r.waveform["residual_ms"] for r in trusted if r.waveform]
    env_trusted = [r.envelope["residual_ms"] for r in trusted if r.envelope]
    cents = [
        r.pitch["cents_difference"]
        for r in results
        if r.pitch and r.pitch.get("cents_difference") is not None
    ]
    lsd_values = [
        r.spectral["corrected_vs_reference_lsd_db"]
        for r in results
        if r.spectral and r.spectral.get("corrected_vs_reference_lsd_db") is not None
    ]
    lsd_src = [
        r.spectral["uncorrected_vs_reference_lsd_db"]
        for r in results
        if r.spectral and r.spectral.get("uncorrected_vs_reference_lsd_db") is not None
    ]

    def stats(values: Sequence[float]) -> dict[str, Any]:
        if not values:
            return {"count": 0, "p50_ms": None, "p95_ms": None, "max_abs_ms": None}
        return {
            "count": len(values),
            "p50_ms": percentile(values, 50),
            "p95_ms": percentile(values, 95),
            "max_abs_ms": float(max(abs(v) for v in values)),
        }

    summary: dict[str, Any] = {
        "positions": len(results),
        "trusted_positions": len(trusted),
        "disputed_positions": sum(1 for r in results if r.disputed),
        "untrusted_positions": sum(1 for r in results if r.status == "UNTRUSTED"),
        "errored_positions": sum(1 for r in results if r.status == "ERROR"),
        "waveform_all_positions": stats(wave_all),
        "waveform_trusted_positions": stats(wave_trusted),
        "envelope_trusted_positions": stats(env_trusted),
        "pitch": {
            "voiced_positions": len(cents),
            "mean_abs_cents": float(np.mean(np.abs(cents))) if cents else None,
            "max_abs_cents": float(np.max(np.abs(cents))) if cents else None,
        },
        "spectral": {
            "corrected_lsd_db_median": float(np.median(lsd_values)) if lsd_values else None,
            "uncorrected_lsd_db_median": float(np.median(lsd_src)) if lsd_src else None,
        },
        "confidence": "high" if len(trusted) >= cfg.min_trusted else "insufficient_data",
        "warnings": [],
    }
    if len(trusted) < cfg.min_trusted:
        summary["warnings"].append(
            f"ONLY {len(trusted)} TRUSTED POSITION(S): fewer than {cfg.min_trusted} were "
            "measured reliably, so NO accuracy claim is made from this run."
        )
    if summary["disputed_positions"]:
        summary["warnings"].append(
            f"{summary['disputed_positions']} position(s) were DISPUTED (the waveform and "
            "envelope methods disagreed by more than "
            f"{cfg.disagree_ms:.0f} ms); they are excluded from the headline statistics."
        )
    if summary["errored_positions"]:
        summary["warnings"].append(
            f"{summary['errored_positions']} position(s) could not be measured at all; "
            "see the per-position problems."
        )
    if lsd_values and lsd_src:
        if float(np.median(lsd_values)) > float(np.median(lsd_src)):
            summary["warnings"].append(
                "the corrected audio is spectrally FURTHER from the reference than the "
                "uncorrected source; the correction did not improve spectral similarity."
            )
    return summary


def print_table(results: list[PositionResult], cfg: Config) -> None:
    header = (
        f"{'#':>3}  {'source_t':>9}  {'ref_t':>9}  {'wave_ms':>9}  {'peak':>7}  "
        f"{'z':>7}  {'env_ms':>9}  {'env_pk':>7}  {'diff_ms':>8}  {'cents':>8}  status"
    )
    print(header)
    print("-" * len(header))
    for r in results:
        wave = f"{r.waveform['residual_ms']:9.3f}" if r.waveform else " " * 9
        peak = f"{r.waveform['peak']:7.3f}" if r.waveform else " " * 7
        z = f"{r.waveform['zscore']:7.2f}" if r.waveform else " " * 7
        env = f"{r.envelope['residual_ms']:9.3f}" if r.envelope else " " * 9
        envpk = f"{r.envelope['peak']:7.3f}" if r.envelope else " " * 7
        if r.waveform and r.envelope:
            diff = f"{abs(r.waveform['residual_ms'] - r.envelope['residual_ms']):8.2f}"
        else:
            diff = " " * 8
        cents = " " * 8
        if r.pitch and r.pitch.get("cents_difference") is not None:
            cents = f"{r.pitch['cents_difference']:8.2f}"
        print(
            f"{r.index:>3}  {r.source_time_s:9.3f}  {r.reference_time_s:9.3f}  {wave}  "
            f"{peak}  {z}  {env}  {envpk}  {diff}  {cents}  {r.status}"
        )
        for problem in r.problems:
            print(f"      note: {problem}")


def print_summary(summary: dict[str, Any], cfg: Config) -> None:
    print()
    print("Summary (waveform GCC-PHAT, the independent measurement)")
    print("-------------------------------------------------------")
    wave = summary["waveform_trusted_positions"]
    print(
        f"  trusted positions : {summary['trusted_positions']} of {summary['positions']}"
        f"   (disputed: {summary['disputed_positions']}, "
        f"untrusted: {summary['untrusted_positions']}, error: {summary['errored_positions']})"
    )
    if wave["count"]:
        print(
            f"  |residual| p50    : {wave['p50_ms']:.3f} ms"
        )
        print(
            f"  |residual| p95    : {wave['p95_ms']:.3f} ms"
        )
        print(
            f"  |residual| max    : {wave['max_abs_ms']:.3f} ms"
        )
    else:
        print("  |residual| p50/p95/max : n/a (no trusted positions)")
    env = summary["envelope_trusted_positions"]
    if env["count"]:
        print(
            f"  envelope method (self-confirming, for comparison): "
            f"p50 {env['p50_ms']:.3f} ms, p95 {env['p95_ms']:.3f} ms, "
            f"max {env['max_abs_ms']:.3f} ms"
        )
    pitch = summary["pitch"]
    if pitch["mean_abs_cents"] is not None:
        print(
            f"  pitch difference  : mean |{pitch['mean_abs_cents']:.2f} cents| "
            f"over {pitch['voiced_positions']} voiced position(s), "
            f"max |{pitch['max_abs_cents']:.2f} cents|"
        )
    else:
        print("  pitch difference  : no position was voiced enough to measure")
    spectral = summary["spectral"]
    if spectral["corrected_lsd_db_median"] is not None:
        line = f"  log-spectral distance (corrected vs reference): {spectral['corrected_lsd_db_median']:.2f} dB"
        if spectral["uncorrected_lsd_db_median"] is not None:
            line += (
                f"   (uncorrected source: {spectral['uncorrected_lsd_db_median']:.2f} dB)"
            )
        print(line)
    print()
    for warning in summary["warnings"]:
        print(f"  *** {warning}")
    if summary["confidence"] == "insufficient_data":
        print()
        print(
            "  RESULT: INSUFFICIENT DATA. This run does NOT establish an accuracy "
            "figure for the export."
        )
    else:
        print()
        print(
            "  RESULT: residual statistics above are the measured alignment error of "
            "this export at the listed positions."
        )


def markdown_report(
    results: list[PositionResult],
    summary: dict[str, Any],
    model: dict[str, Any],
    inputs: dict[str, Any],
    cfg: Config,
) -> str:
    lines: list[str] = []
    lines.append("# StageSync export verification")
    lines.append("")
    lines.append(f"Tool: `{TOOL_NAME}` v{TOOL_VERSION}")
    lines.append("")
    lines.append(f"Coordinate convention: `{COORDINATE_SYSTEM}`")
    lines.append("")
    lines.append("Positions are given on the corrected file's own timeline.")
    lines.append("")
    lines.append("## Inputs")
    lines.append("")
    lines.append("| file | role | sample rate | channels | duration |")
    lines.append("|---|---|---|---|---|")
    for item in inputs["files"]:
        lines.append(
            f"| `{item['path']}` | {item['role']} | {item['sample_rate']} Hz | "
            f"{item['channels']} | {item['duration_s']:.3f} s |"
        )
    lines.append("")
    lines.append("## Model")
    lines.append("")
    lines.append(f"- scale: `{model['scale']:.12f}`")
    lines.append(f"- offset: `{model['offset_seconds']:.9f}` s")
    lines.append(f"- drift: `{model['drift_ppm']:.3f}` ppm")
    lines.append("")
    lines.append("## Positions")
    lines.append("")
    lines.append(
        "| # | source time (s) | reference time (s) | waveform GCC-PHAT (ms) | peak | z | "
        "envelope NCC (ms) | env peak | difference (ms) | pitch (cents) | status |"
    )
    lines.append("|---|---|---|---|---|---|---|---|---|---|---|")
    for r in results:
        wave = f"{r.waveform['residual_ms']:.3f}" if r.waveform else "n/a"
        peak = f"{r.waveform['peak']:.3f}" if r.waveform else "n/a"
        z = f"{r.waveform['zscore']:.2f}" if r.waveform else "n/a"
        env = f"{r.envelope['residual_ms']:.3f}" if r.envelope else "n/a"
        envpk = f"{r.envelope['peak']:.3f}" if r.envelope else "n/a"
        diff = (
            f"{abs(r.waveform['residual_ms'] - r.envelope['residual_ms']):.2f}"
            if r.waveform and r.envelope
            else "n/a"
        )
        cents = (
            f"{r.pitch['cents_difference']:.2f}"
            if r.pitch and r.pitch.get("cents_difference") is not None
            else "n/a"
        )
        lines.append(
            f"| {r.index} | {r.source_time_s:.3f} | {r.reference_time_s:.3f} | {wave} | "
            f"{peak} | {z} | {env} | {envpk} | {diff} | {cents} | {r.status} |"
        )
    lines.append("")
    for r in results:
        for problem in r.problems:
            lines.append(f"- position {r.index}: {problem}")
    if any(r.problems for r in results):
        lines.append("")
    lines.append("## Summary")
    lines.append("")
    wave = summary["waveform_trusted_positions"]
    lines.append(f"- trusted positions: **{summary['trusted_positions']} of {summary['positions']}**")
    if wave["count"]:
        lines.append(
            f"- |residual| over trusted positions: p50 **{wave['p50_ms']:.3f} ms**, "
            f"p95 **{wave['p95_ms']:.3f} ms**, max **{wave['max_abs_ms']:.3f} ms**"
        )
    else:
        lines.append("- |residual| over trusted positions: n/a (no trusted positions)")
    env = summary["envelope_trusted_positions"]
    if env["count"]:
        lines.append(
            f"- envelope method (the estimator the pipeline fits with, for comparison): "
            f"p50 {env['p50_ms']:.3f} ms, p95 {env['p95_ms']:.3f} ms, max {env['max_abs_ms']:.3f} ms"
        )
    pitch = summary["pitch"]
    if pitch["mean_abs_cents"] is not None:
        lines.append(
            f"- pitch difference: mean |{pitch['mean_abs_cents']:.2f} cents| over "
            f"{pitch['voiced_positions']} voiced position(s)"
        )
    spectral = summary["spectral"]
    if spectral["corrected_lsd_db_median"] is not None:
        line = f"- log-spectral distance (corrected vs reference): {spectral['corrected_lsd_db_median']:.2f} dB"
        if spectral["uncorrected_lsd_db_median"] is not None:
            line += f" (uncorrected source: {spectral['uncorrected_lsd_db_median']:.2f} dB)"
        lines.append(line)
    lines.append(f"- confidence: **{summary['confidence']}**")
    lines.append("")
    for warning in summary["warnings"]:
        lines.append(f"> **{warning}**")
    lines.append("")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="verify_exports.py",
        description=(
            "Independently verify a StageSync corrected export: measure the residual "
            "alignment against the reference with GCC-PHAT on the raw waveform, "
            "cross-check it with an RMS-envelope correlation, and measure the pitch "
            "and spectral consequences of the correction."
        ),
        epilog=(
            "Sign convention: a positive residual means the corrected audio is AHEAD "
            "of the reference.  Positions are seconds on the corrected file's own "
            "timeline; the reference time they correspond to is offset + position."
        ),
    )
    parser.add_argument("--corrected", required=True, type=Path, help="corrected WAV to verify")
    parser.add_argument("--reference", required=True, type=Path, help="reference WAV")
    parser.add_argument("--offset", required=True, type=float, help="model offset in seconds")
    parser.add_argument(
        "--scale", type=float, default=1.0, help="model time scale (default: 1.0)"
    )
    parser.add_argument(
        "--source",
        type=Path,
        default=None,
        help="optional uncorrected source WAV, used for the log-spectral comparison",
    )
    parser.add_argument(
        "--positions",
        type=float,
        nargs="*",
        default=None,
        help="measurement positions in seconds on the corrected file's timeline",
    )
    parser.add_argument(
        "--analysis-rate",
        type=int,
        default=16000,
        help="common rate for the waveform measurements (default: 16000)",
    )
    parser.add_argument(
        "--window", type=float, default=20.0, help="measurement window length in seconds"
    )
    parser.add_argument(
        "--max-lag", type=float, default=0.5, help="maximum lag searched in seconds"
    )
    parser.add_argument(
        "--min-zscore", type=float, default=4.0, help="minimum GCC-PHAT peak separation"
    )
    parser.add_argument(
        "--min-peak", type=float, default=0.05, help="minimum GCC-PHAT peak value"
    )
    parser.add_argument(
        "--disagree-ms",
        type=float,
        default=10.0,
        help="method disagreement above which a position is flagged as disputed",
    )
    method = parser.add_mutually_exclusive_group()
    method.add_argument(
        "--waveform-only", action="store_true", help="only run the GCC-PHAT measurement"
    )
    method.add_argument(
        "--envelope-only", action="store_true", help="only run the envelope measurement"
    )
    parser.add_argument(
        "--pitch",
        dest="pitch",
        action="store_true",
        default=True,
        help="include the pitch check (default: on)",
    )
    parser.add_argument(
        "--no-pitch", dest="pitch", action="store_false", help="skip the pitch check"
    )
    parser.add_argument("--json", type=Path, default=None, help="write a JSON report here")
    parser.add_argument("--markdown", type=Path, default=None, help="write a Markdown report here")
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    cfg = Config()
    cfg.analysis_rate = args.analysis_rate
    cfg.window_secs = args.window
    cfg.max_lag_secs = args.max_lag
    cfg.min_zscore = args.min_zscore
    cfg.min_peak = args.min_peak
    cfg.disagree_ms = args.disagree_ms
    if cfg.analysis_rate < 1000:
        print("error: --analysis-rate must be at least 1000 Hz", file=sys.stderr)
        return 2
    if cfg.window_secs <= 0 or cfg.max_lag_secs <= 0:
        print("error: --window and --max-lag must be positive", file=sys.stderr)
        return 2
    if cfg.max_lag_secs >= cfg.window_secs / 2.0:
        print(
            "error: --max-lag must be smaller than half the window length", file=sys.stderr
        )
        return 2

    do_waveform = not args.envelope_only
    do_envelope = not args.waveform_only

    try:
        corrected = open_wav(args.corrected)
        reference = open_wav(args.reference)
        source = open_wav(args.source) if args.source else None
    except VerifyError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    positions = args.positions
    if positions is None:
        positions = default_positions(corrected, reference, args.offset, cfg)
        if not positions:
            print(
                "error: the corrected and reference files do not overlap for a "
                f"{cfg.window_secs:.0f} s window at offset {args.offset:.3f} s",
                file=sys.stderr,
            )
            return 2

    results: list[PositionResult] = []
    backgrounds = {
        "corrected": background_level(corrected),
        "reference": background_level(reference),
    }
    for i, position in enumerate(positions):
        results.append(
            measure_position(
                i,
                float(position),
                corrected,
                reference,
                args.offset,
                args.scale,
                cfg,
                source,
                do_waveform,
                do_envelope,
                args.pitch,
                backgrounds,
            )
        )

    summary = summarise(results, cfg)
    model = {
        "scale": args.scale,
        "offset_seconds": args.offset,
        "drift_ppm": (args.scale - 1.0) * 1_000_000.0,
    }
    inputs = {
        "files": [
            {
                "path": str(corrected.path),
                "role": "corrected",
                "sample_rate": corrected.sample_rate,
                "channels": corrected.channels,
                "duration_s": corrected.duration,
                "frames": corrected.frames,
                "bits_per_sample": corrected.bits_per_sample,
            },
            {
                "path": str(reference.path),
                "role": "reference",
                "sample_rate": reference.sample_rate,
                "channels": reference.channels,
                "duration_s": reference.duration,
                "frames": reference.frames,
                "bits_per_sample": reference.bits_per_sample,
            },
        ]
        + (
            [
                {
                    "path": str(source.path),
                    "role": "uncorrected source",
                    "sample_rate": source.sample_rate,
                    "channels": source.channels,
                    "duration_s": source.duration,
                    "frames": source.frames,
                    "bits_per_sample": source.bits_per_sample,
                }
            ]
            if source
            else []
        )
    }

    print(f"{TOOL_NAME} v{TOOL_VERSION}")
    print(f"  corrected : {corrected.path}  ({corrected.sample_rate} Hz, {corrected.duration:.3f} s)")
    print(f"  reference : {reference.path}  ({reference.sample_rate} Hz, {reference.duration:.3f} s)")
    if source:
        print(f"  source    : {source.path}  ({source.sample_rate} Hz, {source.duration:.3f} s)")
    print(
        f"  model     : scale {args.scale:.12f}, offset {args.offset:.9f} s, "
        f"drift {(args.scale - 1.0) * 1_000_000.0:+.3f} ppm"
    )
    print(f"  methods   : {'GCC-PHAT' if do_waveform else ''}"
          f"{' + ' if do_waveform and do_envelope else ''}"
          f"{'envelope NCC' if do_envelope else ''}"
          f"{' + pitch' if args.pitch else ''}")
    print()
    print_table(results, cfg)
    print_summary(summary, cfg)

    if args.json:
        report = {
            "tool": TOOL_NAME,
            "version": TOOL_VERSION,
            "coordinate_system": COORDINATE_SYSTEM,
            "position_semantics": POSITION_SEMANTICS,
            "sign_convention": "positive residual = the corrected audio is ahead of the reference",
            "inputs": inputs,
            "model": model,
            "configuration": {
                "analysis_rate": cfg.analysis_rate,
                "window_seconds": cfg.window_secs,
                "max_lag_seconds": cfg.max_lag_secs,
                "band_hz": [cfg.band_lo, cfg.band_hi],
                "min_zscore": cfg.min_zscore,
                "min_peak": cfg.min_peak,
                "min_envelope_peak": cfg.min_envelope_peak,
                "disagree_ms": cfg.disagree_ms,
                "envelope_hop_seconds": cfg.envelope_hop_secs,
                "envelope_search_seconds": cfg.envelope_max_lag_secs,
                "methods": {
                    "waveform": do_waveform,
                    "envelope": do_envelope,
                    "pitch": args.pitch,
                },
            },
            "positions": [r.to_dict() for r in results],
            "summary": summary,
        }
        try:
            args.json.parent.mkdir(parents=True, exist_ok=True)
            args.json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        except OSError as exc:
            print(f"error: could not write {args.json}: {exc}", file=sys.stderr)
            return 2
        print(f"  JSON report written to {args.json}")

    if args.markdown:
        text = markdown_report(results, summary, model, inputs, cfg)
        try:
            args.markdown.parent.mkdir(parents=True, exist_ok=True)
            args.markdown.write_text(text, encoding="utf-8")
        except OSError as exc:
            print(f"error: could not write {args.markdown}: {exc}", file=sys.stderr)
            return 2
        print(f"  Markdown report written to {args.markdown}")

    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except VerifyError as exc:  # pragma: no cover - defensive
        print(f"error: {exc}", file=sys.stderr)
        sys.exit(2)
    except KeyboardInterrupt:  # pragma: no cover
        print("interrupted", file=sys.stderr)
        sys.exit(130)
