//! The `anyseg` command: segmentation of text from files and the standard input

use std::borrow::Cow;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use crate::layout::{self, is_separator, one_line};
use crate::{ConlluWriter, Document, Language, Segmenter};

const HELP: &str = "\
anyseg - segmentation of text into tokens and sentences

Usage: anyseg --lang LANG [options] [file ...]

Files are read one after another; without files and in place of \"-\" the standard input is
read, in UTF-8. A blank line ends a paragraph: the text is segmented paragraph by paragraph,
and the result goes to the standard output or to the file of -o.

Options:
  -l, --lang LANG           language of the text, ISO 639-1 code: LANGUAGES
  -f, --format FORMAT       output format:
                              tokens - a token per line, a blank line after every sentence
                              (default)
                              sentences - a sentence per line, a blank line between paragraphs
                              conllu - CoNLL-U: tokens, the spaces after them, sentences and
                              paragraphs
  -o, --output FILE         write the result to a file
      --abbreviations FILE  more abbreviations, one per line
      --tokens FILE         strings that are one token each, one per line
  -h, --help                show this help and exit
  -V, --version             show the version and exit
";

/// Runs the command with `args` (without the name of the program) and returns the exit code: 0 on success, 1 on
/// an error of reading or writing a file, 2 on wrong arguments. `version` is printed by `--version`
pub fn run(args: Vec<OsString>, version: &str) -> u8 {
    let stdout = io::stdout();
    let interactive = stdout.is_terminal();
    let closed = closed_stdout();
    let mut output = BufWriter::new(stdout.lock());
    execute(
        args,
        version,
        interactive,
        closed,
        &mut io::stdin().lock(),
        &mut output,
        &mut io::stderr(),
    )
}

/// The standard output is closed (`>&-`): writing to it would fail without an error. A Rust program finds it open
/// on /dev/null, as the runtime opens the closed standard descriptors there; the module of Python finds it closed
#[cfg(unix)]
fn closed_stdout() -> bool {
    use std::os::fd::AsFd;
    // EBADF, the same number on Linux, macOS and the BSDs: a closed descriptor, not a lack of descriptors
    io::stdout()
        .as_fd()
        .try_clone_to_owned()
        .is_err_and(|err| err.raw_os_error() == Some(9))
}

#[cfg(not(unix))]
fn closed_stdout() -> bool {
    false
}

/// The buffer of the input: the output is flushed when it is empty and the next read may wait for input
#[cfg(not(test))]
const INPUT_BUFFER: usize = 1 << 16;
#[cfg(test)]
const INPUT_BUFFER: usize = 1 << 9;

/// The longest text without blank lines that is segmented at once: a longer one is written without its last
/// sentence, and a longer line is cut at a space, so that the memory does not grow with the input. The allocator of
/// macOS does not give back blocks of more than 4 MiB, which the tokens of larger pieces reach, at two bytes a token
#[cfg(not(test))]
const PARAGRAPH: usize = 1 << 18;
#[cfg(test)]
const PARAGRAPH: usize = 1 << 12;

/// Output format
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Tokens,
    Sentences,
    Conllu,
}

#[derive(Debug, PartialEq)]
enum Command {
    Help,
    Version,
    Run {
        lang: String,
        format: Format,
        output: Option<PathBuf>,
        abbreviations: Vec<PathBuf>,
        tokens: Vec<PathBuf>,
        files: Vec<OsString>,
    },
}

fn languages() -> String {
    Language::ALL
        .iter()
        .map(|language| language.code())
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse(args: Vec<OsString>) -> Result<Command, String> {
    let (mut lang, mut format, mut output) = (None, Format::Tokens, None);
    let (mut abbreviations, mut tokens, mut files) = (Vec::new(), Vec::new(), Vec::new());
    let mut options = true;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let bytes = arg.as_encoded_bytes();
        if !options || !bytes.starts_with(b"-") || bytes == b"-" {
            files.push(arg);
            continue;
        }
        // An option with an attached value not in UTF-8 would be taken for a file
        let Some(option) = arg.to_str().map(str::to_owned) else {
            return Err(format!(
                "cannot read the option {}: give its value as a separate argument",
                arg.to_string_lossy()
            ));
        };
        let (name, inline) = match option.split_once('=') {
            Some((name, value)) if option.starts_with("--") => (name, Some(value)),
            _ if !option.starts_with("--") && option.len() > 2 => option
                .split_at_checked(2)
                .map_or((option.as_str(), None), |(name, value)| (name, Some(value))),
            _ => (option.as_str(), None),
        };
        let mut value = || -> Result<OsString, String> {
            match inline {
                Some(value) => Ok(value.into()),
                None => args
                    .next()
                    .ok_or_else(|| format!("option {name} needs a value")),
            }
        };
        match name {
            "--" => options = false,
            "-h" | "--help" if inline.is_none() => return Ok(Command::Help),
            "-V" | "--version" if inline.is_none() => return Ok(Command::Version),
            "-l" | "--lang" => {
                lang = Some(value()?.into_string().map_err(|_| {
                    format!("unsupported language, expected one of: {}", languages())
                })?);
            }
            "-f" | "--format" => {
                format = match value()?.to_str() {
                    Some("tokens") => Format::Tokens,
                    Some("sentences") => Format::Sentences,
                    Some("conllu") => Format::Conllu,
                    other => {
                        return Err(format!(
                            "unknown format {}, expected one of: tokens, sentences, conllu",
                            other.unwrap_or("?")
                        ));
                    }
                }
            }
            "-o" | "--output" => {
                output = Some(value()?)
                    .filter(|value| value != "-")
                    .map(PathBuf::from)
            }
            "--abbreviations" => abbreviations.push(PathBuf::from(value()?)),
            "--tokens" => tokens.push(PathBuf::from(value()?)),
            _ => return Err(format!("unknown option {option}")),
        }
    }
    let lang =
        lang.ok_or_else(|| format!("the language is required: --lang, one of: {}", languages()))?;
    Ok(Command::Run {
        lang,
        format,
        output,
        abbreviations,
        tokens,
        files,
    })
}

