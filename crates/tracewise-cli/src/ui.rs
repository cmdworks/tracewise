//! Asking a question in the terminal, and wording: written once, used by `setup`, `capture` and `editors`.
use std::io::{BufRead, IsTerminal, Read, Write};

// ---- colour: only on a terminal, and never when NO_COLOR is set (https://no-color.org) ----

/// The brand colour (orange) and the few others the screens use, as the SGR codes of an ANSI terminal.
pub(crate) const ACCENT: &str = "1;38;5;166";
pub(crate) const BOLD: &str = "1";
pub(crate) const DIM: &str = "2";
pub(crate) const GREEN: &str = "32";
pub(crate) const YELLOW: &str = "33";
pub(crate) const RED: &str = "31";
pub(crate) const CYAN: &str = "1;36";

/// Whether to colour what goes to `stream`: a terminal, `TERM` is not `dumb`, `NO_COLOR` is not set; `CLICOLOR_FORCE` forces it.
pub(crate) fn color_for(stream_is_terminal: bool) -> bool {
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    if std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| !v.is_empty() && v != "0") {
        return true;
    }
    stream_is_terminal && std::env::var("TERM").map_or(true, |t| t != "dumb")
}

/// Whether standard output gets colour.
pub(crate) fn color_on() -> bool {
    color_for(std::io::stdout().is_terminal())
}

/// `text` in the style `code`, or as it is when `enabled` is false.
pub(crate) fn paint_with(enabled: bool, code: &str, text: &str) -> String {
    if enabled {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// `text` in the style `code` when standard output gets colour.
pub(crate) fn paint(code: &str, text: &str) -> String {
    paint_with(color_on(), code, text)
}

/// `✔ text` (a check mark in green), or `ok  text` on a terminal that cannot show it.
pub(crate) fn ok_line(text: &str) -> String {
    format!("  {} {text}", paint(GREEN, "✔"))
}

pub(crate) fn warn_line(text: &str) -> String {
    format!("  {} {text}", paint(YELLOW, "!"))
}

/// Asks `question` with the default shown in brackets; an empty or unreadable answer is the default. `input` and `output` are
/// parameters so a test can answer.
pub(crate) fn ask(input: &mut dyn BufRead, out: &mut dyn Write, question: &str, default: &str) -> String {
    let _ = write!(out, "{question} [{default}]: ");
    let _ = out.flush();
    let mut line = String::new();
    let _ = input.read_line(&mut line);
    let line = line.trim();
    if line.is_empty() { default.to_string() } else { line.to_string() }
}

/// The width of the terminal in columns (at most 100, at least 40); 80 when it cannot be told.
pub(crate) fn terminal_width() -> usize {
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    let from_tty = unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut size) } == 0 && size.ws_col > 0;
    let cols = if from_tty { size.ws_col as usize } else { std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()).unwrap_or(80) };
    cols.clamp(40, 100)
}

/// Prints `text` wrapped to the terminal, indented by `indent` spaces, in the style `code`.
pub(crate) fn print_wrapped(indent: usize, text: &str, code: &str) {
    for line in wrap(text, terminal_width().saturating_sub(indent + 1)) {
        println!("{}{}", " ".repeat(indent), paint(code, &line));
    }
}

/// `text` broken into lines of at most `width` characters at spaces (a word longer than that stays whole).
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// The menu as lines: a marker on `selected`, the number, the name, and the meaning wrapped under itself so it never runs under the
/// marker column or off the edge.
fn menu_lines(options: &[(String, String)], selected: usize, width: usize, color: bool) -> Vec<String> {
    let name_w = options.iter().map(|(n, _)| n.chars().count()).max().unwrap_or(0);
    // "  ❯ 1  name  " is how far in the meaning starts: 2 spaces, the marker, a space, the number, 2 spaces, the name, 2 spaces
    let indent = 2 + 1 + 1 + 1 + 2 + name_w + 2;
    let text_w = width.saturating_sub(indent).max(20);
    let mut out = Vec::new();
    for (i, (name, about)) in options.iter().enumerate() {
        let mark = if i == selected { paint_with(color, ACCENT, "❯") } else { " ".to_string() };
        let name_style = if i == selected { ACCENT } else { CYAN };
        let wrapped = wrap(about, text_w);
        for (k, part) in wrapped.iter().enumerate() {
            if k == 0 {
                out.push(format!("  {mark} {}  {}  {part}", paint_with(color, DIM, &(i + 1).to_string()), paint_with(color, name_style, &format!("{name:<name_w$}"))));
            } else {
                out.push(format!("{}{part}", " ".repeat(indent)));
            }
        }
    }
    out
}

