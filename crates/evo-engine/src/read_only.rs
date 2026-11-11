//! The policy behind `access: read-only` agent types (`explore`, `plan`, and
//! any agent file that asks for it): a tool call either only reads, or it is
//! refused before it reaches an approval prompt. The approval modes use the
//! same check to tell a reading shell command from one that needs approval
//! (`approval.rs`).
//!
//! * Tools known to be free of side effects pass; the mesh tools (talking to
//!   other agents) pass; every other tool is refused — file writes and also
//!   tools this policy does not know, such as connector tools.
//! * `bash` and `powershell` commands are split into simple commands
//!   (at `;`, `|`, `&&`, line ends, `(`, `)`, `{`, `}` and command
//!   substitutions, so nested commands are checked too) and each must start
//!   with a command on a read-only allow-list, with the options of that
//!   command that write or run programs refused. Output redirection to
//!   anything but the null device is refused; so are here-documents, command
//!   substitution inside double quotes, PowerShell method calls, static .NET
//!   calls, property assignment and the `&` call operator.
//! * Whatever could turn an allowed command into another one is refused too:
//!   a program named by its path, an option that reaches the command through
//!   a variable, a substitution or a file-name pattern (unless it comes after
//!   `--` or behind a literal prefix such as `./`), variables that programs or
//!   the shell act on (only lowercase shell variables and a few such as
//!   `LC_ALL` may be set), builtins that evaluate their operands, and .NET
//!   conversions outside a short list of value types.
//!
//! It is an allow-list: anything it cannot classify is refused, and so is any
//! text the two shells could read differently from this check (`$'…'`,
//! `${…}` beyond plain forms, here-strings, block comments, typographic
//! quotes). The refusal says why, so the model can retry with a plainer
//! command.

use serde_json::Value;

/// Check one tool call of a read-only agent. `Err` is the refusal shown to
/// the model.
pub fn guard(tool: &str, args: &Value) -> Result<(), String> {
    let shell = match tool {
        "bash" => Shell::Bash,
        "powershell" => Shell::PowerShell,
        name if crate::tools::info::read_only_safe(name) => return Ok(()),
        name => {
            return Err(format!(
                "Refused: this agent is read-only, and `{name}` can change state."
            ))
        }
    };
    let command = args.get("command").and_then(Value::as_str).unwrap_or("");
    check_command(command, shell).map_err(|reason| {
        format!(
            "Refused: this agent is read-only, and the command {reason}. Use read-only commands \
             (search, list, read, git log/show/diff/status/blame); split a complex command into simple ones."
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    PowerShell,
}

/// One word of a simple command.
#[derive(Debug, Clone, Default)]
struct Word {
    /// With quotes removed and their content kept.
    text: String,
    /// The unquoted characters only; each quoted part is a single `"`.
    bare: String,
    /// Holds an unquoted `$…` expansion, a command substitution or a splat:
    /// the shell may turn it into several words.
    expands: bool,
    /// Holds a `$…` expansion inside double quotes: one word, any content.
    expands_quoted: bool,
}

impl Word {
    fn plain(text: &str) -> Self {
        Word {
            text: text.to_string(),
            bare: text.to_string(),
            ..Default::default()
        }
    }

    /// Whether the shell fills in any part of this word.
    fn computed(&self) -> bool {
        self.expands || self.expands_quoted
    }

    /// Whether the word could become an option the check never saw: its
    /// start comes from a variable, a substitution or a file-name pattern.
    fn dynamic(&self, shell: Shell) -> bool {
        let lead = self.bare.chars().next();
        match shell {
            Shell::Bash => {
                self.expands
                    || (self.expands_quoted && self.text.starts_with('$'))
                    || matches!(lead, Some('*' | '?' | '['))
            }
            Shell::PowerShell => self.computed() && self.text.starts_with(['$', '@']),
        }
    }
}

/// `Ok` when every simple command in `command` only reads.
pub fn check_command(command: &str, shell: Shell) -> Result<(), String> {
    for segment in split(command, shell)? {
        check_segment(&segment, shell)?;
    }
    Ok(())
}

/// Targets a redirection may write to: the null device and the standard
/// streams, as the shell names them (`nul` is a real file to bash, and
/// `/dev/null` a real path to PowerShell).
fn null_device(target: &str, ps: bool) -> bool {
    let target = target.to_ascii_lowercase();
    if ps {
        matches!(target.as_str(), "$null" | "nul")
    } else {
        matches!(target.as_str(), "/dev/null" | "/dev/stdout" | "/dev/stderr")
    }
}

/// Characters that end a word.
fn breaks_word(c: char) -> bool {
    c.is_whitespace() || matches!(c, ';' | '|' | '&' | '(' | ')' | '{' | '}' | '<' | '>')
}

/// Characters that end a statement (PowerShell takes a lone carriage
/// return for a line end).
fn ends_line(c: char) -> bool {
    matches!(c, '\n' | '\r')
}

/// Characters one shell or the other may take for a line end or for a
/// space: a command holding one outside quotes is refused.
fn unclear_break(c: char) -> bool {
    matches!(c, '\u{0b}' | '\u{0c}' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// Whether a `$` followed by `next` starts an expansion.
fn starts_expansion(next: Option<&char>, ps: bool) -> bool {
    match next {
        None => false,
        Some(c) if ps => c.is_alphanumeric() || matches!(c, '_' | '{' | '(' | '$' | '?' | '^'),
        Some(c) => {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    '_' | '{' | '(' | '[' | '@' | '*' | '#' | '?' | '$' | '!' | '-'
                )
        }
    }
}

fn is_name(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|c: char| c.is_ascii_digit())
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Environment variables a command may set: they change how text is sorted,
/// formatted or split, not which program runs.
const ALLOWED_ENV: &[&str] = &[
    "LC_ALL",
    "LANG",
    "LC_COLLATE",
    "LC_CTYPE",
    "LC_MESSAGES",
    "LC_NUMERIC",
    "LC_TIME",
    "TZ",
    "NO_COLOR",
    "COLUMNS",
    "LINES",
    "IFS",
];

/// A variable a bash command may set for itself. Names in capitals are the
/// ones bash and programs act on (`PATH`, `PS4`, `GIT_*`, …).
fn local_name(name: &str) -> bool {
    ALLOWED_ENV.contains(&name) || !name.chars().any(|c| c.is_ascii_uppercase())
}

/// The inside of a bash `${…}`, when it is a form this check reads fully: a
/// name (or special parameter), an index that is a number, `@` or `*`, and
/// an operator whose word is plain text. Offsets must be numbers and indexes
/// may not be names: bash evaluates those as arithmetic, which runs any
/// command substitution a variable's value holds. `${x@P}` does the same.
fn bash_parameter(inner: &str) -> bool {
    let mut rest = inner;
    if let Some(after) = rest.strip_prefix(['#', '!']) {
        if after.is_empty() {
            return true;
        }
        rest = after;
    }
    let name_len = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .count();
    let name = &rest[..name_len];
    if name_len == 0 {
        match rest.chars().next() {
            Some('@' | '*' | '#' | '?' | '$' | '!' | '-') => rest = &rest[1..],
            _ => return false,
        }
    } else {
        rest = &rest[name_len..];
    }
    if let Some(after) = rest.strip_prefix('[') {
        let Some(end) = after.find(']') else {
            return false;
        };
        let index = &after[..end];
        let number = !index.is_empty() && index.chars().all(|c| c.is_ascii_digit());
        if !(number || index == "@" || index == "*") {
            return false;
        }
        rest = &after[end + 1..];
    }
    let word = |text: &str| {
        text.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    '_' | '.'
                        | '/'
                        | '*'
                        | '?'
                        | ','
                        | '+'
                        | '='
                        | '^'
                        | '~'
                        | '%'
                        | '#'
                        | ':'
                        | '-'
                )
        })
    };
    let digits = |text: &str| !text.is_empty() && text.chars().all(|c| c.is_ascii_digit());
    let mut chars = rest.chars();
    match chars.next() {
        None => true,
        Some(':') => {
            let after = chars.as_str();
            match after.chars().next() {
                Some('=') => local_name(name) && word(&after[1..]),
                Some('-' | '+' | '?') => word(&after[1..]),
                Some(c) if c.is_ascii_digit() => {
                    after.split(':').count() <= 2 && after.split(':').all(digits)
                }
                _ => false,
            }
        }
        Some('=') => local_name(name) && word(chars.as_str()),
        Some('-' | '+' | '?' | '#' | '%' | '/' | '^' | ',') => word(chars.as_str()),
        Some(_) => false,
    }
}

/// The `${…}` whose `{` is at `open`: the index after its `}`, when the
/// check reads it the way the shell does.
fn braced(chars: &[char], open: usize, ps: bool) -> Result<usize, String> {
    let Some(len) = chars[open..].iter().position(|c| *c == '}') else {
        return Err("has an unterminated `${`".into());
    };
    let inner: String = chars[open + 1..open + len].iter().collect();
    let known = if ps {
        !inner.is_empty()
            && inner
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':'))
    } else {
        bash_parameter(&inner)
    };
    if known {
        Ok(open + len + 1)
    } else {
        Err(format!(
            "uses the expansion `${{{inner}}}`, which may run or set something"
        ))
    }
}