#[allow(clippy::too_many_arguments)]
fn execute(
    args: Vec<OsString>,
    version: &str,
    interactive: bool,
    closed: bool,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8 {
    // A message to stderr: if it is closed too, there is no one to tell
    let mut report = |message: String| drop(writeln!(stderr, "anyseg: {message}"));
    let (lang, format, output_path, abbreviations, tokens, files) = match parse(args) {
        Ok(Command::Run {
            lang,
            format,
            output,
            abbreviations,
            tokens,
            files,
        }) => (lang, format, output, abbreviations, tokens, files),
        Ok(Command::Help | Command::Version) if closed => {
            report("the standard output is closed".to_owned());
            return 1;
        }
        Ok(Command::Help) => {
            let help = HELP.replace("LANGUAGES", &languages());
            let written = stdout
                .write_all(help.as_bytes())
                .and_then(|()| stdout.flush());
            return written.map_or_else(|err| write_error(&err, &mut report), |()| 0);
        }
        Ok(Command::Version) => {
            let written = writeln!(stdout, "anyseg {version}").and_then(|()| stdout.flush());
            return written.map_or_else(|err| write_error(&err, &mut report), |()| 0);
        }
        Err(message) => {
            report(format!("{message}\nHelp: anyseg --help"));
            return 2;
        }
    };
    let Ok(segmenter) = Segmenter::new(&lang) else {
        report(format!(
            "unsupported language {lang}, expected one of: {}\nHelp: anyseg --help",
            languages()
        ));
        return 2;
    };
    let segmenter = match with_lists(segmenter, &abbreviations, &tokens) {
        Ok(segmenter) => segmenter,
        Err(message) => {
            report(message);
            return 1;
        }
    };
    let files = if files.is_empty() {
        vec![OsString::from("-")]
    } else {
        files
    };
    if closed && output_path.is_none() {
        report("the standard output is closed".to_owned());
        return 1;
    }
    let mut file_output;
    let output: &mut dyn Write = match &output_path {
        Some(path) => {
            // The output file is created anew: an input or a list it would overwrite is not read. A device
            // (/dev/null, /dev/stdout in a terminal) is not overwritten by creating it
            let mut sources = files
                .iter()
                .map(|file| (file != "-").then(|| Path::new(file)))
                .chain(
                    abbreviations
                        .iter()
                        .chain(&tokens)
                        .map(|path| Some(path.as_path())),
                );
            let regular = std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file());
            if regular && sources.any(|source| same_file(path, source)) {
                report(format!(
                    "{}: the output file is an input file or a list",
                    path.display()
                ));
                return 1;
            }
            match File::create(path) {
                Ok(file) => {
                    file_output = BufWriter::new(file);
                    &mut file_output
                }
                Err(err) => {
                    report(format!("{}: {}", path.display(), open_error(path, &err)));
                    return 1;
                }
            }
        }
        None => stdout,
    };
    let mut writer = Writer {
        segmenter: &segmenter,
        format,
        output,
        interactive: interactive && output_path.is_none(),
        pending: false,
        conllu: ConlluWriter::new(),
        paragraphs: 0,
        text: String::new(),
        ready: 0,
        continued: false,
        carry: Vec::new(),
        out: String::new(),
    };
    let mut code = 0;
    for file in files {
        let path = Path::new(&file);
        let name = path.display().to_string();
        let result = if file == "-" {
            writer.process(stdin)
        } else {
            match File::open(path) {
                Ok(mut opened) => writer.process(&mut opened),
                Err(err) => Err(Failure::Open(open_error(path, &err))),
            }
        };
        match result {
            Ok(None) => {}
            Ok(Some(line)) => {
                report(format!(
                    "{name}: line {line}: not UTF-8, the wrong bytes are replaced with U+FFFD"
                ));
                code = 1;
            }
            Err(Failure::Open(message)) => {
                report(format!("{name}: {message}"));
                code = 1;
            }
            Err(Failure::Read(err)) => {
                report(format!("{name}: {}", describe(&err)));
                code = 1;
            }
            Err(Failure::Encoding(line)) => {
                report(format!("{name}: the file is not in UTF-8 (line {line})"));
                code = 1;
            }
            Err(Failure::Write(err)) => return write_error(&err, &mut report).max(code),
        }
    }
    match writer.output.flush() {
        Ok(()) => code,
        Err(err) => write_error(&err, &mut report).max(code),
    }
}

