# Test videos

Run `python fixtures/generate.py` from any directory to recreate the two short MP4s and `ground-truth.json`. Python 3 and an FFmpeg build with `libx264` are required. The script uses only the Python standard library.

The view stays fixed. In `scene-motion.mp4`, a person crosses at 2.0–4.0 seconds, a door opens at 6.0–7.6 seconds, and a box moves at 10.0–11.6 seconds. `scene-still.mp4` contains the same scene with no motion. These clips exercise the detector with known timing; tune the threshold against representative real recordings before relying on it for those recordings.
