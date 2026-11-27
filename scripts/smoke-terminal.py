#!/usr/bin/env python3
"""Exercise the built app in isolated Linux PTYs, without network or user state."""

import argparse
import fcntl
import math
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import sys
import tempfile
import termios
import time


ROOT = Path(__file__).resolve().parents[1]


class SmokeFailure(Exception):
    pass


class Session:
    def __init__(self, binary, state_home, timeout):
        self.pid = None
        self.fd = None
        self.returncode = None
        self.timeout = timeout
        self.transcript = bytearray()
        try:
            temporary = Path(state_home) / "tmp"
            temporary.mkdir(parents=True)
            self.fd, slave = pty.openpty()
            try:
                # Capture the shell modes before the child can enable raw mode.
                self.initial = termios.tcgetattr(slave)
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
                self.pid = os.fork()
                if self.pid == 0:
                    try:
                        os.close(self.fd)
                        os.login_tty(slave)
                        os.execve(str(binary), [str(binary)],
                                  dict(os.environ, XDG_STATE_HOME=str(Path(state_home) / "state"),
                                       TMPDIR=str(temporary),
                                       TERM="xterm-256color"))
                    except BaseException as error:
                        os.write(2, f"Cannot start smoke binary: {error}\n".encode())
                        os._exit(127)
            finally:
                os.close(slave)
            self.expect(lambda data: b"Favorites" in data and b"Stopped" in data,
                        "startup frame")
        except BaseException:
            self.close()
            raise

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def poll(self):
        if self.pid is not None:
            found, status = os.waitpid(self.pid, os.WNOHANG)
            if found:
                self.pid = None
                self.returncode = os.waitstatus_to_exitcode(status)
        return self.returncode

    def read(self, timeout):
        if not select.select([self.fd], [], [], timeout)[0]:
            return b""
        try:
            data = os.read(self.fd, 65536)
        except OSError:
            data = b""
        self.transcript.extend(data)
        del self.transcript[:-65536]
        return data

    def failure(self, description):
        tail = bytes(self.transcript[-4000:]).decode(errors="replace")
        return SmokeFailure(f"{description}\nRecent terminal output:\n{tail}")

    def expect(self, predicate, description):
        data = bytearray()
        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            data.extend(self.read(min(0.05, max(0, deadline - time.monotonic()))))
            del data[:-65536]
            if predicate(bytes(data)):
                return
            if self.poll() is not None:
                raise self.failure(f"{description}: child exited with status {self.returncode}")
        raise self.failure(f"{description}: timed out after {self.timeout:g}s")

    def send(self, data):
        if self.poll() is not None:
            raise self.failure("Cannot send input: child already exited")
        os.write(self.fd, data)

    def resize(self, width, height):
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
        os.kill(self.pid, signal.SIGWINCH)

    def suspend(self):
        self.send(b"\x1a")
        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            if self.poll() is not None:
                raise self.failure("suspend: child exited")
            status = Path(f"/proc/{self.pid}/status").read_text()
            if any(line.startswith("State:\tT") for line in status.splitlines()):
                self.check_terminal_modes()
                # The child is stopped: drain old frames so resume cannot match
                # a buffered pre-suspension "Stopped" label and send input early.
                while self.read(0):
                    pass
                return
            self.read(0.03)
        raise self.failure("suspend: timed out waiting for stopped process")

    def check_terminal_modes(self):
        mask = termios.ECHO | termios.ICANON
        if termios.tcgetattr(self.fd)[3] & mask != self.initial[3] & mask:
            raise self.failure("terminal modes were not restored")

    def exit(self, key=None, sig=None):
        if key is not None:
            self.send(key)
        if sig is not None:
            os.kill(self.pid, sig)
        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            if self.poll() is not None:
                if self.returncode != 0:
                    raise self.failure(f"exit: child returned {self.returncode}")
                self.check_terminal_modes()
                return
            self.read(0.03)
        raise self.failure("exit: timed out waiting for shutdown")

    def close(self):
        try:
            if self.pid is not None:
                pid = self.pid
                # Kill the owned session, including any children it spawned.
                try:
                    os.killpg(pid, signal.SIGKILL)
                except ProcessLookupError:
                    try:
                        os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                deadline = time.monotonic() + 2
                while self.poll() is None:
                    if time.monotonic() >= deadline:
                        raise SmokeFailure(f"cleanup: could not reap child {pid}")
                    time.sleep(0.01)
        finally:
            if self.fd is not None:
                os.close(self.fd)
                self.fd = None


def interrupted(signum, _frame):
    raise SystemExit(128 + signum)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/rmusic")
    parser.add_argument("--timeout", type=float, default=5.0,
                        help="deadline in seconds for each terminal condition (default: 5)")
    options = parser.parse_args()
    if not math.isfinite(options.timeout) or options.timeout <= 0:
        parser.error("--timeout must be finite and positive")
    binary = options.binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error(f"smoke binary is missing or not executable: {binary}")
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGHUP, interrupted)

    with tempfile.TemporaryDirectory(prefix="rmusic-terminal-smoke-") as state:
        print("RUN: startup, filter typing, resize and quit", flush=True)
        with Session(binary, state + "/resize", options.timeout) as app:
            app.send(b"/")
            app.expect(lambda data: b"Search filters" in data, "search form")
            app.send(b"qf ")
            app.expect(lambda data: b"qf" in re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", data),
                       "literal shortcut text")
            app.resize(60, 12)
            app.expect(lambda data: b"80x24" in data, "resize fallback")
            app.exit(key=b"q")
        print("PASS: startup, filter typing, small-terminal quit, terminal restoration", flush=True)

        print("RUN: suspension and silent resume", flush=True)
        with Session(binary, state + "/suspend", options.timeout) as app:
            app.suspend()
            os.kill(app.pid, signal.SIGCONT)
            app.expect(lambda data: b"Stopped" in data, "silent resume frame")
            app.exit(key=b"q")
        print("PASS: Ctrl+Z, terminal restoration, SIGCONT, silent resume", flush=True)

        for name, sig in [("SIGINT", signal.SIGINT), ("SIGTERM", signal.SIGTERM),
                          ("SIGHUP", signal.SIGHUP)]:
            print(f"RUN: {name} shutdown", flush=True)
            with Session(binary, state + "/" + name, options.timeout) as app:
                app.exit(sig=sig)
            print(f"PASS: {name}, clean exit, terminal restoration", flush=True)


if __name__ == "__main__":
    try:
        main()
    except SmokeFailure as error:
        print(f"FAIL: {error}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        print("FAIL: smoke check interrupted", file=sys.stderr)
        sys.exit(130)