/// The exit code after an error of writing: a closed pipe (`anyseg ... | head`) is not an error
fn write_error(err: &io::Error, report: &mut impl FnMut(String)) -> u8 {
    if err.kind() == io::ErrorKind::BrokenPipe {
        return 0;
    }
    report(format!("write error: {err}"));
    1
}

/// The segmenter with the abbreviations and the tokens of the files: one per line, blank lines and lines starting
/// with «#» skipped
fn with_lists(
    mut segmenter: Segmenter,
    abbreviations: &[PathBuf],
    tokens: &[PathBuf],
) -> Result<Segmenter, String> {
    for (path, is_abbreviation) in abbreviations
        .iter()
        .map(|path| (path, true))
        .chain(tokens.iter().map(|path| (path, false)))
    {
        let name = path.display();
        let bytes = std::fs::read(path).map_err(|err| format!("{name}: {}", describe(&err)))?;
        let text = String::from_utf8(bytes).map_err(|err| {
            let valid = &err.as_bytes()[..err.utf8_error().valid_up_to()];
            let line = valid.iter().filter(|&&byte| byte == b'\n').count() + 1;
            format!("{name}: line {line}: not UTF-8")
        })?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        for (number, line) in text.lines().enumerate() {
            let entry = line.trim();
            if entry.is_empty() || entry.starts_with('#') {
                continue;
            }
            let added = if is_abbreviation {
                segmenter.with_abbreviations([entry])
            } else {
                segmenter.with_tokens([entry])
            };
            segmenter = added.map_err(|err| format!("{name}: line {}: {err}", number + 1))?;
        }
    }
    Ok(segmenter)
}

/// An error of input and output in plain words for the frequent cases
fn describe(err: &io::Error) -> String {
    match err.kind() {
        io::ErrorKind::NotFound => "no such file".into(),
        io::ErrorKind::PermissionDenied => "permission denied".into(),
        io::ErrorKind::IsADirectory => "is a directory".into(),
        _ => err.to_string(),
    }
}

/// The error of opening a file: on Windows a directory fails to open with «permission denied»
fn open_error(path: &Path, err: &io::Error) -> String {
    if path.is_dir() {
        "is a directory".into()
    } else {
        describe(err)
    }
}

/// The output file is the input `source` (`None` - the standard input). On Unix the device and the number of the
/// file decide, so that a hard link and a symbolic link are the same file
#[cfg(unix)]
fn same_file(output: &Path, source: Option<&Path>) -> bool {
    use std::os::fd::AsFd;
    use std::os::unix::fs::MetadataExt;
    let id = |metadata: std::fs::Metadata| (metadata.dev(), metadata.ino());
    let metadata = match source {
        Some(path) => opened_metadata(path),
        // By the descriptor: /dev/stdin on macOS is a file of devfs, not the redirected file
        None => io::stdin()
            .as_fd()
            .try_clone_to_owned()
            .and_then(|descriptor| File::from(descriptor).metadata()),
    };
    match (opened_metadata(output), metadata) {
        (Ok(output), Ok(source)) => id(output) == id(source),
        _ => false,
    }
}

/// The metadata of the file a path opens: on macOS /dev/stdin and /dev/fd/N are files of devfs with a device of
/// their own, which stand for the files open in the process. A pipe or a socket is not opened, which could wait
#[cfg(unix)]
fn opened_metadata(path: &Path) -> io::Result<std::fs::Metadata> {
    use std::os::unix::fs::FileTypeExt;
    let metadata = std::fs::metadata(path)?;
    let kind = metadata.file_type();
    if kind.is_fifo() || kind.is_socket() {
        return Ok(metadata);
    }
    File::open(path)
        .and_then(|file| file.metadata())
        .or(Ok(metadata))
}

/// The output file is the input `source` (`None` - the standard input). On Windows a file is its canonical path,
/// and the standard input, which has none, is compared by its times and size: the index of a file needs an
/// unstable API
#[cfg(windows)]
fn same_file(output: &Path, source: Option<&Path>) -> bool {
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::AsHandle;
    let Some(source) = source else {
        let stamp = |metadata: std::fs::Metadata| {
            (
                metadata.creation_time(),
                metadata.last_write_time(),
                metadata.file_size(),
            )
        };
        let stdin = io::stdin()
            .as_handle()
            .try_clone_to_owned()
            .and_then(|handle| File::from(handle).metadata());
        return match (std::fs::metadata(output), stdin) {
            (Ok(output), Ok(stdin)) => stamp(output) == stamp(stdin),
            _ => false,
        };
    };
    match (std::fs::canonicalize(output), std::fs::canonicalize(source)) {
        (Ok(output), Ok(source)) => output == source,
        _ => false,
    }
}

/// The output file is the input `source` elsewhere: by the canonical path; the standard input is not known
#[cfg(not(any(unix, windows)))]
fn same_file(output: &Path, source: Option<&Path>) -> bool {
    match (
        std::fs::canonicalize(output),
        source.map(std::fs::canonicalize),
    ) {
        (Ok(output), Some(Ok(source))) => output == source,
        _ => false,
    }
}

enum Failure {
    /// The file does not open: the message
    Open(String),
    Read(io::Error),
    /// The file is in another encoding: UTF-16, or wrong bytes before the first line of UTF-8; the line where it
    /// shows
    Encoding(usize),
    Write(io::Error),
}