/// .NET types a PowerShell command may name. Converting a value to a type
/// runs that type's constructor (`[IO.StreamWriter]'f'` creates the file),
/// so only value types are let through.
const POWERSHELL_TYPES: &[&str] = &[
    "int",
    "int16",
    "int32",
    "int64",
    "uint16",
    "uint32",
    "uint64",
    "long",
    "short",
    "byte",
    "sbyte",
    "double",
    "single",
    "float",
    "decimal",
    "bool",
    "boolean",
    "char",
    "string",
    "object",
    "array",
    "hashtable",
    "ordered",
    "pscustomobject",
    "psobject",
    "datetime",
    "timespan",
    "guid",
    "version",
    "uri",
    "regex",
    "xml",
    "void",
    "switch",
];

/// What PowerShell does with one finished word, beyond naming a command.
fn check_powershell_word(bare: &str) -> Result<(), String> {
    if bare.contains("::") {
        return Err("calls a .NET type member".into());
    }
    if bare == "--%" {
        return Err("uses the stop-parsing token `--%`".into());
    }
    // `$f. Delete()`: the member's name is the next word.
    if bare.len() > 1
        && bare.ends_with('.')
        && !bare.chars().all(|c| c == '.')
        && !bare.contains(['/', '\\'])
    {
        return Err("splits a member access from its name".into());
    }
    // `[type]`: a letter after `[` (an index starts with a digit, `$` or a quote).
    let chars: Vec<char> = bare.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '[' || !chars.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
            i += 1;
            continue;
        }
        let mut depth = 0usize;
        let mut end = None;
        for (at, c) in chars.iter().enumerate().skip(i) {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(at);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            return Err("has an unclosed `[`".into());
        };
        let inner: String = chars[i + 1..end].iter().collect();
        let lower = inner.to_ascii_lowercase();
        let mut name = lower.as_str();
        while let Some(element) = name.strip_suffix("[]") {
            name = element;
        }
        let name = name.strip_prefix("system.").unwrap_or(name);
        if !POWERSHELL_TYPES.contains(&name) {
            return Err(format!("names the .NET type `[{inner}]`"));
        }
        i = end + 1;
    }
    Ok(())
}

