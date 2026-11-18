#!/usr/bin/env python3
"""Independently decode a StageSync mix and check what it contains.

This tool shares no code with the Rust core. It parses the RIFF/RF64 container by
hand and decodes the samples with `numpy`, so a file it accepts has been read by a
second implementation. That is the point: the exporter's own reader could agree with
the exporter's own writer about a file that is wrong.

    python tools/verify_mix.py MIX.wav [--expect-channels 2] [--expect-frames N]
                                      [--max-peak 1.0] [--check-pan]

The checks:

* the container is a WAV, in either the ordinary RIFF or the RF64 form;
* the channel count and the length are what was expected;
* the loudest sample of each channel is reported;
* the loudest sample of the whole file is reported, and it is below the limit
  unless ``--allow-overs`` is given;
* with ``--check-pan``, the left and right channels are not identical, which is
  what a mono recording panned to one side produces.

Exit status is 0 when every check passes and 1 otherwise.
"""

from __future__ import annotations

import argparse
import struct
import sys

import numpy as np

# The format tags the tool understands.
WAVE_FORMAT_PCM = 0x0001
WAVE_FORMAT_IEEE_FLOAT = 0x0003
WAVE_FORMAT_EXTENSIBLE = 0xFFFE


def read_container(path: str) -> dict:
    """Parse the header of a RIFF, RF64 or BW64 WAV file by hand."""
    with open(path, "rb") as handle:
        data = handle.read()

    if len(data) < 12:
        raise ValueError(f"{path} is too short to be a WAV file")

    magic = data[0:4]
    if magic == b"RIFF":
        riff_size = struct.unpack_from("<I", data, 4)[0]
    elif magic == b"RF64":
        riff_size = None
    else:
        raise ValueError(f"{path} does not begin with RIFF or RF64")

    if data[8:12] != b"WAVE":
        raise ValueError(f"{path} is not a WAVE file")

    offset = 12
    fmt = None
    data_offset = None
    data_size = None
    ds64_data_size = None
    ds64_sample_count = None

    while offset + 8 <= len(data):
        chunk_id = data[offset : offset + 4]
        chunk_size = struct.unpack_from("<I", data, offset + 4)[0]
        payload = offset + 8

        if chunk_id == b"ds64":
            if payload + 28 > len(data):
                raise ValueError(f"{path} has a truncated ds64 chunk")
            _riff, ds64_data_size, ds64_sample_count = struct.unpack_from("<QQQ", data, payload)
        elif chunk_id == b"fmt ":
            if payload + 16 > len(data):
                raise ValueError(f"{path} has a truncated fmt chunk")
            tag, channels, rate, _byte_rate, _align, bits = struct.unpack_from(
                "<HHIIHH", data, payload
            )
            if tag == WAVE_FORMAT_EXTENSIBLE:
                if payload + 40 > len(data):
                    raise ValueError(f"{path} has a truncated extensible fmt chunk")
                tag = struct.unpack_from("<H", data, payload + 24)[0]
            fmt = {
                "tag": tag,
                "channels": channels,
                "rate": rate,
                "bits": bits,
            }
        elif chunk_id == b"data":
            data_offset = payload
            if chunk_size == 0xFFFFFFFF:
                data_size = None
            else:
                data_size = chunk_size
            break

        # The next chunk starts after the payload, padded to an even length.
        offset = payload + chunk_size + (chunk_size & 1)

    if fmt is None:
        raise ValueError(f"{path} has no fmt chunk")
    if data_offset is None:
        raise ValueError(f"{path} has no data chunk")

    if ds64_data_size is not None:
        data_size = ds64_data_size
    if data_size is None:
        data_size = len(data) - data_offset
    data_size = min(data_size, len(data) - data_offset)

    if fmt["tag"] == WAVE_FORMAT_IEEE_FLOAT and fmt["bits"] == 32:
        dtype = np.float32
    elif fmt["tag"] == WAVE_FORMAT_PCM and fmt["bits"] == 16:
        dtype = np.int16
    elif fmt["tag"] == WAVE_FORMAT_PCM and fmt["bits"] == 32:
        dtype = np.int32
    else:
        raise ValueError(
            f"{path} uses an unsupported format: tag {fmt['tag']} with {fmt['bits']} bits"
        )

    frames = data_size // (np.dtype(dtype).itemsize * fmt["channels"])
    samples = np.frombuffer(
        data, dtype=dtype, count=frames * fmt["channels"], offset=data_offset
    )
    if dtype == np.int16:
        samples = samples.astype(np.float32) / 32768.0
    elif dtype == np.int32:
        samples = samples.astype(np.float32) / 2147483648.0
    else:
        samples = samples.astype(np.float32)

    planes = samples.reshape(frames, fmt["channels"]) if frames else samples.reshape(0, 0)
    return {
        "container": "RF64" if magic == b"RF64" else "RIFF",
        "riff_size": riff_size,
        "channels": fmt["channels"],
        "rate": fmt["rate"],
        "bits": fmt["bits"],
        "frames": frames,
        "planes": planes,
        "sample_count": ds64_sample_count,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mix", help="the mixed WAV file to check")
    parser.add_argument("--expect-channels", type=int, default=None)
    parser.add_argument("--expect-frames", type=int, default=None)
    parser.add_argument("--max-peak", type=float, default=1.0)
    parser.add_argument(
        "--allow-overs",
        action="store_true",
        help="allow samples above the limit (a float file can hold them)",
    )
    parser.add_argument(
        "--check-pan",
        action="store_true",
        help="require the two channels to differ, as a panned mix does",
    )
    arguments = parser.parse_args()

    try:
        info = read_container(arguments.mix)
    except (OSError, ValueError) as error:
        print(f"FAILED: {error}")
        return 1

    planes = info["planes"]
    failures = []

    print(f"container:   {info['container']}")
    print(f"channels:    {info['channels']}")
    print(f"rate:        {info['rate']} Hz")
    print(f"bits:        {info['bits']}")
    print(f"frames:      {info['frames']}")

    peaks = []
    for channel in range(info["channels"]):
        if planes.shape[0] == 0:
            peaks.append(0.0)
            continue
        peaks.append(float(np.max(np.abs(planes[:, channel]))))
        print(f"peak ch {channel}: {peaks[-1]:.6f} ({20 * np.log10(max(peaks[-1], 1e-12)):.2f} dBFS)")

    overall = max(peaks) if peaks else 0.0
    print(f"peak:        {overall:.6f} ({20 * np.log10(max(overall, 1e-12)):.2f} dBFS)")

    if arguments.expect_channels is not None and info["channels"] != arguments.expect_channels:
        failures.append(
            f"the file has {info['channels']} channels and {arguments.expect_channels} were expected"
        )
    if arguments.expect_frames is not None and info["frames"] != arguments.expect_frames:
        failures.append(
            f"the file has {info['frames']} frames and {arguments.expect_frames} were expected"
        )
    if not arguments.allow_overs and overall > arguments.max_peak:
        failures.append(
            f"the loudest sample is {overall:.6f}, which is above the limit of {arguments.max_peak}"
        )
    if arguments.check_pan:
        if info["channels"] < 2:
            failures.append("the file has fewer than two channels, so it cannot be panned")
        elif planes.shape[0] > 0 and np.array_equal(planes[:, 0], planes[:, 1]):
            failures.append("the two channels are identical, so nothing was panned")

    for failure in failures:
        print(f"FAILED: {failure}")
    if failures:
        return 1
    print("OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
