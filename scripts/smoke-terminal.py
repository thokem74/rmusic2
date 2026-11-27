#!/usr/bin/env python3
"""Exercise the built app in isolated PTYs; no real station or saved user state."""

import fcntl
import os
from pathlib import Path
import pty
import select
import signal
import struct
import tempfile
import termios
import time


class Session:
    def __init__(self, state_home):
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            binary = str(Path("target/debug/rmusic").resolve())
            os.execve(binary, [binary], dict(os.environ, XDG_STATE_HOME=state_home,
                                           TERM="xterm-256color"))
        self.initial = termios.tcgetattr(self.fd)
        self.resize(80, 24)
        frame = self.read(0.6)
        assert b"Favorites" in frame and b"Stopped" in frame, "startup frame missing"

    def read(self, seconds=0.2):
        data = b""
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if select.select([self.fd], [], [], 0.03)[0]:
                try:
                    data += os.read(self.fd, 65536)
                except OSError:
                    break
        return data

    def resize(self, width, height):
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
        os.kill(self.pid, signal.SIGWINCH)

    def exit(self, key=None, sig=None):
        if key is not None:
            os.write(self.fd, key)
        if sig is not None:
            os.kill(self.pid, sig)
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            found, status = os.waitpid(self.pid, os.WNOHANG)
            if found:
                self.pid = None
                assert os.waitstatus_to_exitcode(status) == 0, status
                mask = termios.ECHO | termios.ICANON
                assert termios.tcgetattr(self.fd)[3] & mask == self.initial[3] & mask
                return
            self.read(0.05)
        raise AssertionError("rmusic did not exit")

    def close(self):
        if self.pid is not None:
            try:
                os.kill(self.pid, signal.SIGKILL)
                os.waitpid(self.pid, 0)
            except ProcessLookupError:
                pass
        os.close(self.fd)


with tempfile.TemporaryDirectory(prefix="rmusic-terminal-smoke-") as state:
    app = Session(state + "/resize")
    try:
        os.write(app.fd, b"/qf ")
        app.read()
        assert os.waitpid(app.pid, os.WNOHANG)[0] == 0, "text shortcut quit the app"
        app.resize(60, 12)
        assert b"80x24" in app.read(0.4), "resize fallback missing"
        app.exit(key=b"q")
        print("PASS: startup, filter typing, small-terminal quit, terminal restoration")
    finally:
        app.close()

    app = Session(state + "/suspend")
    try:
        os.write(app.fd, b"\x1a")
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            if "\nState:\tT" in Path(f"/proc/{app.pid}/status").read_text():
                break
            app.read(0.05)
        else:
            raise AssertionError("Ctrl+Z did not suspend")
        assert termios.tcgetattr(app.fd)[3] & termios.ICANON
        os.kill(app.pid, signal.SIGCONT)
        assert b"Stopped" in app.read(0.6), "silent resume frame missing"
        app.exit(key=b"q")
        print("PASS: Ctrl+Z, terminal restoration, SIGCONT, silent resume")
    finally:
        app.close()

    for name, sig in [("SIGINT", signal.SIGINT), ("SIGTERM", signal.SIGTERM),
                      ("SIGHUP", signal.SIGHUP)]:
        app = Session(state + "/" + name)
        try:
            app.exit(sig=sig)
            print(f"PASS: {name}, clean exit, terminal restoration")
        finally:
            app.close()
