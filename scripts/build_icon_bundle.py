"""Build compact LorePickup thumbnails from the public Elden Ring media dump."""

from pathlib import Path
import argparse

from PIL import Image


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--size", type=int, default=96)
    args = parser.parse_args()

    args.destination.mkdir(parents=True, exist_ok=True)
    files = sorted(args.source.glob("MENU_Knowledge_*.png"))
    if not files:
        raise SystemExit(f"No inventory icons found under {args.source}")

    for source in files:
        with Image.open(source) as image:
            image.thumbnail((args.size, args.size), Image.Resampling.LANCZOS)
            image.save(args.destination / source.name, optimize=True)

    print(f"Built {len(files)} inventory thumbnails.")


if __name__ == "__main__":
    main()