struct Writer<'a> {
    segmenter: &'a Segmenter,
    format: Format,
    output: &'a mut dyn Write,
    /// Flush after every paragraph: in a terminal the result shows at once
    interactive: bool,
    /// There is output after the last flush
    pending: bool,
    conllu: ConlluWriter,
    /// The number of paragraphs written: a blank line goes before the next one in the format `sentences`
    paragraphs: usize,
    /// The text read and not yet written, and its length up to the last blank line: the paragraphs before it are
    /// complete
    text: String,
    ready: usize,
    /// The text goes on with a paragraph cut by its length: its first paragraph is not a new one
    continued: bool,
    /// The end of a line longer than PARAGRAPH, which the next read gives
    carry: Vec<u8>,
    /// The output of a sentence, kept between the pieces: a block of its own for each would not be given back
    out: String,
}

impl Writer<'_> {
    /// Reads the input by lines and writes every paragraph as soon as a blank line or the end of the input
    /// closes it; returns the number of the first line that is not UTF-8, whose wrong bytes become U+FFFD
    fn process(&mut self, reader: &mut dyn Read) -> Result<Option<usize>, Failure> {
        let mut reader = BufReader::with_capacity(INPUT_BUFFER, reader);
        self.flush_pending().map_err(Failure::Write)?;
        // The encoding shows at the start of the input: ASCII at the start of a file in another encoding is not
        // written
        let head = loop {
            match reader.fill_buf() {
                Ok(head) => break head,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) => return Err(Failure::Read(err)),
            }
        };
        if let Some(line) = foreign_encoding(head) {
            return Err(Failure::Encoding(line));
        }
        // The end of the input in a terminal (Ctrl+D) is not read twice: a second read would wait for input
        if head.is_empty() {
            return Ok(None);
        }
        self.conllu.start_document();
        self.text.clear();
        self.carry.clear();
        (self.ready, self.continued) = (0, false);
        let mut bytes = Vec::new();
        let mut end = false;
        let mut number = 0;
        let mut invalid = None;
        // A line longer than PARAGRAPH is read in pieces: the piece starts a line, and the line so far is blank
        let mut starts = true;
        let mut line_blank = false;
        // A line of UTF-8 with non-ASCII bytes was read: before it, wrong bytes and NUL mean another encoding of
        // the file (cp1251, UTF-16 without BOM), after it a damaged line
        let mut utf8 = false;
        loop {
            bytes.clear();
            let Some(ended) = self.read_line(&mut reader, &mut bytes, &mut end)? else {
                break;
            };
            let first = starts;
            starts = ended;
            number += usize::from(first);
            if !utf8 && bytes.contains(&0) {
                return Err(Failure::Encoding(number));
            }
            let decoded = String::from_utf8_lossy(&bytes);
            match decoded {
                Cow::Owned(_) if !utf8 => return Err(Failure::Encoding(number)),
                Cow::Owned(_) => {
                    invalid.get_or_insert(number);
                }
                Cow::Borrowed(_) => utf8 |= !bytes.is_ascii(),
            }
            let mut line: &str = &decoded;
            if number == 1 && first {
                line = line.strip_prefix('\u{feff}').unwrap_or(line);
            }
            line_blank = (first || line_blank) && blank(line);
            if !line_blank {
                self.text.push_str(line);
                if self.text.len() >= PARAGRAPH || !ended {
                    self.write_long().map_err(Failure::Write)?;
                }
            } else if ended {
                // A blank line ends the paragraphs before it, which are segmented as in the whole text: in pieces of
                // about INPUT_BUFFER, or as soon as the next read may wait. It also ends a paragraph written whole
                if blank(&self.text) {
                    self.continued = false;
                }
                if !self.text.is_empty() {
                    self.text.push_str(line);
                    self.ready = self.text.len();
                }
                if self.ready >= INPUT_BUFFER {
                    self.write_ready().map_err(Failure::Write)?;
                }
            }
        }
        self.ready = self.text.len();
        self.write_ready().map_err(Failure::Write)?;
        Ok(invalid)
    }

    /// A line of the input with its line break (LF, CR or CRLF), or a piece of a line longer than PARAGRAPH, which
    /// ends after a space or else at a character; `true` - the line ended, `None` - the input did. Before a read
    /// that may wait for input (an empty buffer), the output is flushed, so that the command can be kept open and
    /// fed text. After the end of the input (`end`) nothing is read: in a terminal a line without a line break ends
    /// with one Ctrl+D
    fn read_line(
        &mut self,
        reader: &mut BufReader<&mut dyn Read>,
        bytes: &mut Vec<u8>,
        end: &mut bool,
    ) -> Result<Option<bool>, Failure> {
        bytes.append(&mut self.carry);
        // A CR ended the buffer: the line feed of CRLF may begin the next one
        let mut after_cr = false;
        while !*end {
            if bytes.len() >= PARAGRAPH && !after_cr {
                let cut = bytes
                    .iter()
                    .rposition(|&byte| byte == b' ' || byte == b'\t')
                    .map(|at| at + 1)
                    .or_else(|| (1..bytes.len()).rev().find(|&at| bytes[at] & 0xC0 != 0x80))
                    .unwrap_or(bytes.len());
                self.carry = bytes.split_off(cut);
                return Ok(Some(false));
            }
            // The next read may wait for input: the complete paragraphs go out first
            if reader.buffer().is_empty() {
                self.write_ready().map_err(Failure::Write)?;
                self.flush_pending().map_err(Failure::Write)?;
            }
            let available = match reader.fill_buf() {
                Ok(available) => available,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(Failure::Read(err)),
            };
            if available.is_empty() {
                *end = true;
                break;
            }
            if after_cr {
                if available[0] == b'\n' {
                    bytes.push(b'\n');
                    reader.consume(1);
                }
                break;
            }
            let (length, done) = match available
                .iter()
                .position(|&byte| byte == b'\n' || byte == b'\r')
            {
                Some(at) if available[at] == b'\r' => match available.get(at + 1) {
                    Some(b'\n') => (at + 2, true),
                    Some(_) => (at + 1, true),
                    None => {
                        after_cr = true;
                        (at + 1, false)
                    }
                },
                Some(at) => (at + 1, true),
                None => (available.len(), false),
            };
            bytes.extend_from_slice(&available[..length]);
            reader.consume(length);
            if done {
                break;
            }
        }
        Ok((!bytes.is_empty()).then_some(true))
    }

    /// The text up to its last blank line, whose paragraphs are complete
    fn write_ready(&mut self) -> io::Result<()> {
        if self.ready == 0 {
            return Ok(());
        }
        let rest = self.text.split_off(self.ready);
        let ready = std::mem::replace(&mut self.text, rest);
        self.ready = 0;
        let doc = self.segmenter.segment_string(ready);
        self.write_sentences(&doc, doc.sentence_count())?;
        self.continued = false;
        Ok(())
    }

    /// A text without a blank line that reached PARAGRAPH, without its last sentence, which the next line may go on
    /// with: it stays at the start of the text, and the next piece continues its paragraph. One sentence longer than
    /// PARAGRAPH is written whole
    fn write_long(&mut self) -> io::Result<()> {
        let doc = self
            .segmenter
            .segment_string(std::mem::take(&mut self.text));
        let count = doc.sentence_count();
        let (written, keep) = match count {
            0..=1 => (count, doc.text().len()),
            _ => (count - 1, doc.sentence(count - 1).span.start),
        };
        self.write_sentences(&doc, written)?;
        // The last sentence may itself start a paragraph (after U+2029), and the separators dropped may end one
        self.continued = match count {
            0 => self.continued && !closes_paragraph(doc.text()),
            1 => !closes_paragraph(&doc.text()[doc.sentence(0).span.end..]),
            _ => doc.paragraph(doc.paragraph_count() - 1).sentences.start < count - 1,
        };
        self.text = doc.text()[keep..].to_owned();
        self.ready = 0;
        Ok(())
    }

    /// The first `count` sentences of a document in the format of the output, written sentence by sentence
    fn write_sentences(&mut self, doc: &Document, count: usize) -> io::Result<()> {
        self.out.clear();
        for paragraph in 0..doc.paragraph_count() {
            let sentences = doc.paragraph(paragraph).sentences;
            if sentences.start >= count {
                break;
            }
            if std::mem::take(&mut self.continued) {
                self.conllu.continue_paragraph();
            } else {
                if self.format == Format::Sentences && self.paragraphs > 0 {
                    self.out.push('\n');
                }
                self.paragraphs += 1;
            }
            for sentence in sentences.start..sentences.end.min(count) {
                match self.format {
                    Format::Tokens => {
                        for token in doc.sentence(sentence).tokens {
                            self.out.push_str(doc.tokens()[token].slice(doc.text()));
                            self.out.push('\n');
                        }
                        self.out.push('\n');
                    }
                    Format::Sentences => {
                        let text = doc.sentence(sentence).span.slice(doc.text());
                        let _ = writeln!(self.out, "{}", one_line(text));
                    }
                    Format::Conllu => self
                        .conllu
                        .write(doc, sentence..sentence + 1, &mut self.out),
                }
                self.output.write_all(self.out.as_bytes())?;
                self.out.clear();
            }
        }
        self.pending = true;
        if self.interactive {
            self.flush_pending()?;
        }
        Ok(())
    }

    fn flush_pending(&mut self) -> io::Result<()> {
        if self.pending {
            self.output.flush()?;
            self.pending = false;
        }
        Ok(())
    }
}

