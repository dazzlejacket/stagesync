# PUBLIC_DSP_REFERENCES.md — independent public research for MiniMax (clean-room)

> **CLEAN-ROOM DOCUMENT.** Everything here is publicly available research or open-source
> software, selected to implement the behaviors in CLEANROOM_SPEC.md **without** any
> reference to proprietary implementation. Section numbers correspond to references made
> in CLEANROOM_SPEC §5.

## 1. Alignment path search (DTW family & robust variants)

- Sakoe, H. & Chiba, S. (1978). "Dynamic programming algorithm optimization for spoken
  word recognition." *IEEE Trans. ASSP*, 26(1), 43–49. — classic windowed DTW (basis for
  bounded-drift alignment).
- Myers, C. & Rabiner, L. (1981). "A subword connected digit recognizer using DTW."
  *IEEE Trans. ASSP*, 29(4). — relaxed-step DTW (allowable step patterns).
- Müller, M. & Ewert, S. (2011). "Chroma-based audio features… robust music tempo estimation"
  and the *Tuner* work: Müller, M. & Ewert, S. (2010). "Vamp plug-in … 'tuning-frequency
  appearance'"; see also Müller, M. (2007). *Information Retrieval for Music and Motion*, ch. 3
  (DTW, derivative DTW, implementation with step patterns and bounds).
- Sarto, C. L., Bello, J. P., & Monti, J. (2020). "Optimal time warping for automatic
  singing assessment." *IEEE/ACM TASLP*, 28. — **OTW: DTW where the time-modification
  function itself is optimized under bounded stretch rate**, directly relevant to
  F3/F7 (bounded, artifact-minimized warping).
- Cuturi, M. & Blondel, M. (2017). "Soft-DTW: a differentiable loss function for
  time-series." *ICML*. — differentiable, robust alignment; useful for learned variants.
- Salamon, J., Jacoby, E., & Bello, J. P. (2014). "A taxonomy and evaluation of dynamic
  time warping variants for music score alignment." *ISMIR*. — subsequence/windowed
  variants for partial alignment (F10).
- Eilers, P. H. C. (2003). "Analyzing compositional data with rational splines" (and the
  *fda* literature) — smooth monotone warp-map regularization.
- Needleman–Sunsch/Wunsch-style global alignment with affine gaps (Gotoh 1982) —
  for silence/insertion handling (F10).
- Beam search & A*: Hart, Nilsson & Raphael (1968); classic Viterbi (Forney 1973) —
  pruned path search for 2-hour-scale inputs (F12).

## 2. Frame-level audio features

- Davis, S. & Mermelstein, P. (1980). "Comparison of parametric representations for
  monosyllabic word recognition." *IEEE Trans. ASSP*. — MFCC.
- Log-mel spectrogram features: Slaney, M. (1998). *Auditory Toolbox* (Apple TR).
- Ellis, D. P. W. (2007). "Chroma feature extraction" (MATLAB/Octave toolbox; ISMIR tutorial).
- Fu, G. et al. (2011). "A survey of audio fingerprinting" (or Cano, P. et al. (2005).
  "A review of audio fingerprinting." *J. VLSI SP Syst.*) — robust content features
  for cross-device matching (T5).
- Haitsma, J. & Kalker, T. (2002). "A highly robust audio fingerprinting system." *ISMIR*.
- Wang, A. (2003). "An industrial strength audio search algorithm." *ISMIR* (Shazam) —
  landmark/peak hashing for fast global offset lock (F2).

## 3. Pitch detection & presence

- de Cheveigné, A. & Kawahara, H. (2002). "YIN, a fundamental frequency estimator for
  speech and music." *JASA*, 111(4). — recommended baseline (independent tracker).
- Mauch, M. & Dixon, S. (2010). "pYIN: a fundamental frequency estimator using
  probabilistic threshold crossings." *ISMIR*. — voiced/unvoiced + interpolation
  semantics matching "measured vs interpolated" UX.
- Kim, J. W. et al. (2018). "CREPE: a convolutional representation for pitch tracking."
  *ICASSP*. — NN-based F0; viable path for a modern vibrato/pitch model (spec §3, ML note).
- Alwan, A., Jiang, J., & Chen, W. (2011) and general F0-smoothing literature
  (e.g., smoothed contours via median + spline) — for `SmoothPitch`/`Variation` semantics.
- Seppänen, T. (2001). "Tremolo and vibrato analysis for computer music applications."
  *Computer Music Journal*, 25(2). — public vibrato rate/depth estimation for the
  vibrato-protection feature.
- Klapuri, A. (2003). "Multiple F0 estimation…" (ISMIR/DAFx) — multi-pitch for harmonies.

## 4. Time stretching / local timing correction

- Portnoff, M. (1981). "Time-scale modification of waveforms without an analysis/ynthesis
  phase" — and Moorer, J. (1976). "Computer synthesizing music with a computer."
  *Computer Music Journal* 1(2). — phase-vocoder lineage.