/// Split `command` into simple commands (lists of words).
fn split(command: &str, shell: Shell) -> Result<Vec<Vec<Word>>, String> {
    let ps = shell == Shell::PowerShell;
    let escape = if ps { '`' } else { '\\' };
    let chars: Vec<char> = command.chars().collect();
    let n = chars.len();
    let mut segments: Vec<Vec<Word>> = Vec::new();
    let mut words: Vec<Word> = Vec::new();
    let mut word = Word::default();
    let mut in_word = false;
    // The character before this one was an unquoted, unescaped `$`: a `{`,
    // `(`, `[` or quote right after it belongs to an expansion.
    let mut dollar = false;
    // Inside a bash `` `…` `` substitution.
    let mut in_backticks = false;
    // The word being read is what a `<` reads from: not an argument.
    let mut operand = false;

    fn end_word(
        words: &mut Vec<Word>,
        word: &mut Word,
        in_word: &mut bool,
        operand: &mut bool,
        ps: bool,
    ) -> Result<(), String> {
        if *in_word {
            if ps {
                check_powershell_word(&word.bare)?;
            }
            let word = std::mem::take(word);
            if !std::mem::take(operand) {
                words.push(word);
            }
            *in_word = false;
        }
        Ok(())
    }
    macro_rules! end_word {
        () => {
            end_word(&mut words, &mut word, &mut in_word, &mut operand, ps)?
        };
    }
    macro_rules! end_segment {
        () => {{
            end_word!();
            if !words.is_empty() {
                segments.push(std::mem::take(&mut words));
            }
        }};
    }

    let mut i = 0;
    while i < n {
        let c = chars[i];
        let after_dollar = std::mem::take(&mut dollar);
        // PowerShell reads these as quotes; this check would not.
        if ps && ('\u{2018}'..='\u{201f}').contains(&c) {
            return Err("has a typographic quote".into());
        }
        if unclear_break(c) {
            return Err("has an unusual line-break character".into());
        }
        if c == escape {
            match chars.get(i + 1) {
                // Line continuation.
                Some('\n') => i += 2,
                Some('\r') if ps => {
                    i += 2;
                    if chars.get(i) == Some(&'\n') {
                        i += 1;
                    }
                }
                Some(&next) => {
                    word.text.push(next);
                    word.bare.push(next);
                    in_word = true;
                    i += 2;
                }
                None => i += 1,
            }
            continue;
        }
        if (c == '\'' || c == '"') && after_dollar {
            return Err("uses `$'…'` or `$\"…\"` quoting".into());
        }
        if (c == '\'' || c == '"') && ps && i > 0 && chars[i - 1] == '@' {
            return Err("uses a here-string".into());
        }
        match c {
            '\'' => {
                i += 1;
                while i < n && chars[i] != '\'' {
                    word.text.push(chars[i]);
                    i += 1;
                }
                if i >= n {
                    return Err("has an unterminated quote".into());
                }
                word.bare.push('"');
                in_word = true;
                i += 1;
            }
            '"' => {
                i += 1;
                loop {
                    let Some(&d) = chars.get(i) else {
                        return Err("has an unterminated quote".into());
                    };
                    if d == '"' {
                        i += 1;
                        break;
                    }
                    if d == escape {
                        if let Some(&next) = chars.get(i + 1) {
                            // Bash keeps a backslash before other characters.
                            if !ps && !matches!(next, '$' | '`' | '"' | '\\' | '\n') {
                                word.text.push('\\');
                            }
                            word.text.push(next);
                            i += 2;
                            continue;
                        }
                    }
                    if !ps && d == '`' {
                        return Err("runs a command inside a double-quoted string".into());
                    }
                    if d == '$' {
                        match chars.get(i + 1) {
                            Some('(') => {
                                return Err("runs a command inside a double-quoted string".into())
                            }
                            Some('[') if !ps => return Err("uses `$[…]` arithmetic".into()),
                            Some('{') => {
                                let end = braced(&chars, i + 1, ps)?;
                                word.text.extend(&chars[i..end]);
                                word.expands_quoted = true;
                                i = end;
                                continue;
                            }
                            next if starts_expansion(next, ps) => word.expands_quoted = true,
                            _ => {}
                        }
                    }
                    word.text.push(d);
                    i += 1;
                }
                word.bare.push('"');
                in_word = true;
            }
            '#' if !in_word => {
                while i < n && !ends_line(chars[i]) {
                    i += 1;
                }
            }
            ';' => {
                end_segment!();
                i += 1;
            }
            c if ends_line(c) => {
                end_segment!();
                i += 1;
            }
            '|' => {
                end_segment!();
                i += if chars.get(i + 1) == Some(&'|') { 2 } else { 1 };
            }
            '&' if chars.get(i + 1) == Some(&'&') => {
                end_segment!();
                i += 2;
            }
            '&' if !ps && chars.get(i + 1) == Some(&'>') => {
                // `&>file`: both streams to a file.
                end_word!();
                i = redirect(&chars, i + 1, ps)?;
            }
            '&' if ps => return Err("uses the `&` call operator".into()),
            '&' => {
                end_segment!();
                i += 1;
            }
            // `${name}`: a variable, part of the word.
            '{' if after_dollar => {
                let end = braced(&chars, i, ps)?;
                word.text.extend(&chars[i..end]);
                word.bare.extend(&chars[i..end]);
                i = end;
            }
            '(' | ')' | '{' | '}' => {
                if c == '(' {
                    // `$x.Delete(`, `$x.'Delete'(`, `$x.$name(`.
                    if ps && in_word && word.bare.contains('.') {
                        return Err("calls a method".into());
                    }
                    // `$(…)`, `@(…)`: the word takes what the commands print.
                    if after_dollar || (ps && in_word && word.bare.ends_with('@')) {
                        word.expands = true;
                    }
                }
                // `{ list; }` groups commands; `a{b,c}` and `{a,b}` expand
                // into words this check would read as separate commands.
                let expansion = match c {
                    '{' => in_word || !chars.get(i + 1).is_none_or(|c| c.is_whitespace()),
                    '}' => in_word || !chars.get(i + 1).is_none_or(|c| breaks_word(*c)),
                    _ => false,
                };
                if !ps && expansion {
                    return Err("uses brace expansion".into());
                }
                // PowerShell hands a command the value of `(…)` and the text
                // of `{…}` as arguments: mark that the command has one.
                let group = ps && matches!(c, '(' | '{') && (in_word || !words.is_empty());
                end_word!();
                if group {
                    words.push(Word {
                        expands: true,
                        ..Word::plain(&c.to_string())
                    });
                }
                end_segment!();
                i += 1;
            }
            '`' if !ps => {
                // An opening one: the word takes what the commands print.
                if !in_backticks {
                    word.expands = true;
                    in_word = true;
                }
                in_backticks = !in_backticks;
                end_segment!();
                i += 1;
            }
            '>' => {
                // A stream number (`2>`, `*>`) belongs to the redirection.
                if in_word && word.bare.chars().all(|c| c.is_ascii_digit() || c == '*') {
                    word = Word::default();
                    in_word = false;
                } else {
                    end_word!();
                }
                i = redirect(&chars, i, ps)?;
            }
            '<' => {
                if ps && chars.get(i + 1) == Some(&'#') {
                    return Err("has a block comment".into());
                }
                let here_string = chars.get(i + 1) == Some(&'<') && chars.get(i + 2) == Some(&'<');
                if !ps && !here_string && chars.get(i + 1) == Some(&'<') {
                    return Err("uses a here-document".into());
                }
                end_word!();
                i += if here_string { 3 } else { 1 };
                // `< file`, `<<< word`: the next word, when one follows.
                operand = chars[i..]
                    .iter()
                    .find(|c| !matches!(c, ' ' | '\t'))
                    .is_some_and(|c| !breaks_word(*c));
            }
            c if c.is_whitespace() => {
                end_word!();
                i += 1;
            }
            _ => {
                if c == '$' && !after_dollar {
                    // `$$` is one variable: the second `$` opens nothing.
                    dollar = true;
                    if starts_expansion(chars.get(i + 1), ps) {
                        word.expands = true;
                    }
                }
                if c == '[' && after_dollar && !ps {
                    return Err("uses `$[…]` arithmetic".into());
                }
                // `@name`: a splatted variable.
                if ps
                    && c == '@'
                    && !in_word
                    && chars
                        .get(i + 1)
                        .is_some_and(|c| c.is_alphabetic() || *c == '_')
                {
                    word.expands = true;
                }
                word.text.push(c);
                word.bare.push(c);
                in_word = true;
                i += 1;
            }
        }
    }
    end_segment!();
    Ok(segments)
}

