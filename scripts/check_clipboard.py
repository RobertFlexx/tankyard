#!/usr/bin/env python3
"""Check real cross-process UTF-8 clipboard exchange and X11 format discovery under Xvfb."""
import ctypes as c
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def advertised_formats():
    x = c.CDLL("libX11.so.6")
    ptr, word = c.c_void_p, c.c_ulong
    x.XOpenDisplay.argtypes, x.XOpenDisplay.restype = [c.c_char_p], ptr
    x.XDefaultRootWindow.argtypes, x.XDefaultRootWindow.restype = [ptr], word
    x.XCreateSimpleWindow.argtypes = [ptr, word, c.c_int, c.c_int, c.c_uint, c.c_uint, c.c_uint, word, word]
    x.XCreateSimpleWindow.restype = word
    x.XInternAtom.argtypes, x.XInternAtom.restype = [ptr, c.c_char_p, c.c_int], word
    x.XConvertSelection.argtypes = [ptr, word, word, word, word, word]
    x.XFlush.argtypes = [ptr]
    x.XPending.argtypes, x.XPending.restype = [ptr], c.c_int
    x.XNextEvent.argtypes = [ptr, ptr]
    x.XGetWindowProperty.argtypes = [ptr, word, word, c.c_long, c.c_long, c.c_int, word,
                                   c.POINTER(word), c.POINTER(c.c_int), c.POINTER(word), c.POINTER(word), c.POINTER(ptr)]
    x.XGetAtomName.argtypes, x.XGetAtomName.restype = [ptr, word], ptr
    x.XFree.argtypes = [ptr]
    x.XDestroyWindow.argtypes = [ptr, word]
    x.XCloseDisplay.argtypes = [ptr]
    display = x.XOpenDisplay(None)
    assert display, "Run this check under Xvfb or an X11 display"
    window = x.XCreateSimpleWindow(display, x.XDefaultRootWindow(display), 0, 0, 1, 1, 0, 0, 0)
    try:
        atom = lambda name: x.XInternAtom(display, name.encode(), 0)
        selection, target, prop = atom("CLIPBOARD"), atom("TARGETS"), atom("TANKYARD_CLIPBOARD_TEST")
        x.XConvertSelection(display, selection, target, prop, window, 0)
        x.XFlush(display)
        event = c.create_string_buffer(192)
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            if x.XPending(display):
                x.XNextEvent(display, event)
                if c.cast(event, c.POINTER(c.c_int)).contents.value == 31:
                    break
            else:
                time.sleep(0.005)
        else:
            raise AssertionError("Clipboard TARGETS request timed out")
        actual, fmt, length, left, data = word(), c.c_int(), word(), word(), ptr()
        result = x.XGetWindowProperty(display, window, prop, 0, 4096, 0, 0,
                                     c.byref(actual), c.byref(fmt), c.byref(length), c.byref(left), c.byref(data))
        assert result == 0 and fmt.value == 32 and length.value, "Clipboard did not advertise its formats"
        try:
            atoms = c.cast(data, c.POINTER(word))
            names = set()
            for i in range(length.value):
                name = x.XGetAtomName(display, atoms[i])
                if name:
                    names.add(c.string_at(name).decode())
                    x.XFree(name)
            return names
        finally:
            if data:
                x.XFree(data)
    finally:
        x.XDestroyWindow(display, window)
        x.XCloseDisplay(display)


def check(binary):
    environment = dict(os.environ)
    # Xvfb must not accidentally use the operator's real Wayland clipboard.
    environment.pop("WAYLAND_DISPLAY", None)
    environment.pop("WAYLAND_SOCKET", None)
    with tempfile.TemporaryDirectory(prefix="tankyard-clipboard-") as directory:
        root = Path(directory)
        processes = []
        flags = []
        try:
            for index, text in enumerate(["tankyard://party/127.0.0.1:5000#" + "ab" * 32,
                                          "Tanks you — 世界\nUnicode paste: café ☃"]):
                source, ready, output = (root / (str(index) + suffix) for suffix in (".txt", ".ready", ".out"))
                source.write_text(text, encoding="utf-8")
                processes.append(subprocess.Popen([binary, "copy", str(source), str(ready)], env=environment))
                flags.append(ready)
                deadline = time.monotonic() + 5
                while not ready.exists():
                    assert processes[-1].poll() is None and time.monotonic() < deadline, "Copy failed"
                    time.sleep(0.01)
                formats = advertised_formats()
                assert "UTF8_STRING" in formats and any(name.startswith("text/plain") for name in formats)
                subprocess.run([binary, "paste", str(output)], check=True, timeout=5, env=environment)
                assert output.read_text(encoding="utf-8") == text, "External paste changed the text"
        finally:
            for ready in flags:
                ready.unlink(missing_ok=True)
            for process in processes:
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
    print("Native cross-process Unicode clipboard and format discovery passed")


if __name__ == "__main__":
    check(str(Path(sys.argv[1]).resolve()))