- Laroche, J. & Dolson, M. (1999). "Improved phase vocoder time-scale modification of
  audio." *IEEE Trans. SAP*, 7(3). — phase-vocoder improvements (transient/phase locking).
- Verfaille, V., Arfib, D., & Keiler, U. (2001). "Adaptive digital time-stretching…"
  (DAFx) — and the WSOLA family: Niemitalo, M.-J. (1990) post-facto WSOLA;
  Laroche & Dolson's "WSOLA" treatment; Radford & … — **WSOLA-class OLA with
  best-lag search is the recommended public method for vocal timing edits** (low artifact,
  matches "tight timing without pitch change" behavior).
- Charpentier, F. (1986). "Diphone synthesis using an overlap-add technique" (PSOLA),
  *ICASSP*; Moulines, E. & Laroche, J. (1995). "Non-subsampled sinusoidal analysis-
  synthesis…" *IEEE Trans. SAP*. — TD-PSOLA for pitch transfer (F4).
- Fitzgerald, D. & Paulus, J. (2007) and Herrera & Gómez (2006) surveys of singing
  analysis/synthesis — orientation for vocal-grade quality bars.
- Laroche, J. (1999). "Efficient analysis and synthesis of musical instruments using
  sinusoidal modeling" and formant-preservation methods for PSOLA (e.g., level
  spectral-envelope matching during pitch shift) — for `FormantsEditing`.

## 5. Transient detection & protection

- Bello, J. P. et al. (2005). "A tutorial on onset detection in music signals."
  *IEEE Trans. SAP*, 13(5). — spectral flux, complex domain methods.
- Stowell, D. & Plumbley, M. (2007). "An adaptive plucker… " — better: Stowell &
  Plumbley, "Fast multidomain transient detection" (DAFx 2007).
- Duxbury, C., Bello, J. P., Davies, M., & Plumbley, M. (2001). "Joint time-frequency
  segmentation…" *DARPA*. — transient separation for protection (F9).

## 6. Level/loudness transfer

- ITU-R BS.1770-4 (2015) / EBU R128 (loudness metering) — pyloudnorm (LoudnessMeter
  implementation by Aaron C.) for measurements and for `UseDubForAverageLevel` semantics.
- Giannoulis, D., Massberg, E., & Reiss, J. (2012). "Dynamic range compression…
  tutorial." *JAES* — attack/decay dynamics design for gain envelopes (F5).

## 7. Stereo & artifact control

- Cabral, J. & Zölzer, U. (2012). "Stereo width measurement and control" (DAFx/AES
  e-lib) and general mid-side correlation practice per AES conventions;
- Haas-effect/precedence literature (Haas 1951; Blauert 1971) for why linked
  inter-channel delay must be preserved (F14);
- AES/DAFx e-lib papers on phase-coherent stereo time-stretching ("stereo time-stretch
  with phase preservation") — search DAFx proceedings 2005–2020.

## 8. Resampling & rendering

- Crochiere, R. & Rabiner, L. (1983). *Multirate Digital Signal Processing*. — polyphase
  windowed-sinc resamplers.
- Open components: libsamplerate (Secret Rabbit Code, GPL-2); SoX resamplers; IPP-like
  alternatives: Ooura FFT (free), KissFFT (BSD), pffft (BSD), IPP under commercial license.
- Välimäki, V. & Holm-Rasmussen, B. (2021?) — skip; use: Laroche & Dolson transient
  handling above for the render stage.

## 9. Long-form / streaming alignment

- Gkiokas, A. & Giannakopoulos, T. (2022) "Deep subsequence alignment…" style works;
  Salamon et al. (2014) §1; windowed DTW with overlap-and-blend (standard practice in
  chorus-line estimation, e.g., Gillet, O. & Richard, G. (2008). "Transcription and
  separation of drum recordings…" *IEEE TASLP*) — chunked alignment + path stitching
  for 2-hour inputs (F12).

## 10. Open-source components under permissive/copyleft licenses (candidates)

- **Rubber Band Library** (breakfastquay, GPL-2/commercial) — high-quality pitch/tempo
  library; usable if MiniMax is GPL.
- **SoundTouch** (Oliver Larkin, LGPL-2.1) — WSOLA/TD-ALT time-stretch.
- **librosa / librosa.effects** (ISC) — assessment + baseline stretching.
- **Crepe / pYIN (librosa.pyin)** — pitch trackers.
- **pffft / KissFFT / Ooura** — FFTs. **libsamplerate** (GPL-2) or SoX-style polyphase.
- **pyloudnorm** (MIT) — BS.1770 loudness.

## 11. Methodology for calibration

- Use BEHAVIOR_TESTS metrics to compare candidate public methods against the reference
  product's observed behavior; select and tune from public literature only.
- Record all chosen hyperparameters in implementation repo; keep this clean-room
  boundary (no reference-product internals) auditable per PROVENANCE.md.