/// A text of nothing but separators
fn blank(text: &str) -> bool {
    text.chars().all(is_separator)
}

/// The separators after a sentence hold the end of its paragraph: U+2029 or a blank line
fn closes_paragraph(separators: &str) -> bool {
    layout::paragraphs(&format!("x{separators}x")).len() > 1
}

/// The line at which the start of the input shows another encoding than UTF-8: a byte order mark of UTF-16, NUL
/// or wrong bytes on a line before the first line of UTF-8 with non-ASCII characters
fn foreign_encoding(head: &[u8]) -> Option<usize> {
    if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
        return Some(1);
    }
    for (index, line) in lines(head).enumerate() {
        if line.contains(&0) {
            return Some(index + 1);
        }
        if line.is_ascii() {
            continue;
        }
        return match std::str::from_utf8(line) {
            // The line is cut by the end of the buffer in the middle of a character: the reading of lines decides
            Err(err) if err.error_len().is_some() => Some(index + 1),
            _ => None,
        };
    }
    None
}

/// The lines of bytes as the command reads them: each ends after LF, CR or CRLF
fn lines(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = bytes;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let end = match rest.iter().position(|&byte| byte == b'\n' || byte == b'\r') {
            Some(at) if rest[at] == b'\r' && rest.get(at + 1) == Some(&b'\n') => at + 2,
            Some(at) => at + 1,
            None => rest.len(),
        };
        let (line, after) = rest.split_at(end);
        rest = after;
        Some(line)
    })
}

