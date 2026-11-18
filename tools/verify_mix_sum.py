#!/usr/bin/env python3
"""Check that a mixed file is the sum of its stems, with the pan law applied.

The stems are rendered by the timeline renderer and the mix by the mixer's own
streaming renderer, so the two are produced by different code paths. This script
decodes both with `numpy` and checks that the mix really is the sum of the
stems, weighted the way the pan law says. It shares no code with the Rust core.

    python tools/verify_mix_sum.py MIX.wav STEM1.wav STEM2.wav ...

Every stem must have the same length as the mix. A mono stem is expected in
both output channels at the equal-power centre gain of 0.7071; a stereo stem
keeps its own left and right channels (the balance law at centre is unity).
"""

from __future__ import annotations

import argparse
import sys

import numpy as np

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0])
from verify_mix import read_container  # noqa: E402

# The equal-power centre gain: -3.01 dB.
CENTRE = 1.0 / np.sqrt(2.0)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mix")
    parser.add_argument("stems", nargs="+")
    arguments = parser.parse_args()

    mix = read_container(arguments.mix)
    print(f"mix:    {mix['channels']} channel(s), {mix['frames']} frames, {mix['rate']} Hz")

    expected = np.zeros((mix["frames"], mix["channels"]), dtype=np.float64)
    for stem_path in arguments.stems:
        stem = read_container(stem_path)
        name = stem_path.replace("\\", "/").rsplit("/", 1)[-1]
        if stem["frames"] != mix["frames"]:
            print(f"FAILED: {name} has {stem['frames']} frames, the mix has {mix['frames']}")
            return 1
        if stem["rate"] != mix["rate"]:
            print(f"FAILED: {name} is at {stem['rate']} Hz, the mix is at {mix['rate']} Hz")
            return 1
        if stem["channels"] == 1:
            # A mono recording at the centre of the equal-power law.
            gain = CENTRE
            for channel in range(min(2, mix["channels"])):
                expected[:, channel] += gain * stem["planes"][:, 0]
            print(f"stem {name}: mono, added at {gain:.4f} to both channels")
        else:
            # A stereo recording keeps its own channels: the balance law at the
            # centre is unity.
            for channel in range(min(2, mix["channels"])):
                expected[:, channel] += stem["planes"][:, channel]
            print(f"stem {name}: stereo, added with its own left and right channels")

    actual = mix["planes"].astype(np.float64)
    difference = actual - expected
    scale = max(float(np.max(np.abs(expected))), 1e-12)
    rms = float(np.sqrt(np.mean(difference * difference)))
    worst = float(np.max(np.abs(difference)))
    print(f"peak of the expected sum: {scale:.6f}")
    print(f"rms difference:           {rms:.6f} ({20 * np.log10(max(rms / scale, 1e-12)):.2f} dB of the peak)")
    print(f"worst difference:         {worst:.6f} ({20 * np.log10(max(worst / scale, 1e-12)):.2f} dB of the peak)")

    # The two renderers are not the same code path, so they are not expected to
    # be bit-identical. They are expected to agree to well within a tenth of a
    # percent of the peak, which is far below anything audible.
    if rms / scale > 1e-3:
        print("FAILED: the mix is not the sum of its stems")
        return 1
    print("OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