/// Read the redirection starting at the `>` at `start`; `Err` unless it goes
/// to a stream or the null device. Returns the index after it.
fn redirect(chars: &[char], start: usize, ps: bool) -> Result<usize, String> {
    let mut i = start + 1;
    if matches!(chars.get(i), Some('>' | '|')) {
        i += 1;
    }
    // `>&2`, `2>&1`: into another stream (`>&2file` is a file).
    if chars.get(i) == Some(&'&') {
        let digits = chars[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_digit() || **c == '-')
            .count();
        let ends = chars.get(i + 1 + digits).is_none_or(|c| breaks_word(*c));
        if digits > 0 && ends {
            return Ok(i + 1 + digits);
        }
        i += 1;
    }
    while chars.get(i).is_some_and(|c| *c == ' ' || *c == '\t') {
        i += 1;
    }
    // Quotes stay in the target, so a quoted one is never the null device.
    let mut target = String::new();
    while let Some(&c) = chars.get(i) {
        if breaks_word(c) {
            break;
        }
        target.push(c);
        i += 1;
    }
    if null_device(&target, ps) {
        Ok(i)
    } else if target.is_empty() {
        Err("redirects output".into())
    } else {
        Err(format!("writes to `{target}` with a redirection"))
    }
}

/// A command word as a plain lowercase name, without `.exe`.
fn command_name(word: &str) -> String {
    let base = word.to_ascii_lowercase();
    for suffix in [".exe", ".com"] {
        if let Some(stem) = base.strip_suffix(suffix) {
            return stem.to_string();
        }
    }
    base
}

/// `NAME=` or `NAME+=` at the start of an unquoted word: the name.
fn bash_assignment(word: &Word) -> Option<&str> {
    let eq = word.bare.find('=')?;
    let name = word.bare[..eq]
        .strip_suffix('+')
        .unwrap_or(&word.bare[..eq]);
    is_name(name).then_some(name)
}

fn check_segment(words: &[Word], shell: Shell) -> Result<(), String> {
    match shell {
        Shell::Bash => check_bash(words),
        Shell::PowerShell => check_powershell(words),
    }
}

fn refuse_variable(name: &str) -> Result<(), String> {
    Err(format!(
        "sets the variable `{name}`: only lowercase shell variables may be set, and none for a single command (except LC_ALL, LANG, TZ and IFS)"
    ))
}

fn check_bash(mut words: &[Word]) -> Result<(), String> {
    let mut assigned: Vec<&str> = Vec::new();
    while let Some(first) = words.first() {
        match first.bare.as_str() {
            "!" | "then" | "do" | "else" | "elif" | "if" | "while" | "until" | "time" => {
                words = &words[1..]
            }
            // `for NAME in values`: the values are not commands. The loop
            // variable is set like any other.
            "for" | "select" => {
                let Some(variable) = words.get(1) else {
                    return Ok(());
                };
                if !is_name(&variable.bare) || !local_name(&variable.bare) {
                    return refuse_variable(&variable.text);
                }
                match words.get(2).map(|w| w.bare.as_str()) {
                    None | Some("in") => return Ok(()),
                    // `for NAME do command`: over the arguments.
                    Some("do") => words = &words[2..],
                    Some(_) => return Err("has a loop this check cannot read".into()),
                }
            }
            "case" | "done" | "fi" | "esac" => return Ok(()),
            _ => match bash_assignment(first) {
                Some(name) => {
                    assigned.push(name);
                    words = &words[1..];
                }
                None => break,
            },
        }
    }
    // Before a command, an assignment goes into that command's environment.
    let for_a_command = !words.is_empty();
    for name in assigned {
        let allowed = if for_a_command {
            ALLOWED_ENV.contains(&name)
        } else {
            local_name(name)
        };
        if !allowed {
            return refuse_variable(name);
        }
    }
    let Some(first) = words.first() else {
        return Ok(());
    };
    if first.text.contains(['/', '\\']) {
        return Err(format!("runs `{}` by its path", first.text));
    }
    let name = command_name(&first.text);
    let args = &words[1..];
    match name.as_str() {
        "cd" | "pushd" | "popd" | "true" | "false" | ":" | "]]" | "pwd" | "shopt" | "exit"
        | "return" | "break" | "continue" | "type" | "sleep" | "wait" => Ok(()),
        "test" | "[" => check_test(args),
        // Its number comparisons and `-v` evaluate their operands.
        "[[" => match args.iter().find(|a| {
            matches!(
                a.bare.as_str(),
                "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" | "-v" | "-R"
            )
        }) {
            Some(arg) => Err(format!(
                "uses `{}` inside `[[ ]]`, which evaluates its operands",
                arg.text
            )),
            None => Ok(()),
        },
        "read" => check_read(args),
        "set" => check_set(args),
        "export" | "local" | "declare" | "typeset" | "readonly" => check_declare(&name, args),
        "unset" => match args
            .iter()
            .find(|a| !(is_name(&a.text) || a.text == "-v" || a.text == "-f") || a.computed())
        {
            Some(arg) => Err(format!("passes `unset` the argument `{}`", arg.text)),
            None => Ok(()),
        },
        "command"
            if args
                .first()
                .is_some_and(|a| a.text == "-v" || a.text == "-V") =>
        {
            Ok(())
        }
        _ => check_native(&name, args, Shell::Bash),
    }
}

