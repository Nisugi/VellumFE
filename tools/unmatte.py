#!/usr/bin/env python3
"""Recover per-pixel alpha from a white-background / black-background render pair.

Generative art tools cannot emit an alpha channel, but they can render the same
subject twice against two known backdrops.  Compositing says each render is

    white = C*a + 1*(1-a)
    black = C*a + 0*(1-a)

so subtracting gives `white - black == 1 - a`, and the premultiplied colour
falls out of the black render.  That is exact for a true pair; the only error
is source compression noise, which `--snap` cleans up at the extremes.

Two ways to name inputs:

  * suffix pairing (default) -- a directory of `<name>_white.jpg` /
    `<name>_black.jpg` files, written out as `<name>.png`.
  * explicit pairing -- `--white A.jpg --black B.jpg --out out.png`, for
    unlabelled files (e.g. generator-assigned names) where you have eyeballed
    which is which.

Requires: pillow, numpy.

Usage:
    python tools/unmatte.py <dir> [--out <dir>] [--snap 0.02]
    python tools/unmatte.py --white w.jpg --black b.jpg --out doll.png
"""

import argparse
import glob
import os
import sys

import numpy as np
from PIL import Image

# Below this alpha a pixel carries no recoverable colour: dividing the black
# render by it amplifies compression noise into confetti, so we zero it out.
# Fully transparent pixels are invisible, so the discarded colour never shows.
MIN_ALPHA = 1e-3


def measure_backdrop(img, margin=40):
    """Mean colour of the left/right edge strips, which are always backdrop.

    Generators do not reliably honour "pure white" / "pure black": a backdrop
    that renders at 0.03 instead of 0 leaves that much alpha across the whole
    canvas, because the maths assumes the two backdrops are exactly 1 and 0.
    Measuring the real levels and rescaling removes that error.
    """
    strips = np.concatenate(
        [img[:, :margin].reshape(-1, 3), img[:, -margin:].reshape(-1, 3)]
    )
    return float(strips.mean())


def unmatte(white_path, black_path, snap=0.0, calibrate=True):
    """Return an RGBA Image recovered from the two backdrop renders."""
    w = np.asarray(Image.open(white_path).convert("RGB"), dtype=np.float32) / 255.0
    b = np.asarray(Image.open(black_path).convert("RGB"), dtype=np.float32) / 255.0
    if w.shape != b.shape:
        raise ValueError(
            f"size mismatch: {os.path.basename(white_path)} is {w.shape[1]}x{w.shape[0]}, "
            f"{os.path.basename(black_path)} is {b.shape[1]}x{b.shape[0]}"
        )

    # The backdrop only survives in the difference, so averaging the three
    # channels cancels most of the per-channel JPEG ringing.
    diff = (w - b).mean(axis=2)

    if calibrate:
        # `diff` reads (white_level - black_level) over backdrop, which is 1.0
        # only for a true pair. Normalising by the measured span makes an
        # off-black or off-white backdrop land at alpha 0 as it should.
        span = measure_backdrop(w) - measure_backdrop(b)
        if span > 0.5:  # sane pair; otherwise leave the raw maths alone
            diff = diff / span

    alpha = np.clip(1.0 - diff, 0.0, 1.0)

    if snap > 0.0:
        # Lossy sources never quite reach 0 or 1; a flat background lands at
        # alpha ~= 0.004 and shows up as a grey haze once composited.
        alpha[alpha < snap] = 0.0
        alpha[alpha > 1.0 - snap] = 1.0

    # `black` is the premultiplied colour, so un-premultiply to get the real one.
    opaque = alpha >= MIN_ALPHA
    safe = np.where(opaque, alpha, 1.0)[..., None]
    color = np.clip(b / safe, 0.0, 1.0)
    color[~opaque] = 0.0

    rgba = np.dstack([color, alpha]) * 255.0
    return Image.fromarray(rgba.round().astype(np.uint8), "RGBA")


def coverage(img):
    """Fraction of pixels that are not fully transparent, for a sanity log."""
    a = np.asarray(img)[:, :, 3]
    return float((a > 0).mean())


def run_directory(src, out_dir, snap, calibrate=True):
    os.makedirs(out_dir, exist_ok=True)
    whites = sorted(glob.glob(os.path.join(src, "*_white.*")))
    if not whites:
        print(f"no *_white.* files in {src}", file=sys.stderr)
        return 1
    failures = 0
    for white in whites:
        stem = os.path.basename(white).rsplit("_white", 1)[0]
        blacks = glob.glob(os.path.join(src, stem + "_black.*"))
        if not blacks:
            print(f"skip {stem}: no matching _black file", file=sys.stderr)
            failures += 1
            continue
        try:
            img = unmatte(white, blacks[0], snap, calibrate)
        except ValueError as exc:
            print(f"skip {stem}: {exc}", file=sys.stderr)
            failures += 1
            continue
        dest = os.path.join(out_dir, stem + ".png")
        img.save(dest)
        print(f"wrote {dest}  ({coverage(img):.1%} opaque)")
    return 1 if failures else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("dir", nargs="?", help="directory of *_white / *_black pairs")
    ap.add_argument("--white", help="explicit white-background render")
    ap.add_argument("--black", help="explicit black-background render")
    ap.add_argument("--out", help="output file (explicit mode) or directory")
    ap.add_argument(
        "--snap",
        type=float,
        default=0.02,
        help="clamp alpha within this distance of 0 or 1 (default 0.02; 0 disables)",
    )
    ap.add_argument(
        "--no-calibrate",
        action="store_true",
        help="skip backdrop-level normalisation (assume exact white/black)",
    )
    args = ap.parse_args()

    if args.white or args.black:
        if not (args.white and args.black and args.out):
            ap.error("--white, --black and --out must be given together")
        img = unmatte(args.white, args.black, args.snap, not args.no_calibrate)
        out_parent = os.path.dirname(os.path.abspath(args.out))
        os.makedirs(out_parent, exist_ok=True)
        img.save(args.out)
        print(f"wrote {args.out}  ({coverage(img):.1%} opaque)")
        return 0

    if not args.dir:
        ap.error("give a directory, or --white/--black/--out")
    return run_directory(
        args.dir,
        args.out or os.path.join(args.dir, "transparent"),
        args.snap,
        not args.no_calibrate,
    )


if __name__ == "__main__":
    sys.exit(main())
