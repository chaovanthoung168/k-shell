//! Side-effect-free checks for a complete proposed command. Never execute a
//! suggestion to discover whether it works. Unknown external arguments stay
//! neutral until an actual execution supplies a result.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use crate::{commands, language::parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verdict {
    Valid,
    Invalid,
    Unknown,
}

#[derive(Default)]
pub(super) struct Feedback(VecDeque<(PathBuf, Vec<String>, bool)>);

impl Feedback {
    pub(super) fn record(&mut self, cwd: &Path, line: &str, status: i32) {
        // Interrupting a dev server does not make its command invalid.
        if status == 130 {
            return;
        }
        let Ok(Some(command)) = parser::parse(line) else {
            return;
        };
        let words: Vec<String> = command.words.iter().map(|word| word.literal()).collect();
        self.0
            .retain(|(directory, args, _)| directory != cwd || args != &words);
        self.0.push_front((cwd.to_path_buf(), words, status == 0));
        self.0.truncate(200);
    }

    pub(super) fn verdict(&self, cwd: &Path, args: &[String]) -> Option<Verdict> {
        self.0
            .iter()
            .find(|(directory, words, _)| directory == cwd && words == args)
            .map(|(_, _, success)| {
                if *success {
                    Verdict::Valid
                } else {
                    Verdict::Invalid
                }
            })
    }
}

pub(super) fn check(
    line: &str,
    cwd: &Path,
    feedback: &Feedback,
    available: impl FnOnce(&str) -> bool,
) -> Verdict {
    let Ok(Some(command)) = parser::parse(line) else {
        return Verdict::Invalid;
    };
    let args: Vec<String> = command.words.iter().map(|word| word.literal()).collect();
    if !available(&args[0]) {
        return Verdict::Invalid;
    }
    let syntax = if let Some(valid) = commands::valid_preview_syntax(&args) {
        if valid {
            Verdict::Valid
        } else {
            Verdict::Invalid
        }
    } else if Path::new(&args[0])
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.eq_ignore_ascii_case("docker") || name.eq_ignore_ascii_case("docker.exe")
        })
    {
        docker(&args[1..])
    } else if args.len() == 1 {
        Verdict::Valid
    } else {
        Verdict::Unknown
    };
    if syntax == Verdict::Invalid {
        return syntax;
    }
    feedback.verdict(cwd, &args).unwrap_or(syntax)
}

/// Docker's documented global options, checked before the subcommand. Do not
/// pretend to validate plugin/subcommand arguments with a root-option table.
fn docker(args: &[String]) -> Verdict {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            return Verdict::Unknown;
        }
        if !arg.starts_with('-') || arg == "-" {
            return Verdict::Unknown;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, value) = long
                .split_once('=')
                .map_or((long, None), |(name, value)| (name, Some(value)));
            match name {
                "help" | "version" | "debug" | "tls" | "tlsverify" => {
                    if value.is_some_and(|value| {
                        !matches!(
                            value,
                            "true"
                                | "false"
                                | "1"
                                | "0"
                                | "t"
                                | "f"
                                | "TRUE"
                                | "FALSE"
                                | "True"
                                | "False"
                                | "T"
                                | "F"
                        )
                    }) {
                        return Verdict::Invalid;
                    }
                }
                "config" | "context" | "host" | "log-level" | "tlscacert" | "tlscert"
                | "tlskey" => {
                    if value.is_none() {
                        index += 1;
                        if args.get(index).is_none() {
                            return Verdict::Invalid;
                        }
                    }
                }
                _ => return Verdict::Invalid,
            }
        } else {
            for (offset, flag) in arg[1..].char_indices() {
                match flag {
                    'h' | 'v' | 'D' => {
                        if let Some(value) = arg[1 + offset + flag.len_utf8()..].strip_prefix('=') {
                            if !matches!(
                                value,
                                "true"
                                    | "false"
                                    | "1"
                                    | "0"
                                    | "t"
                                    | "f"
                                    | "TRUE"
                                    | "FALSE"
                                    | "True"
                                    | "False"
                                    | "T"
                                    | "F"
                            ) {
                                return Verdict::Invalid;
                            }
                            break;
                        }
                    }
                    'c' | 'H' | 'l' => {
                        if offset + flag.len_utf8() == arg[1..].len() {
                            index += 1;
                            if args.get(index).is_none() {
                                return Verdict::Invalid;
                            }
                        }
                        break;
                    }
                    _ => return Verdict::Invalid,
                }
            }
        }
        index += 1;
    }
    Verdict::Valid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_bad_global_flags_are_red_before_any_execution() {
        let feedback = Feedback::default();
        for line in [
            "docker --sdf",
            "docker --unknown",
            "docker -z",
            "docker --context",
            "docker --debug=maybe",
        ] {
            assert_eq!(
                check(line, Path::new("."), &feedback, |_| true),
                Verdict::Invalid,
                "{line}"
            );
        }
        for line in [
            "docker",
            "docker --help",
            "docker --version",
            "docker -Dv",
            "docker --context local --help",
            "docker --config='folder with spaces' --help",
            "docker -Htcp://localhost:2375 --help",
        ] {
            assert_eq!(
                check(line, Path::new("."), &feedback, |_| true),
                Verdict::Valid,
                "{line}"
            );
        }
        assert_eq!(
            check("docker ps --sdf", Path::new("."), &feedback, |_| true),
            Verdict::Unknown
        );
    }

    #[test]
    fn feedback_tracks_exact_argument_vectors_cwd_and_latest_result() {
        let mut feedback = Feedback::default();
        feedback.record(Path::new("one"), "tool --bad 'a b'", 2);
        assert_eq!(
            check("tool  --bad \"a b\"", Path::new("one"), &feedback, |_| true),
            Verdict::Invalid
        );
        assert_eq!(
            check("tool --bad other", Path::new("one"), &feedback, |_| true),
            Verdict::Unknown
        );
        assert_eq!(
            check("tool --bad 'a b'", Path::new("two"), &feedback, |_| true),
            Verdict::Unknown
        );
        feedback.record(Path::new("one"), "tool --bad 'a b'", 0);
        assert_eq!(
            check("tool --bad 'a b'", Path::new("one"), &feedback, |_| true),
            Verdict::Valid
        );
        feedback.record(Path::new("one"), "tool --bad 'a b'", 130);
        assert_eq!(
            check("tool --bad 'a b'", Path::new("one"), &feedback, |_| true),
            Verdict::Valid
        );
        assert_eq!(
            check("tool --bad 'a b'", Path::new("one"), &feedback, |_| false),
            Verdict::Invalid
        );
    }

    #[test]
    fn unsupported_shell_syntax_is_red_without_running_anything() {
        for line in [
            "echo hi | cat",
            "echo 'unterminated",
            "echo hi > file",
            "echo a; echo b",
        ] {
            assert_eq!(
                check(line, Path::new("."), &Feedback::default(), |_| true),
                Verdict::Invalid
            );
        }
    }
}