/// A numbered menu to answer by typing: `options` are (name, what it means); the answer is a number or a name, and an empty or
/// unknown answer is the default. Returns the index chosen.
pub(crate) fn choose(input: &mut dyn BufRead, out: &mut dyn Write, question: &str, options: &[(String, String)], default: usize) -> usize {
    let color = color_on();
    let _ = writeln!(out, "\n  {}\n", paint_with(color, BOLD, question));
    for line in menu_lines(options, default, terminal_width(), color) {
        let _ = writeln!(out, "{line}");
    }
    let _ = writeln!(out);
    let answer = ask(input, out, &format!("  Choose 1-{}", options.len()), &(default + 1).to_string());
    if let Ok(n) = answer.parse::<usize>() {
        if (1..=options.len()).contains(&n) {
            return n - 1;
        }
    }
    options.iter().position(|(n, _)| n.eq_ignore_ascii_case(&answer)).unwrap_or(default)
}

/// The terminal in raw mode (no echo, no line buffering, keys arrive one by one) until dropped, which puts it back.
struct RawMode {
    saved: libc::termios,
}

impl RawMode {
    fn on() -> Option<Self> {
        let mut t: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(0, &mut t) } != 0 {
            return None;
        }
        let saved = t;
        t.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
        t.c_cc[libc::VMIN] = 1;
        t.c_cc[libc::VTIME] = 0;
        (unsafe { libc::tcsetattr(0, libc::TCSANOW, &t) } == 0).then_some(Self { saved })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &self.saved) };
    }
}

#[derive(PartialEq, Eq, Debug)]
enum Key {
    Up,
    Down,
    Enter,
    Digit(usize),
    Abort,
    Other,
}

fn read_byte() -> Option<u8> {
    let mut b = [0u8; 1];
    (std::io::stdin().read(&mut b).ok()? == 1).then_some(b[0])
}

fn read_key() -> Option<Key> {
    Some(match read_byte()? {
        b'\r' | b'\n' => Key::Enter,
        3 | 4 | b'q' => Key::Abort,
        b'k' => Key::Up,
        b'j' => Key::Down,
        d @ b'1'..=b'9' => Key::Digit((d - b'0') as usize),
        0x1b => match (read_byte()?, read_byte()?) {
            (b'[' | b'O', b'A') => Key::Up,
            (b'[' | b'O', b'B') => Key::Down,
            _ => Key::Other,
        },
        _ => Key::Other,
    })
}

/// The menu on the real terminal: move with the arrow keys (or j and k), jump with a number, choose with Enter. Where keys cannot be
/// read one by one (a pipe, a terminal that will not switch modes) it is the typed menu of `choose`.
pub(crate) fn choose_here(question: &str, options: &[(String, String)], default: usize) -> usize {
    let color = color_on();
    let raw = if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() { RawMode::on() } else { None };
    let Some(_raw) = raw else {
        return choose(&mut std::io::stdin().lock(), &mut std::io::stdout(), question, options, default);
    };
    let mut out = std::io::stdout();
    let width = terminal_width();
    let hint = paint_with(color, DIM, "  ↑/↓ to move · enter to choose · 1-9 to jump");
    let _ = writeln!(out, "\n  {}\n", paint_with(color, BOLD, question));
    let _ = write!(out, "\x1b[?25l"); // hide the cursor while choosing
    let mut selected = default.min(options.len().saturating_sub(1));
    let mut drawn = 0usize;
    loop {
        if drawn > 0 {
            let _ = write!(out, "\x1b[{drawn}A\x1b[J");
        }
        let lines = menu_lines(options, selected, width, color);
        for line in &lines {
            let _ = writeln!(out, "{line}");
        }
        let _ = writeln!(out, "\n{hint}");
        let _ = out.flush();
        drawn = lines.len() + 2;
        match read_key() {
            Some(Key::Up) => selected = if selected == 0 { options.len() - 1 } else { selected - 1 },
            Some(Key::Down) => selected = (selected + 1) % options.len(),
            Some(Key::Digit(n)) if (1..=options.len()).contains(&n) => selected = n - 1,
            Some(Key::Enter) => break,
            Some(Key::Abort) | None => {
                let _ = write!(out, "\x1b[?25h\n");
                drop(_raw);
                eprintln!("tracewise: setup cancelled");
                std::process::exit(130);
            }
            _ => {}
        }
    }
    // leave the choice on screen in place of the menu
    let _ = write!(out, "\x1b[{drawn}A\x1b[J\x1b[?25h");
    let _ = writeln!(out, "  {} {}\n", paint_with(color, ACCENT, "❯"), paint_with(color, ACCENT, &options[selected].0));
    let _ = out.flush();
    selected
}

