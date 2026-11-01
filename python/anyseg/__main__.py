"""
The anyseg command: segmentation of text from files and the standard input

Example:
    anyseg --lang ru --format conllu text.txt
    python -m anyseg --help
"""

import contextlib
import signal
import sys
import threading

from ._anyseg import _main


def main() -> None:
    """
    Entry point of the anyseg command

    Description:
        The Rust core parses the arguments, reads the files and writes the result. Ctrl+C ends the
        command at once, as it ends a program, and an ignored SIGINT (a command in the background
        of a script) stays ignored; the handler of Python comes back after the command

    Raises:
        SystemExit: Always, with the exit code of the command
    """
    # The core writes to the descriptors: what Python holds in its buffers goes first
    # The descriptors stay open when a stream of Python is closed or replaced
    for stream in (sys.stdout, sys.stderr):
        with contextlib.suppress(AttributeError, ValueError):
            stream.flush()
    handler = None
    main_thread = threading.current_thread() is threading.main_thread()
    if main_thread and signal.getsignal(signal.SIGINT) is signal.default_int_handler:
        handler = signal.signal(signal.SIGINT, signal.SIG_DFL)
    try:
        code = _main(sys.argv[1:])
    finally:
        if handler is not None:
            signal.signal(signal.SIGINT, handler)
    sys.exit(code)


if __name__ == "__main__":
    main()
