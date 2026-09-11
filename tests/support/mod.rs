//! Direct-rustc fixtures use the artifacts Cargo actually produced.

use std::{collections::BTreeSet, ffi::OsString, path::PathBuf, process::Command};

pub struct Artifacts {
    metadata: PathBuf,
    library: PathBuf,
    directories: BTreeSet<PathBuf>,
}

impl Artifacts {
    pub fn build(command: &mut Command) -> Self {
        let output = command
            .arg("--message-format=json-render-diagnostics")
            .output()
            .expect("build direct-rustc fixture dependencies");
        assert!(
            output.status.success(),
            "dependency build failed: {}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout),
        );
        Self::from_messages(std::str::from_utf8(&output.stdout).expect("Cargo emits UTF-8 JSON"))
    }

    pub fn from_messages(messages: &str) -> Self {
        let files: BTreeSet<PathBuf> = messages
            .lines()
            .flat_map(|line| filenames(line).expect("valid Cargo artifact filenames"))
            .map(PathBuf::from)
            .collect();
        let metadata: Vec<_> = files
            .iter()
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "rmeta")
                    && path.file_stem().is_some_and(|name| {
                        let name = name.to_string_lossy();
                        name == "libsystasis" || name.starts_with("libsystasis-")
                    })
            })
            .cloned()
            .collect();
        assert_eq!(
            metadata.len(),
            1,
            "expected one systasis metadata artifact: {metadata:?}"
        );
        let libraries: Vec<_> = files
            .iter()
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "rlib")
                    && path.file_stem().is_some_and(|name| {
                        let name = name.to_string_lossy();
                        name == "libsystasis" || name.starts_with("libsystasis-")
                    })
            })
            .cloned()
            .collect();
        assert_eq!(
            libraries.len(),
            1,
            "expected one systasis library artifact: {libraries:?}"
        );
        Self {
            metadata: metadata.into_iter().next().expect("length checked above"),
            library: libraries.into_iter().next().expect("length checked above"),
            directories: files
                .iter()
                .filter_map(|path| path.parent().map(PathBuf::from))
                .collect(),
        }
    }

    pub fn rustc(&self) -> Command {
        let mut command = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
        // Newer Cargo separates metadata from the rlib. Passing both artifacts
        // supports metadata-only checks and linking without guessing their paths.
        for path in [&self.library, &self.metadata] {
            let mut external = OsString::from("systasis=");
            external.push(path);
            command.arg("--extern").arg(external);
        }
        for directory in &self.directories {
            let mut search = OsString::from("dependency=");
            search.push(directory);
            command.arg("-L").arg(search);
        }
        command
    }
}

// Only Cargo's top-level `filenames` array is needed. Read JSON strings rather
// than splitting on commas/quotes: workspace paths can contain either. Nested
// objects and string contents cannot impersonate this top-level property.
fn filenames(mut input: &str) -> Result<Vec<String>, &'static str> {
    let mut depth = 0_u32;
    loop {
        input = input.trim_start();
        match input.as_bytes().first() {
            None => return Ok(Vec::new()),
            Some(b'{' | b'[') => depth += 1,
            Some(b'}' | b']') => depth = depth.checked_sub(1).ok_or("unbalanced JSON")?,
            Some(b'"') => {
                let key = string(&mut input)?;
                if depth == 1 && key == "filenames" && input.trim_start().starts_with(':') {
                    input = input
                        .trim_start()
                        .strip_prefix(':')
                        .ok_or("missing colon")?;
                    input = input
                        .trim_start()
                        .strip_prefix('[')
                        .ok_or("expected filenames array")?;
                    let mut files = Vec::new();
                    loop {
                        input = input.trim_start();
                        if input.starts_with(']') {
                            return Ok(files);
                        }
                        files.push(string(&mut input)?);
                        input = input.trim_start();
                        match input.as_bytes().first() {
                            Some(b',') => input = &input[1..],
                            Some(b']') => return Ok(files),
                            _ => return Err("expected array delimiter"),
                        }
                    }
                }
                continue;
            }
            _ => {}
        }
        input = &input[1..];
    }
}

fn string(input: &mut &str) -> Result<String, &'static str> {
    *input = input.strip_prefix('"').ok_or("expected JSON string")?;
    let mut decoded = String::new();
    loop {
        let character = input.chars().next().ok_or("unterminated JSON string")?;
        *input = &input[character.len_utf8()..];
        match character {
            '"' => return Ok(decoded),
            '\\' => {
                let escape = input.chars().next().ok_or("incomplete JSON escape")?;
                *input = &input[escape.len_utf8()..];
                decoded.push(match escape {
                    '"' | '\\' | '/' => escape,
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'u' => {
                        let first = code_unit(input)?;
                        let scalar = if (0xd800..=0xdbff).contains(&first) {
                            *input = input.strip_prefix("\\u").ok_or("missing low surrogate")?;
                            let second = code_unit(input)?;
                            if !(0xdc00..=0xdfff).contains(&second) {
                                return Err("invalid low surrogate");
                            }
                            0x10000 + ((first - 0xd800) << 10) + second - 0xdc00
                        } else {
                            first
                        };
                        char::from_u32(scalar).ok_or("invalid Unicode scalar")?
                    }
                    _ => return Err("invalid JSON escape"),
                });
            }
            '\u{0}'..='\u{1f}' => return Err("unescaped JSON control character"),
            _ => decoded.push(character),
        }
    }
}

fn code_unit(input: &mut &str) -> Result<u32, &'static str> {
    let digits = input.get(..4).ok_or("incomplete Unicode escape")?;
    let unit = u32::from_str_radix(digits, 16).map_err(|_| "invalid Unicode escape")?;
    *input = &input[4..];
    Ok(unit)
}