/// `test` and `[`: `-v` and `-R` evaluate an array index in their operand,
/// so they are refused, and a word the shell fills in must be an operand —
/// next to a literal operator — so that it cannot become one of them.
fn check_test(args: &[Word]) -> Result<(), String> {
    const UNARY: &[&str] = &[
        "-b", "-c", "-d", "-e", "-f", "-g", "-h", "-k", "-p", "-r", "-s", "-t", "-u", "-w", "-x",
        "-L", "-S", "-N", "-O", "-G", "-n", "-z",
    ];
    const BINARY: &[&str] = &[
        "=", "==", "!=", "-eq", "-ne", "-lt", "-le", "-gt", "-ge", "-nt", "-ot", "-ef",
    ];
    let args = match args.split_last() {
        Some((last, rest)) if last.text == "]" => rest,
        _ => args,
    };
    if let Some(arg) = args.iter().find(|a| a.text == "-v" || a.text == "-R") {
        return Err(format!(
            "uses `test {}`, which evaluates its operand",
            arg.text
        ));
    }
    if args.len() <= 1 {
        return Ok(());
    }
    let operator =
        |word: &Word, names: &[&str]| !word.computed() && names.contains(&word.text.as_str());
    for (i, arg) in args.iter().enumerate() {
        if !arg.computed() {
            continue;
        }
        let before = i.checked_sub(1).map(|at| &args[at]);
        let after = args.get(i + 1);
        let operand = before.is_some_and(|w| operator(w, UNARY) || operator(w, BINARY))
            || after.is_some_and(|w| operator(w, BINARY));
        if !operand {
            return Err(format!(
                "passes `test` the variable `{}` where an operator could stand",
                arg.text
            ));
        }
    }
    Ok(())
}

/// `read`: options and plain variable names (an index would be evaluated).
fn check_read(args: &[Word]) -> Result<(), String> {
    for arg in args {
        let text = arg.text.as_str();
        let plain = text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if arg.computed() || !plain {
            return Err(format!("passes `read` the argument `{text}`"));
        }
        if !text.starts_with('-') && !local_name(text) {
            return refuse_variable(text);
        }
    }
    Ok(())
}

/// `set`: shell options that change how commands are reported or stopped,
/// not `-a` / `-k` (which put variables into every command's environment).
fn check_set(args: &[Word]) -> Result<(), String> {
    const NAMES: &[&str] = &[
        "pipefail",
        "errexit",
        "nounset",
        "xtrace",
        "noglob",
        "errtrace",
        "verbose",
        "noclobber",
    ];
    let mut names = false;
    for arg in args {
        let text = arg.text.as_str();
        if text == "--" {
            // Positional parameters from here on: values.
            return Ok(());
        }
        if arg.computed() {
            return Err(format!("passes `set` the argument `{text}`"));
        }
        if !text.starts_with(['-', '+']) && !names {
            return Ok(());
        }
        let allowed = if names {
            names = false;
            NAMES.contains(&text)
        } else {
            names = text.ends_with('o');
            text.len() > 1 && text[1..].chars().all(|c| "efuxvhoBCEHPT".contains(c))
        };
        if !allowed {
            return Err(format!("passes `set` the option `{text}`"));
        }
    }
    Ok(())
}

/// `export`, `declare` and the like: plain names and values, no attribute
/// that evaluates (`-i`), redirects (`-n`) or exports (`-x`), and `export`
/// only for the variables a command may be given.
fn check_declare(builtin: &str, args: &[Word]) -> Result<(), String> {
    for arg in args {
        let text = arg.text.as_str();
        if arg.computed() {
            return Err(format!("passes `{builtin}` the argument `{text}`"));
        }
        if let Some(letters) = text.strip_prefix(['-', '+']) {
            let allowed = if builtin == "export" {
                letters == "p"
            } else {
                !letters.is_empty() && letters.chars().all(|c| "aAfFgprlut".contains(c))
            };
            if !allowed {
                return Err(format!("passes `{builtin}` the option `{text}`"));
            }
            continue;
        }
        let name = text.split('=').next().unwrap_or("");
        let name = name.strip_suffix('+').unwrap_or(name);
        if !is_name(name) {
            return Err(format!("passes `{builtin}` the argument `{text}`"));
        }
        let allowed = match (builtin, text.contains('=')) {
            ("export", _) => ALLOWED_ENV.contains(&name),
            (_, true) => local_name(name),
            (_, false) => true,
        };
        if !allowed {
            return refuse_variable(name);
        }
    }
    Ok(())
}

/// PowerShell cmdlets and aliases that only read or format.
const POWERSHELL_SAFE: &[&str] = &[
    "get-childitem",
    "gci",
    "ls",
    "dir",
    "get-content",
    "gc",
    "cat",
    "type",
    "select-string",
    "sls",
    "get-item",
    "gi",
    "get-itemproperty",
    "gp",
    "test-path",
    "resolve-path",
    "rvpa",
    "get-location",
    "gl",
    "pwd",
    "measure-object",
    "measure",
    "select-object",
    "select",
    "where-object",
    "where",
    "?",
    "sort-object",
    "sort",
    "group-object",
    "group",
    "format-table",
    "ft",
    "format-list",
    "fl",
    "format-wide",
    "fw",
    "out-string",
    "out-null",
    "out-host",
    "convertto-json",
    "convertfrom-json",
    "convertto-csv",
    "convertfrom-csv",
    "write-output",
    "echo",
    "write",
    "write-host",
    "get-command",
    "gcm",
    "get-member",
    "gm",
    "get-date",
    "get-filehash",
    "split-path",
    "join-path",
    "compare-object",
    "diff",
    "compare",
    "get-process",
    "gps",
    "ps",
    "select-xml",
    "get-acl",
    "get-alias",
    "get-variable",
    "get-psdrive",
    "get-unique",
    "set-location",
    "cd",
    "sl",
    "push-location",
    "pushd",
    "pop-location",
    "popd",
];