/// `ask` on the real terminal.
pub(crate) fn ask_here(question: &str, default: &str) -> String {
    ask(&mut std::io::stdin().lock(), &mut std::io::stdout(), question, default)
}

/// A yes/no question on the real terminal. The default is yes, and so is an input that cannot be read (a script that runs setup).
pub(crate) fn confirm(prompt: &str) -> bool {
    let _ = write!(std::io::stdout(), "{prompt} [Y/n] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return true;
    }
    let t = line.trim().to_lowercase();
    t.is_empty() || t == "y" || t == "yes"
}

/// `1 line`, `2 lines`.
pub(crate) fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_takes_a_number_a_name_or_the_default() {
        let options = vec![("agents".to_string(), "a".to_string()), ("balanced".to_string(), "b".to_string()), ("full".to_string(), "c".to_string())];
        let pick = |input: &str| choose(&mut std::io::Cursor::new(input.as_bytes().to_vec()), &mut Vec::new(), "How?", &options, 1);
        assert_eq!((pick("3\n"), pick("agents\n"), pick("\n"), pick("9\n"), pick("nonsense\n"), pick("")), (2, 0, 1, 1, 1, 1));
    }

    #[test]
    fn text_is_wrapped_at_spaces_and_the_menu_keeps_its_columns() {
        assert_eq!(wrap("one two three four", 9), ["one two", "three", "four"]);
        assert_eq!(wrap("unbreakableword", 5), ["unbreakableword"]);
        assert_eq!(wrap("", 10), [""]);
        let options = vec![("agents".to_string(), "Agents and editors only; nothing runs in the background and your shell is left alone".to_string())];
        let lines = menu_lines(&options, 0, 50, false);
        assert!(lines.len() > 1, "{lines:?}");
        assert!(lines.iter().all(|l| l.chars().count() <= 50), "no line is wider than the terminal: {lines:?}");
        let indent = lines[1].chars().take_while(|c| *c == ' ').count();
        assert!(lines[0].chars().nth(indent).is_some_and(|c| c != ' '), "the meaning continues under itself: {lines:?}");
    }

    #[test]
    fn colour_is_only_added_when_asked_for() {
        assert_eq!(paint_with(false, GREEN, "ok"), "ok");
        assert_eq!(paint_with(true, GREEN, "ok"), "\x1b[32mok\x1b[0m");
        assert!(!color_for(false), "a pipe gets no colour");
    }

    #[test]
    fn an_answer_or_the_default_and_the_plural() {
        let mut out = Vec::new();
        assert_eq!(ask(&mut std::io::Cursor::new(b" 7 \n".to_vec()), &mut out, "how many", "3"), "7");
        assert_eq!(ask(&mut std::io::Cursor::new(b"\n".to_vec()), &mut out, "how many", "3"), "3");
        assert_eq!(ask(&mut std::io::Cursor::new(Vec::new()), &mut out, "how many", "3"), "3");
        assert!(String::from_utf8(out).unwrap().contains("how many [3]: "));
        assert_eq!((plural(1, "line").as_str(), plural(0, "line").as_str(), plural(2, "character").as_str()), ("1 line", "0 lines", "2 characters"));
    }
}