#[cfg(all(test, feature = "ru"))]
mod tests {
    use super::*;

    /// The exit code, the output and the messages of the command on an input
    fn command(args: &[&str], input: &[u8]) -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let code = execute(
            args.iter().map(OsString::from).collect(),
            "1.2.3",
            false,
            false,
            &mut &input[..],
            &mut stdout,
            &mut stderr,
        );
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    const TEXT: &str = "Ёж идёт. Кот спит в 1914 г. Затем\nушёл\n\n\nУтро настало";

    #[test]
    fn tokens() {
        let (code, out, err) = command(&["-l", "ru"], TEXT.as_bytes());
        assert_eq!((code, err.as_str()), (0, ""));
        assert_eq!(
            out,
            "Ёж\nидёт\n.\n\nКот\nспит\nв\n1914\nг\n.\n\nЗатем\nушёл\n\nУтро\nнастало\n\n"
        );
    }

    #[test]
    fn sentences() {
        let (code, out, _) = command(&["--lang=ru", "-fsentences"], TEXT.as_bytes());
        assert_eq!(code, 0);
        assert_eq!(
            out,
            "Ёж идёт.\nКот спит в 1914 г.\nЗатем ушёл\n\nУтро настало\n"
        );
    }

    #[test]
    fn conllu_numbers_sentences_through_paragraphs_and_files() {
        let path = std::env::temp_dir().join(format!("anyseg-cli-{}.txt", std::process::id()));
        std::fs::write(&path, "Кот спит").unwrap();
        let file = path.to_str().unwrap();
        let (code, out, _) = command(&["-l", "ru", "-f", "conllu", "-", file], TEXT.as_bytes());
        std::fs::remove_file(&path).unwrap();
        assert_eq!(code, 0);
        let whole = Segmenter::new("ru").unwrap().segment(TEXT).to_conllu();
        let second = "# newdoc\n# newpar\n# sent_id = 5\n# text = Кот спит\n\
            1\tКот\t_\t_\t_\t_\t_\t_\t_\t_\n2\tспит\t_\t_\t_\t_\t_\t_\t_\t_\n\n";
        assert_eq!(out, format!("{whole}{second}"));
    }

    #[test]
    fn paragraphs_as_in_the_whole_text() {
        // Line breaks of every kind, blank lines of separators, a byte order mark and no final line break
        let text = "\u{feff}Раз. Два\r\nтри\r\n \r\nЧетыре\u{2028}пять.\n\u{200b}\nШесть\u{2029}Семь\n\n\n";
        let (_, out, _) = command(&["-l", "ru"], text.as_bytes());
        let doc = Segmenter::new("ru").unwrap().segment(text);
        let mut expected = String::new();
        for sentence in 0..doc.sentence_count() {
            for token in doc.sentence(sentence).tokens {
                expected.push_str(doc.tokens()[token].slice(doc.text()));
                expected.push('\n');
            }
            expected.push('\n');
        }
        assert_eq!(out, expected);
    }

    #[test]
    fn pieces_of_many_paragraphs() {
        // Paragraphs are segmented in pieces of about the input buffer, with the same result
        let text = (0..20_000)
            .map(|number| format!("Абзац {number}. Ещё\n\n"))
            .collect::<String>();
        assert!(text.len() > 4 * INPUT_BUFFER);
        let (_, out, _) = command(&["-l", "ru", "-f", "conllu"], text.as_bytes());
        assert_eq!(
            out,
            Segmenter::new("ru").unwrap().segment(&text).to_conllu()
        );
    }

