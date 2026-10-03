"""Recreate the small fixed-camera MP4 fixtures using Python and FFmpeg."""

import json
import subprocess
from pathlib import Path

WIDTH = 160
HEIGHT = 90
FPS = 10
ROOT = Path(__file__).resolve().parent


def rectangle(frame, x0, y0, x1, y1, color):
    x0, x1 = max(0, x0), min(WIDTH, x1)
    y0, y1 = max(0, y0), min(HEIGHT, y1)
    if x0 >= x1 or y0 >= y1:
        return
    row = bytes(color) * (x1 - x0)
    for y in range(y0, y1):
        offset = (y * WIDTH + x0) * 3
        frame[offset : offset + len(row)] = row


def background():
    frame = bytearray(bytes((205, 212, 215)) * WIDTH * HEIGHT)
    rectangle(frame, 0, 69, WIDTH, HEIGHT, (135, 142, 142))
    rectangle(frame, 0, 68, WIDTH, 70, (91, 96, 97))
    rectangle(frame, 20, 14, 44, 34, (70, 76, 78))
    rectangle(frame, 22, 16, 42, 32, (103, 157, 184))
    rectangle(frame, 108, 23, 145, 70, (65, 69, 69))
    rectangle(frame, 111, 26, 142, 68, (45, 49, 52))
    return frame


def scene(time_seconds):
    frame = background()

    door_progress = min(1.0, max(0.0, (time_seconds - 6.0) / 1.6))
    door_width = round(26 - 23 * door_progress)
    rectangle(frame, 112, 26, 112 + door_width, 68, (133, 83, 52))

    box_progress = min(1.0, max(0.0, (time_seconds - 10.0) / 1.6))
    box_x = round(55 + 48 * box_progress)
    rectangle(frame, box_x, 55, box_x + 17, 68, (210, 145, 53))
    rectangle(frame, box_x + 2, 57, box_x + 15, 60, (229, 175, 76))

    if 2.0 <= time_seconds < 4.0:
        person_x = round(-8 + 108 * (time_seconds - 2.0) / 2.0)
        rectangle(frame, person_x + 3, 28, person_x + 11, 37, (58, 49, 46))
        rectangle(frame, person_x + 1, 37, person_x + 13, 58, (40, 72, 101))
        rectangle(frame, person_x + 2, 58, person_x + 6, 68, (36, 41, 45))
        rectangle(frame, person_x + 8, 58, person_x + 12, 68, (36, 41, 45))

    return bytes(frame)


def encode(path, frames):
    command = [
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
        "-f", "rawvideo", "-pix_fmt", "rgb24", "-s:v", f"{WIDTH}x{HEIGHT}",
        "-r", str(FPS), "-i", "pipe:0", "-an", "-c:v", "libx264",
        "-threads", "1", "-crf", "18", "-g", str(FPS),
        "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(path),
    ]
    subprocess.run(command, input=b"".join(frames), check=True)


def main():
    encode(ROOT / "scene-motion.mp4", (scene(index / FPS) for index in range(14 * FPS)))
    still = scene(0.0)
    encode(ROOT / "scene-still.mp4", (still for _ in range(3 * FPS)))
    truth = {
        "events": [
            {"name": "person crossing", "start_seconds": 2.0, "end_seconds": 4.0},
            {"name": "door opening", "start_seconds": 6.0, "end_seconds": 7.6},
            {"name": "box moving", "start_seconds": 10.0, "end_seconds": 11.6},
        ]
    }
    (ROOT / "ground-truth.json").write_text(
        json.dumps(truth, indent=2) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