/// A PowerShell number (`5`, `1.5`, `0x1F`, `1MB`) or range (`1..10`).
fn powershell_number(bare: &str) -> bool {
    let one = |text: &str| {
        let lower = text.to_ascii_lowercase();
        let mut digits = lower.as_str();
        for suffix in ["kb", "mb", "gb", "tb", "pb"] {
            if let Some(stem) = digits.strip_suffix(suffix) {
                digits = stem;
                break;
            }
        }
        if let Some(hex) = digits.strip_prefix("0x") {
            return !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit());
        }
        let digits = digits.strip_suffix(['l', 'd']).unwrap_or(digits);
        digits.starts_with(|c: char| c.is_ascii_digit())
            && digits.chars().filter(|c| *c == '.').count() <= 1
            && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
    };
    match bare.split_once("..") {
        Some((from, to)) => {
            (one(from) || from.starts_with('$')) && (one(to) || to.starts_with('$'))
        }
        None => one(bare),
    }
}

/// A word that is a value or an operator in a PowerShell expression, not a
/// command: a variable, a literal, a cast, `-not`, a property of what stands
/// before it (`.Count`; a method call was refused while splitting).
fn powershell_value(word: &Word) -> bool {
    let bare = word.bare.as_str();
    match bare.chars().next() {
        None => true,
        Some('$' | '"' | '@' | '[' | '-' | '!' | ',' | '+' | '*' | '/') => true,
        Some('.') => {
            bare.len() > 1
                && bare[1..].split('.').all(|part| {
                    part.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                        && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                })
        }
        Some(c) if c.is_ascii_digit() => powershell_number(bare),
        Some(_) => false,
    }
}

/// Whether `word` marks a `(…)` or `{…}` among a command's arguments
/// (`split` adds it; an unquoted bracket never stays in a word).
fn is_group(word: &Word) -> bool {
    matches!(word.bare.as_str(), "(" | "{")
}

fn check_powershell(words: &[Word]) -> Result<(), String> {
    let group = words.iter().any(is_group);
    let words: Vec<Word> = words.iter().filter(|w| !is_group(w)).cloned().collect();
    check_statement(&words, group)
}

/// One PowerShell statement; `group`: a `(…)` or `{…}` stood among its words.
fn check_statement(mut words: &[Word], group: bool) -> Result<(), String> {
    while let Some(first) = words.first() {
        match first.bare.to_ascii_lowercase().as_str() {
            // Alone, `foreach` is the ForEach-Object alias (checked below).
            "foreach" if words.len() > 1 => words = &words[1..],
            "if" | "else" | "elseif" | "for" | "while" | "do" | "switch" | "try" | "catch"
            | "finally" | "return" | "break" | "continue" | "throw" => words = &words[1..],
            _ => break,
        }
    }
    let Some(first) = words.first() else {
        return Ok(());
    };
    // `-as` converts to a type named by any value, a string included.
    for (i, word) in words.iter().enumerate() {
        if word.bare.eq_ignore_ascii_case("-as")
            && !words.get(i + 1).is_some_and(|t| t.bare.starts_with('['))
        {
            return Err("converts with `-as` to a type not written as `[type]`".into());
        }
    }
    let value = powershell_value(first);
    // An assignment (`$x = …`) or a hashtable entry (`n = 'Size'`, `'a' = …`).
    if let Some(index) = words.iter().position(|w| w.bare.contains('=')) {
        if value || (index == 0 && !first.bare.starts_with('-')) {
            let eq = words[index].bare.find('=').unwrap_or(0);
            let target: String = words[..index]
                .iter()
                .map(|w| w.bare.as_str())
                .chain([&words[index].bare[..eq]])
                .collect();
            let target = target.trim_end_matches(['+', '-', '*', '/', '%']);
            // `[xml]$doc = …`: the cast was checked as a type.
            let target = match target
                .strip_prefix('[')
                .and_then(|rest| rest.split_once(']'))
            {
                Some((_, variable)) if variable.starts_with('$') => variable,
                _ => target,
            };
            if target.contains('.') || target.contains('[') {
                return Err("sets a property".into());
            }
            // A plain variable or a key: `$env:X`, `$alias:x` and
            // `${function:x}` change what later commands do.
            let variable = target.strip_prefix('$').is_some_and(is_name);
            if !(variable || target == "\"" || is_name(target))
                || target.eq_ignore_ascii_case("$PSDefaultParameterValues")
            {
                return Err(format!("assigns to `{target}`, not a plain variable"));
            }
            let rest = words[index].bare[eq + 1..].trim_start_matches(['=', '+', '-', '*', '/']);
            let mut assigned: Vec<Word> = Vec::new();
            if !rest.is_empty() {
                assigned.push(Word::plain(rest));
            }
            assigned.extend(words[index + 1..].iter().cloned());
            return check_statement(&assigned, group);
        }
    }
    if value {
        // The rest of an expression is values and operators; a command
        // stands only after `in` (`foreach ($x in command)`).
        for (i, word) in words.iter().enumerate().skip(1) {
            if word.bare.eq_ignore_ascii_case("in") {
                return check_statement(&words[i + 1..], group);
            }
            if !(powershell_value(word) || word.bare == "%") {
                return Err(format!("has `{}` after a value", word.text));
            }
        }
        return Ok(());
    }
    if first.bare.contains(['/', '\\']) {
        return Err(format!("runs `{}` by its path", first.text));
    }
    let name = command_name(&first.text);
    let args = &words[1..];
    // `sort.exe` is the program, not the alias of Sort-Object.
    let lower = first.text.to_ascii_lowercase();
    let program = lower.ends_with(".exe") || lower.ends_with(".com");
    if !program {
        if matches!(name.as_str(), "foreach-object" | "%" | "foreach") {
            // A bare word names a member to call on each item (`% Delete`).
            return match args.iter().find(|a| {
                !matches!(
                    a.bare.to_ascii_lowercase().as_str(),
                    "-process" | "-begin" | "-end"
                )
            }) {
                Some(arg) => Err(format!("calls `{}` on each item", arg.text)),
                None => Ok(()),
            };
        }
        if POWERSHELL_SAFE.contains(&name.as_str()) {
            return Ok(());
        }
    }
    if group && !NATIVE_SAFE.contains(&name.as_str()) {
        return Err(format!(
            "passes `{name}` the value of `(…)` or a script block: write its arguments out"
        ));
    }
    check_native(&name, args, Shell::PowerShell)
}

