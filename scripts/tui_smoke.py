#!/usr/bin/env python3
"""End-to-end test of `vibabel edit` through a pseudo-terminal.

Sends key sequences to the editor, reconstructs the last frame drawn and
checks it together with the saved page. Standard library only.

Usage:
    cargo build && python3 scripts/tui_smoke.py            # run the checks
    python3 scripts/tui_smoke.py --show ROWS COLS KEYS...   # just print a frame

In KEYS, ESC means Escape and CR means Enter, e.g.
    python3 scripts/tui_smoke.py --show 47 90 ihello ESC :w CR
"""

import fcntl
import os
import pty
import re
import select
import struct
import sys
import tempfile
import termios
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BINARY = os.path.join(ROOT, "target", "debug", "vibabel")
LOCATION = ["-x", "deadbeef", "-w", "1", "-s", "2", "-b", "3", "-p", "4"]


def drive(keys, rows, cols, cwd):
    """Run `vibabel edit` on a rows x cols pty, send keys, return output."""
    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(cwd)
        os.execv(BINARY, [BINARY, "edit", *LOCATION])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    output = b""

    def pump(seconds):
        nonlocal output
        deadline = time.time() + seconds
        while time.time() < deadline:
            ready, _, _ = select.select([fd], [], [], 0.05)
            if ready:
                try:
                    output += os.read(fd, 65536)
                except OSError:  # child exited
                    return

    pump(0.3)
    for key in keys:
        os.write(fd, key.encode())
        pump(0.05)
    pump(0.4)
    os.close(fd)  # hangs up the editor if it's still running
    os.waitpid(pid, 0)
    return output


def render(output, rows, cols):
    """Replay the last frame into a character grid. Interprets cursor
    positioning (H) and erasure (K, J); ignores other sequences."""
    text = output.decode("utf-8", "replace")
    text = text[text.rfind("\x1b[?2026h"):]  # last synchronized update
    grid = [[" "] * cols for _ in range(rows)]
    row = col = 0
    i = 0
    while i < len(text):
        if text[i] == "\x1b":
            match = re.match(r"\x1b\[([\d;?]*) ?([A-Za-z])", text[i:])
            if match:
                params, final = match.groups()
                if final == "H":
                    row, col = [int(p) - 1 for p in params.split(";")] if params else [0, 0]
                elif final in "KJ" and row < rows:
                    grid[row][col:] = [" "] * (cols - col)
                    if final == "J":
                        for r in range(row + 1, rows):
                            grid[r] = [" "] * cols
                i += len(match.group(0))
            else:
                i += 1
            continue
        if text[i] not in "\r\n":
            if row < rows and col < cols:
                grid[row][col] = text[i]
            col += 1
        i += 1
    return "\n".join("".join(line).rstrip() for line in grid)


def expand(keys):
    return [k.replace("ESC", "\x1b").replace("CR", "\r") for k in keys]


def check():
    with tempfile.TemporaryDirectory() as cwd:
        # Typing, with foreign symbols; save and quit.
        keys = expand(["iHello, World! the library.", "CR", "  line two", "ESC",
                       "G", "A", "last line", "ESC", ":wq", "CR"])
        drive(keys, 47, 90, cwd)
        files = os.listdir(cwd)
        assert len(files) == 1, files
        name = files[0]
        assert name == "G9W33eEQD", name
        with open(os.path.join(cwd, name)) as f:
            page = f.read()
        lines = page.split("\n")
        assert len(page) == 3240, len(page)
        assert lines[0].rstrip() == "ello, orld the library.", lines[0]
        assert lines[1].rstrip() == "  line two", lines[1]
        assert lines[39].rstrip() == "last line", lines[39]
        print("saved page: ok")

        # Reopen and overwrite.
        screen = render(drive(expand(["ixyz", "ESC"]), 47, 90, cwd), 47, 90)
        assert "VIBABEL" in screen and "PAGE 4/410" in screen, screen
        assert " 1 │xyzo, orld the library." in screen, screen
        assert "NORMAL  [+]" in screen, screen
        print("full-size frame: ok")

        # A small terminal scrolls to keep the cursor in view.
        screen = render(drive(expand(["G$rq"]), 16, 40, cwd), 16, 40)
        assert "40 │" in screen and "q│" in screen, screen
        print("scrolled frame: ok")

        # dw, then put the deleted word over the next line.
        drive(expand(["ggwwdwjp", ":wq", "CR"]), 47, 90, cwd)
        with open(os.path.join(cwd, name)) as f:
            lines = f.read().split("\n")
        assert lines[0].rstrip() == "ello, the library.", lines[0]
        assert lines[1].rstrip() == "  lineorld", lines[1]
        print("dw and p: ok")


if __name__ == "__main__":
    if not os.path.exists(BINARY):
        sys.exit(f"build first: {BINARY} not found")
    if len(sys.argv) > 3 and sys.argv[1] == "--show":
        rows, cols = int(sys.argv[2]), int(sys.argv[3])
        with tempfile.TemporaryDirectory() as cwd:
            print(render(drive(expand(sys.argv[4:]), rows, cols, cwd), rows, cols))
    else:
        check()
