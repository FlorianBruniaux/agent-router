# Authored public fixture, never model inference.
import pathlib
import sys
calls = pathlib.Path(".scratch/calls.txt")
count = int(calls.read_text()) if calls.exists() else 0
calls.write_text(str(count + 1))
sys.stdout.buffer.write(("\x1b[31m" * 80 + "RED é\r\n" + "\x1b[0m").encode("utf-8"))
sys.stderr.buffer.write(b"detail\n")
raise SystemExit(1)