/// Programs that only read, whatever their arguments.
const NATIVE_SAFE: &[&str] = &[
    "cat",
    "head",
    "tail",
    "ls",
    "dir",
    "pwd",
    "echo",
    "wc",
    "cut",
    "tr",
    "nl",
    "column",
    "stat",
    "du",
    "df",
    "which",
    "whereis",
    "where",
    "basename",
    "dirname",
    "realpath",
    "readlink",
    "grep",
    "egrep",
    "fgrep",
    "diff",
    "cmp",
    "comm",
    "jq",
    "whoami",
    "uname",
    "id",
    "printenv",
    "ps",
    "tac",
    "rev",
    "fold",
    "paste",
    "join",
    "seq",
    "expr",
    "strings",
    "od",
    "hexdump",
    "md5sum",
    "sha1sum",
    "sha256sum",
    "findstr",
    "true",
    "false",
];

/// Programs that read unless given one of their writing options.
const NATIVE_CHECKED: &[&str] = &[
    "sort", "tree", "rg", "fd", "fdfind", "find", "uniq", "git", "file", "date", "hostname",
];

fn check_native(name: &str, args: &[Word], shell: Shell) -> Result<(), String> {
    if NATIVE_SAFE.contains(&name) {
        return Ok(());
    }
    if name == "printf" {
        // `printf -v name` sets a variable (and evaluates an index in it).
        return match args.first() {
            Some(first)
                if first.dynamic(shell) || (first.text.starts_with('-') && first.text != "--") =>
            {
                Err(format!(
                    "passes `printf` `{}` where its format stands",
                    first.text
                ))
            }
            _ => Ok(()),
        };
    }
    if !NATIVE_CHECKED.contains(&name) {
        return Err(format!("runs `{name}`, which is not on the read-only list"));
    }
    if shell == Shell::PowerShell {
        if let Some(arg) = args.iter().find(|a| a.bare.contains(',')) {
            return Err(format!(
                "passes `{name}` the list `{}`, which becomes several arguments: quote it",
                arg.text
            ));
        }
    }
    // Options end at `--` (`find` has no such marker).
    let end = match name {
        "find" => args.len(),
        _ => args
            .iter()
            .position(|a| a.text == "--")
            .unwrap_or(args.len()),
    };
    let opts = &args[..end];
    if let Some(arg) = opts.iter().find(|a| a.dynamic(shell)) {
        return Err(format!(
            "passes `{name}` the argument `{}`, which could turn into an option: put it after `--`, or start it with a literal such as `./`",
            arg.text
        ));
    }
    // A cluster of short options holding one of `letters` (`-uo`, `-oout`).
    let short = |arg: &Word, letters: &[char]| {
        arg.text.starts_with('-') && !arg.text.starts_with("--") && arg.text.contains(letters)
    };
    let long = |arg: &Word, prefixes: &[&str]| prefixes.iter().any(|p| arg.text.starts_with(p));
    let exact = |arg: &Word, names: &[&str]| names.contains(&arg.text.as_str());
    let found = match name {
        // `--o…` abbreviates `--output`, `--co…` `--compress-program`; `/O`
        // is the output file of the Windows program.
        "sort" => opts.iter().find(|a| {
            short(a, &['o'])
                || long(a, &["--o", "--co"])
                || (shell == Shell::PowerShell && a.text.to_ascii_lowercase().starts_with("/o"))
        }),
        "tree" => opts.iter().find(|a| short(a, &['o'])),
        "rg" => opts.iter().find(|a| long(a, &["--pre", "--hostname-bin"])),
        "fd" | "fdfind" => opts
            .iter()
            .find(|a| short(a, &['x', 'X']) || long(a, &["--exec"])),
        "find" => opts.iter().find(|a| {
            exact(
                a,
                &[
                    "-delete", "-exec", "-execdir", "-ok", "-okdir", "-fprint", "-fprint0",
                    "-fprintf", "-fls",
                ],
            )
        }),
        "file" => opts.iter().find(|a| short(a, &['C']) || long(a, &["--co"])),
        "date" => opts
            .iter()
            .find(|a| (short(a, &['s']) && !a.text.starts_with("-I")) || long(a, &["--s"])),
        // A name sets the host name.
        "hostname" => args.iter().find(|a| {
            !exact(
                a,
                &[
                    "-s",
                    "-f",
                    "-i",
                    "-I",
                    "-d",
                    "-A",
                    "-y",
                    "--short",
                    "--fqdn",
                    "--long",
                    "--ip-address",
                    "--all-ip-addresses",
                    "--domain",
                    "--version",
                    "--help",
                ],
            )
        }),
        "uniq" => {
            // A second file operand is an output file (`-` is one too).
            let operands = opts
                .iter()
                .filter(|a| !a.text.starts_with('-') || a.text == "-")
                .count()
                + args.len().saturating_sub(end + 1);
            return if operands > 1 {
                Err("passes `uniq` an output file".into())
            } else {
                Ok(())
            };
        }
        _ => return check_git(args),
    };
    match found {
        Some(arg) => Err(format!(
            "passes `{name}` the option `{}`, which writes or runs a program",
            arg.text
        )),
        None => Ok(()),
    }
}

/// Git subcommands that only read.
const GIT_READ: &[&str] = &[
    "log",
    "show",
    "diff",
    "status",
    "blame",
    "annotate",
    "grep",
    "ls-files",
    "ls-tree",
    "rev-parse",
    "rev-list",
    "cat-file",
    "describe",
    "shortlog",
    "merge-base",
    "name-rev",
    "show-ref",
    "for-each-ref",
    "whatchanged",
    "count-objects",
    "check-ignore",
    "check-attr",
    "diff-tree",
    "diff-files",
    "diff-index",
    "version",
    "var",
    "show-branch",
    "cherry",
    "range-diff",
];

