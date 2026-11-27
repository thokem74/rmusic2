#!/usr/bin/env python3
"""Exercise the built app in isolated Linux PTYs, without network or user state."""

import argparse
from contextlib import contextmanager
import fcntl
import math
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import shutil
import subprocess
import struct
import sys
import tempfile
import termios
import time
import wave


ROOT = Path(__file__).resolve().parents[1]


class SmokeFailure(Exception):
    pass


class Session:
    def __init__(self, binary, state_home, timeout, environment=None):
        self.pid = None
        self.group_id = None
        self.fd = None
        self.returncode = None
        self.timeout = timeout
        self.transcript = bytearray()
        try:
            temporary = Path(state_home) / "tmp"
            temporary.mkdir(parents=True)
            child_environment = dict(os.environ)
            child_environment.update(environment or {})
            child_environment.update(XDG_STATE_HOME=str(Path(state_home) / "state"),
                                     TMPDIR=str(temporary), TERM="xterm-256color")
            self.fd, slave = pty.openpty()
            try:
                # Capture the shell modes before the child can enable raw mode.
                self.initial = termios.tcgetattr(slave)
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
                self.pid = os.fork()
                if self.pid:
                    self.group_id = self.pid
                if self.pid == 0:
                    try:
                        os.close(self.fd)
                        os.login_tty(slave)
                        os.execve(str(binary), [str(binary)], child_environment)
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
            # The leader may already be reaped while descendants still own its
            # session. Retain that group identity independently of self.pid.
            group = self.group_id or self.pid
            if group is not None:
                try:
                    os.killpg(group, signal.SIGKILL)
                except ProcessLookupError:
                    if self.pid is not None:
                        try:
                            os.kill(self.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                self.group_id = None
            if self.pid is not None:
                pid = self.pid
                deadline = time.monotonic() + 2
                while self.poll() is None:
                    if time.monotonic() >= deadline:
                        raise SmokeFailure(f"cleanup: could not reap child {pid}")
                    time.sleep(0.01)
        finally:
            if self.fd is not None:
                os.close(self.fd)
                self.fd = None


@contextmanager
def local_audio(state_home):
    """Serve silence and record the real null-output mpv owned by the app."""
    directory = Path(state_home)
    directory.mkdir()
    with wave.open(str(directory / "silence.wav"), "wb") as audio:
        audio.setnchannels(1)
        audio.setsampwidth(2)
        audio.setframerate(44100)
        audio.writeframes(bytes(44100 * 2 * 20))
    player = shutil.which("mpv")
    if player is None:
        raise SmokeFailure("local playback/quit requires mpv")
    wrapper_directory = directory / "bin"
    wrapper_directory.mkdir()
    pid_file = directory / "player.pid"
    wrapper = wrapper_directory / "mpv"
    wrapper.write_text(
        f"#!{sys.executable}\nimport os,pathlib,sys\n"
        f"pathlib.Path({str(pid_file)!r}).write_text(str(os.getpid()))\n"
        f"os.execv({player!r}, [{player!r}, '--ao=null', *sys.argv[1:]])\n"
    )
    wrapper.chmod(0o755)
    # A separate server process keeps the PTY parent single-threaded at fork.
    server = subprocess.Popen(
        [sys.executable, "-u", "-c",
         "import functools,http.server,sys\n"
         "class Handler(http.server.SimpleHTTPRequestHandler):\n"
         "    def log_message(self, *_): pass\n"
         "server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), "
         "functools.partial(Handler, directory=sys.argv[1]))\n"
         "print(server.server_port, flush=True)\nserver.serve_forever()\n",
         str(directory)], stdout=subprocess.PIPE, start_new_session=True,
    )
    try:
        if not select.select([server.stdout], [], [], 5)[0]:
            raise SmokeFailure("local audio server did not become ready within 5s")
        port = server.stdout.readline().strip()
        if not port.isdigit():
            raise SmokeFailure("local audio server exited without a port")
        saved = directory / "state" / "rmusic"
        saved.mkdir(parents=True)
        (saved / "state.json").write_text(json.dumps({
            "version": 1, "volume": 50, "favorites": [],
            "last_station": {"stationuuid": "", "name": "Local smoke station",
                             "url": f"http://127.0.0.1:{int(port)}/silence.wav"},
        }))
        yield {"PATH": str(wrapper_directory) + os.pathsep + os.environ["PATH"]}, pid_file
    finally:
        if server.poll() is None:
            server.terminate()
            try:
                server.wait(timeout=2)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait(timeout=2)
        server.stdout.close()


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

        print("RUN: local playback and quit with an owned real player", flush=True)
        playback_home = state + "/playback"
        with local_audio(playback_home) as (environment, pid_file):
            with Session(binary, playback_home, options.timeout, environment) as app:
                app.send(b" ")
                app.expect(lambda data: b"Playing" in data, "local audio playback")
                player_pid = int(pid_file.read_text())
                app.exit(key=b"q")
                # Check before Session cleanup could kill and mask a leaked player.
                try:
                    os.kill(player_pid, 0)
                except ProcessLookupError:
                    pass
                else:
                    raise SmokeFailure(f"quit left owned player {player_pid} alive")
        print("PASS: quit stopped and reaped the owned real mpv player", flush=True)


if __name__ == "__main__":
    try:
        main()
    except SmokeFailure as error:
        print(f"FAIL: {error}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        print("FAIL: smoke check interrupted", file=sys.stderr)
        sys.exit(130)
