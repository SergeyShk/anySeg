"""
The anyseg command: segmentation of text from files and the standard input

Example:
    anyseg --lang ru --format conllu text.txt
    python -m anyseg --help
"""

import signal
import sys

from ._anyseg import _main


def main() -> None:
    """
    Entry point of the anyseg command

    Description:
        The Rust core parses the arguments, reads the files and writes the result. Ctrl+C ends the
        command at once, as it ends a program, and an ignored SIGINT (a command in the background
        of a script) stays ignored

    Raises:
        SystemExit: Always, with the exit code of the command
    """
    if signal.getsignal(signal.SIGINT) is signal.default_int_handler:
        signal.signal(signal.SIGINT, signal.SIG_DFL)
    sys.exit(_main(sys.argv[1:]))


if __name__ == "__main__":
    main()