fn check_git(args: &[Word]) -> Result<(), String> {
    let mut i = 0;
    while let Some(arg) = args.get(i).filter(|a| a.text.starts_with('-')) {
        let text = arg.text.as_str();
        match text {
            "--no-pager"
            | "-P"
            | "--no-optional-locks"
            | "--literal-pathspecs"
            | "--no-replace-objects"
            | "--version" => i += 1,
            "-C" => i += 2,
            _ if text.starts_with("--git-dir=") || text.starts_with("--work-tree=") => i += 1,
            // `-c` can point a pager or diff driver at any program.
            _ => return Err(format!("passes git the option `{text}`")),
        }
    }
    let Some(sub) = args.get(i) else {
        return Ok(());
    };
    let sub = sub.text.as_str();
    let rest = &args[i + 1..];
    // Options end at `--`: what follows are paths.
    let end = rest.iter().position(|a| a.text == "--");
    let opts = &rest[..end.unwrap_or(rest.len())];
    let flag = |a: &Word| a.text.split('=').next().unwrap_or("").to_string();
    let any = |names: &[&str]| opts.iter().any(|a| names.contains(&flag(a).as_str()));
    let first_operand = opts
        .iter()
        .find(|a| !a.text.starts_with('-'))
        .map(|a| a.text.as_str());
    // `--output`, and any abbreviation git takes for it (`--outp`).
    let writes_file = |a: &&Word| {
        let flag = flag(a);
        flag.len() >= 3
            && ("--output".starts_with(flag.as_str())
                || (flag.starts_with("--output") && !flag.starts_with("--output-indicator")))
    };
    if let Some(arg) = opts.iter().find(writes_file) {
        return Err(format!(
            "passes git the option `{}`, which writes a file",
            arg.text
        ));
    }
    if any(&["--help"]) {
        return Err("passes git `--help`, which opens a viewer".into());
    }
    let refuse = || {
        Err(format!(
            "runs `git {sub}` in a way that can change the repository"
        ))
    };
    match sub {
        // `-O<pager>`, `--open-files-in-pager` and its abbreviations.
        "grep"
            if opts
                .iter()
                .any(|a| a.text.starts_with("-O") || a.text.starts_with("--open")) =>
        {
            refuse()
        }
        _ if GIT_READ.contains(&sub) => Ok(()),
        "branch" | "tag" => {
            let listing: &[&str] = &[
                "-a",
                "--all",
                "-r",
                "--remotes",
                "-v",
                "-vv",
                "--verbose",
                "-l",
                "--list",
                "--show-current",
                "--contains",
                "--no-contains",
                "--merged",
                "--no-merged",
                "--points-at",
                "--sort",
                "--format",
                "--color",
                "--no-color",
                "--column",
                "--no-column",
                "-i",
                "--ignore-case",
                "--abbrev",
                "--no-abbrev",
                "-n",
            ];
            let flags_ok = opts.iter().filter(|a| a.text.starts_with('-')).all(|a| {
                let f = flag(a);
                listing.contains(&f.as_str())
                    || (f.starts_with("-n") && f[2..].chars().all(|c| c.is_ascii_digit()))
            });
            // Only these make a name a pattern; without one (and with
            // `--sort` or `--format` alone) a name creates a branch or tag.
            let lists = any(&[
                "-l",
                "--list",
                "--contains",
                "--no-contains",
                "--merged",
                "--no-merged",
                "--points-at",
            ]);
            if end.is_none() && flags_ok && (first_operand.is_none() || lists) {
                Ok(())
            } else {
                refuse()
            }
        }
        "remote" => match first_operand {
            None | Some("show" | "get-url") => Ok(()),
            _ => refuse(),
        },
        "stash" | "notes" => match first_operand {
            Some("list" | "show") => Ok(()),
            _ => refuse(),
        },
        "reflog" => match first_operand {
            None | Some("show") => Ok(()),
            _ => refuse(),
        },
        "worktree" if first_operand == Some("list") => Ok(()),
        "submodule" if first_operand == Some("status") => Ok(()),
        "config" if end.is_none() && reads_git_config(opts) => Ok(()),
        _ => refuse(),
    }
}

/// `git config` when it only reads: `get` / `list`, one of the reading
/// options, or a single key. Every option must be one that reading takes, so
/// a value that looks like a reading option (`git config key --list`) does
/// not pass for one.
fn reads_git_config(opts: &[Word]) -> bool {
    const READS: &[&str] = &[
        "--get",
        "--get-all",
        "--get-regexp",
        "--get-urlmatch",
        "--list",
        "-l",
    ];
    const TAKES_VALUE: &[&str] = &[
        "--file",
        "-f",
        "--blob",
        "--type",
        "--default",
        "--url",
        "--value",
    ];
    const OTHER: &[&str] = &[
        "--global",
        "--local",
        "--system",
        "--worktree",
        "--show-origin",
        "--show-scope",
        "--show-names",
        "--no-show-names",
        "--name-only",
        "-z",
        "--null",
        "--bool",
        "--int",
        "--bool-or-int",
        "--path",
        "--expiry-date",
        "--no-type",
        "--includes",
        "--no-includes",
        "--all",
        "--regexp",
        "--fixed-value",
    ];
    let mut operands: Vec<&str> = Vec::new();
    let mut reads = false;
    let mut skip = false;
    for arg in opts {
        let text = arg.text.as_str();
        if std::mem::take(&mut skip) {
            continue;
        }
        if !text.starts_with('-') {
            operands.push(text);
            continue;
        }
        let flag = text.split('=').next().unwrap_or("");
        if READS.contains(&flag) {
            reads = true;
        } else if TAKES_VALUE.contains(&flag) {
            skip = !text.contains('=');
        } else if !OTHER.contains(&flag) {
            return false;
        }
    }
    match operands.as_slice() {
        ["get" | "list", ..] => true,
        ["set" | "unset" | "rename-section" | "remove-section" | "edit", ..] => false,
        _ if reads => true,
        [_key] => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