    #[cfg(unix)]
    #[test]
    fn option_not_in_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let args = vec![
            OsString::from("-l"),
            OsString::from("ru"),
            OsString::from_vec(b"--output=\xff.out".to_vec()),
        ];
        let code = execute(
            args,
            "",
            false,
            false,
            &mut &b""[..],
            &mut stdout,
            &mut stderr,
        );
        let message = String::from_utf8_lossy(&stderr);
        assert_eq!(code, 2);
        assert!(
            message.contains("cannot read the option --output="),
            "{message}"
        );
    }

    #[test]
    fn long_text_without_blank_lines() {
        // A sentence per line, with LF and with CR only, across lines and on one line: written in pieces, the same
        // sentences as the whole text, in one paragraph
        let number = PARAGRAPH / 2;
        for (text, sentence) in [
            ("Мама мыла раму.\n".repeat(number), "Мама мыла раму."),
            ("Мама мыла раму.\r".repeat(number), "Мама мыла раму."),
            ("Мама мыла\nраму.\n".repeat(number), "Мама мыла раму."),
            ("Мама мыла раму. ".repeat(number), "Мама мыла раму."),
            ("Ма\u{a0}ма. ".repeat(number), "Ма\u{a0}ма."),
        ] {
            assert!(text.len() > 3 * PARAGRAPH);
            let (code, out, _) = command(&["-l", "ru", "-f", "sentences"], text.as_bytes());
            assert_eq!(code, 0);
            assert_eq!(out, format!("{sentence}\n").repeat(number), "{text:?}");
            let (_, conllu, _) = command(&["-l", "ru", "-f", "conllu"], text.as_bytes());
            assert_eq!(conllu.matches("# newpar").count(), 1);
            assert_eq!(conllu.matches("# newdoc").count(), 1);
            assert_eq!(
                conllu,
                Segmenter::new("ru").unwrap().segment(&text).to_conllu()
            );
        }
    }

    #[test]
    fn long_paragraphs() {
        let args = ["-l", "ru", "-f", "conllu"];
        // A paragraph written whole, of one sentence without final marks, ends at a blank line, and the last sentence
        // of a piece may itself start a paragraph
        let speech = "мама мыла раму ".repeat(PARAGRAPH / 10) + "\n\nРаму.\n";
        let (_, conllu, _) = command(&args, speech.as_bytes());
        assert_eq!(conllu.matches("# newpar").count(), 2);
        assert!(conllu.ends_with("# text = Раму.\n1\tРаму\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=No\n2\t.\t_\t_\t_\t_\t_\t_\t_\t_\n\n"), "{conllu}");
        let (_, sentences, _) = command(&["-l", "ru", "-f", "sentences"], speech.as_bytes());
        assert!(sentences.ends_with(" раму\n\nРаму.\n"), "{sentences}");
        // A sentence longer than PARAGRAPH on one line is cut, without a new paragraph
        let line = "мама мыла раму ".repeat(PARAGRAPH / 5);
        let (_, conllu, _) = command(&args, line.as_bytes());
        assert!(conllu.matches("# sent_id").count() > 1, "{conllu}");
        assert_eq!(conllu.matches("# newpar").count(), 1);
        // A blank line ends a paragraph whatever its length, and so does what follows a sentence written whole
        for text in [
            format!("Кот спит.\n{}\nЁж идёт.", " ".repeat(3 * PARAGRAPH)),
            format!("Кот спит.\n{}\nЁж идёт.", "\u{200b}".repeat(PARAGRAPH)),
            format!(
                "{}кот.\u{2029}\nЁж идёт.",
                "мама мыла раму ".repeat(PARAGRAPH / 15)
            ),
            format!(
                "{}кот.\u{c}\u{c}\nЁж идёт.",
                "мама мыла раму ".repeat(PARAGRAPH / 15)
            ),
        ] {
            let (_, conllu, _) = command(&args, text.as_bytes());
            assert_eq!(conllu.matches("# newpar").count(), 2, "{text:?}");
            let (_, out, _) = command(&["-l", "ru", "-f", "sentences"], text.as_bytes());
            assert!(out.ends_with("\n\nЁж идёт.\n"), "{out}");
        }
        let separated = "Мама мыла раму.\u{2029}".repeat(PARAGRAPH / 10);
        let (_, conllu, _) = command(&args, separated.as_bytes());
        assert_eq!(conllu.matches("# newpar").count(), PARAGRAPH / 10);
        // A paragraph that ends a piece does not continue into the next file
        let path = std::env::temp_dir().join(format!("anyseg-long-{}.txt", std::process::id()));
        std::fs::write(&path, "Кот спит").unwrap();
        let file = path.to_str().unwrap();
        let (_, conllu, _) = command(&["-l", "ru", "-f", "conllu", "-", file], speech.as_bytes());
        std::fs::remove_file(&path).unwrap();
        assert_eq!(conllu.matches("# newpar").count(), 3);
        assert_eq!(conllu.matches("# newdoc").count(), 2);
    }

    #[test]
    fn long_line_without_spaces() {
        // Cut at a character: the memory stays bounded, the bytes are all written
        let word = "ё".repeat(PARAGRAPH);
        let (code, out, err) = command(&["-l", "ru"], word.as_bytes());
        assert_eq!((code, err.as_str()), (0, ""));
        assert_eq!(out.replace('\n', ""), word);
    }

    #[test]
    fn line_numbers_of_long_lines() {
        // A piece of a long line is not a line: the damaged line is the second one
        let mut text = "Ёж\n".as_bytes().to_vec();
        text.extend("ма ".repeat(PARAGRAPH).as_bytes());
        text.extend(b"\xff\n");
        let (code, _, err) = command(&["-l", "ru"], &text);
        assert_eq!(code, 1);
        assert!(err.contains("line 2: not UTF-8"), "{err}");
        // A piece of a long line of spaces does not end a paragraph
        let spaces = format!("Ёж{}кот", " ".repeat(3 * PARAGRAPH));
        let (_, conllu, _) = command(&["-l", "ru", "-f", "conllu"], spaces.as_bytes());
        assert_eq!(conllu.matches("# newpar").count(), 1);
    }

    #[test]
    fn end_of_input_read_once() {
        // In a terminal every read after the end would wait for another Ctrl+D
        struct Terminal(Vec<&'static [u8]>);
        impl Read for Terminal {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                let piece = self.0.pop().expect("a read after the end of the input");
                buffer[..piece.len()].copy_from_slice(piece);
                Ok(piece.len())
            }
        }
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut terminal = Terminal(vec![b"", "Кот спит".as_bytes()]);
        let args = vec![OsString::from("-l"), OsString::from("ru")];
        let code = execute(
            args,
            "",
            false,
            false,
            &mut terminal,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!((code, stdout.as_slice()), (0, "Кот\nспит\n\n".as_bytes()));
    }

    #[test]
    fn closed_standard_output() {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let args = vec![OsString::from("-l"), OsString::from("ru")];
        let code = execute(
            args,
            "",
            false,
            true,
            &mut &b"Text"[..],
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 1);
        assert!(String::from_utf8_lossy(&stderr).contains("the standard output is closed"));
    }

    #[test]
    fn lists() {
        let dir = std::env::temp_dir();
        let abbreviations = dir.join(format!("anyseg-abbr-{}.txt", std::process::id()));
        let tokens = dir.join(format!("anyseg-tokens-{}.txt", std::process::id()));
        std::fs::write(&abbreviations, "# abbreviations\n\nсущ.\n").unwrap();
        std::fs::write(&tokens, " C++ \n").unwrap();
        let args = [
            "-l",
            "ru",
            "--abbreviations",
            abbreviations.to_str().unwrap(),
            "--tokens",
            tokens.to_str().unwrap(),
        ];
        let (code, out, _) = command(&args, "Язык C++ - сущ. Это".as_bytes());
        assert_eq!((code, out.as_str()), (0, "Язык\nC++\n-\nсущ.\nЭто\n\n"));
        std::fs::write(&tokens, "ok\nтри слова\n").unwrap();
        let (code, out, err) = command(&args, b"text");
        assert_eq!((code, out.as_str()), (1, ""));
        assert!(
            err.contains("line 2: token \"три слова\" is empty"),
            "{err}"
        );
        std::fs::remove_file(&abbreviations).unwrap();
        std::fs::remove_file(&tokens).unwrap();
    }

    #[test]
    fn arguments() {
        let (code, out, _) = command(&["--help"], b"");
        assert!(code == 0 && out.starts_with("anyseg - ") && out.contains(": ru\n"));
        assert_eq!(
            command(&["-V"], b""),
            (0, "anyseg 1.2.3\n".into(), String::new())
        );
        for (args, message) in [
            (&["x.txt"][..], "the language is required"),
            (&["-l", "xx"], "unsupported language xx"),
            (&["-l", "ru", "-f", "json"], "unknown format json"),
            (&["-l", "ru", "--frmat"], "unknown option --frmat"),
            (&["-l"], "option -l needs a value"),
        ] {
            let (code, out, err) = command(args, b"");
            assert_eq!((code, out.as_str()), (2, ""), "{args:?}");
            assert!(
                err.starts_with("anyseg: ") && err.contains(message) && err.contains("--help"),
                "{err}"
            );
        }
    }

    #[test]
    fn encodings() {
        let (code, out, err) = command(&["-l", "ru"], &[0xFF, 0xFE, b'a', 0]);
        assert_eq!((code, out.as_str()), (1, ""));
        assert!(err.contains("not in UTF-8 (line 1)"), "{err}");
        // cp1251 before any UTF-8: another encoding
        let (code, _, err) = command(&["-l", "ru"], b"ok\n\xcf\xf0\xe8\xe2\xe5\xf2\n");
        assert!(code == 1 && err.contains("not in UTF-8 (line 2)"), "{err}");
        // The same with CR line ends: the lines of the check are those of the reading
        let (code, out, err) = command(&["-l", "ru"], b"\xd0\x81\xd0\xb6\r\xcf\xf0\xe8\r");
        assert!(
            code == 1 && out == "Ёж\n\n\u{fffd}\n\u{fffd}\n\u{fffd}\n\n",
            "{out:?}"
        );
        assert!(err.contains("line 2: not UTF-8"), "{err}");
        // A damaged line after UTF-8: replaced
        let (code, out, err) = command(
            &["-l", "ru"],
            "Ёж\n"
                .as_bytes()
                .iter()
                .chain(b"a\xffb\n")
                .copied()
                .collect::<Vec<_>>()
                .as_slice(),
        );
        assert_eq!((code, out.as_str()), (1, "Ёж\na\n\u{fffd}\nb\n\n"));
        assert!(err.contains("line 2: not UTF-8"), "{err}");
        assert_eq!(
            command(&["-l", "ru"], b""),
            (0, String::new(), String::new())
        );
    }

    #[test]
    fn files() {
        let (code, _, err) = command(&["-l", "ru", "/no/such/file"], b"");
        assert!(
            code == 1 && err.contains("/no/such/file: no such file"),
            "{err}"
        );
        let path = std::env::temp_dir().join(format!("anyseg-same-{}.txt", std::process::id()));
        std::fs::write(&path, "Кот").unwrap();
        let file = path.to_str().unwrap();
        let (code, _, err) = command(&["-l", "ru", "-o", file, file], b"");
        assert!(
            code == 1 && err.contains("the output file is an input file"),
            "{err}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Кот");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn closed_output() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut stderr = Vec::new();
        let code = execute(
            vec!["-l".into(), "ru".into()],
            "",
            false,
            false,
            &mut &b"Text"[..],
            &mut Closed,
            &mut stderr,
        );
        assert_eq!((code, stderr.len()), (0, 0));
    }
}
