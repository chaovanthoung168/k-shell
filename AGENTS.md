# AGENTS.md — K Shell

## Mission
Build **K Shell (`kshell`)**, a native Windows command shell written in Rust. It should offer a familiar Zsh-like interactive experience and Linux-style commands while executing Windows applications natively. It must not use PowerShell, CMD, Git Bash, or WSL as its general-purpose parser or executor. Explicit compatibility modes for `.cmd`/`.bat` scripts may invoke `cmd.exe` when documented.

This file instructs coding agents working in this repository. Treat the existing repository as the source of truth: inspect code, `Cargo.toml`, tests, and README before changing anything. Do not assume every feature below is already implemented.

Product branding is K Shell, the Cargo library/package and executable are
`kshell` / `kshell.exe`; the repository folder remains `myshell`. Startup settings
prefer `KSHELL_*` but accept the corresponding legacy `MYSH_*` names when absent,
including empty-value precedence. Keep the existing `%APPDATA%\\mysh\\history.txt`
(home fallback `.mysh/history.txt`) location for history continuity; no migration
or rewrite of users' files is performed by the rename.

Distribution: `scripts/Build-Release.ps1` builds a locked, offline Windows x64
release with static CRT/remapped paths and packages only an explicit whitelist.
Include license texts/notices and a SHA256 manifest; never package history,
user configuration, font files or external application installations. The
per-user distribution installer verifies the binary checksum first, protects
unmarked folders, backs up upgrades and uses standard icons/font fallback when
Nerd Fonts are absent. Setup scripts may use PowerShell for installation only;
the shell's runtime/parser never delegates user input to them. Tests use private
install/profile destinations and never modify the real user's installation.

## Goals and non-goals

### Goals
- Native Windows executable (`kshell.exe`) with interactive REPL and `-c` command mode.
- Own lexer, parser, AST, expansion rules, and execution engine.
- Execute native Windows programs (`git`, `cargo`, `python`, etc.).
- Provide familiar Unix-like commands (`ls`, `cat`, `grep`, `mkdir`, `touch`, `cp`, `mv`, `rm`, `find`, `which`, etc.).
- Support pipes, file redirection, shell variables, environment variables, aliases, history, autocomplete, syntax highlighting, and configurable prompts incrementally.
- Prioritize correctness, predictable behavior, safety, testability, and clear error messages.

### Non-goals for initial releases
- Full POSIX, Bash, or Zsh compatibility.
- Running Linux ELF executables without a Linux compatibility environment.
- Implementing a terminal emulator; use Windows Terminal or another host.
- PowerShell object pipelines; K Shell pipelines are byte streams.
- A plugin ABI before core syntax and execution are stable.

## Technical baseline
- Language: stable Rust, edition determined by the repository's `Cargo.toml`.
- Platform: Windows 10/11, with Windows-specific APIs isolated behind `platform` modules.
- Prefer Rust standard library for core process launching and filesystem operations.
- Consider `reedline` or `rustyline` for interactive editing; inspect existing dependencies first and avoid introducing competing editors.
- Optional crates when justified: `windows`, `serde`, `toml`, `thiserror`, `dirs`, `nu-ansi-term`.
- Never add dependencies without explaining why they are necessary.

## Architectural boundaries

```text
Windows Terminal / console host
           |
Interactive UI: prompt, editor, history, completion, highlighting
           |
Language: lexer -> parser -> AST -> expansion
           |
Runtime: dispatcher, built-ins, pipelines, redirection, shell state
           |
Commands: Unix-like internal utilities | external executables
           |
Platform: Windows processes, paths, console, filesystem
```

Keep the following responsibilities separate:

1. **Lexer**: tokenize operators, words, quoting and escaping; preserve sufficient quote context for later expansion.
2. **Parser**: construct AST and report syntax errors with spans; never execute programs.
3. **Expansion**: resolve tilde, variables and later globbing/substitution in documented order; do not expand single-quoted text.
4. **Executor**: dispatch commands, create processes, connect streams, apply redirects and return exit statuses.
5. **Built-ins**: mutate shell state only when appropriate (`cd`, `export`, `unset`, `exit`, `alias`).
6. **Utilities**: Linux-like commands with consistent argument handling and stream-based I/O.
7. **Platform**: contain Windows-specific APIs, executable lookup and console/process behavior.
8. **Interactive**: depend on the language/runtime APIs rather than reimplementing parsing.

`open` is an explicit Windows file-association action implemented in
`platform::open` with `ShellExecuteExW`; it does not parse or execute shell input.
Keep its UTF-16 paths and COM lifecycle inside that module. Normal executable
launching still uses `std::process::Command`, and `open` rejects `.exe`, `.com`,
`.cmd`, and `.bat` files. Automated tests must not launch users' desktop apps.

The Windows `code` adapter in `platform::vscode` launches VS Code's native
`Code.exe` with its packaged `cli.js` and child-only `ELECTRON_RUN_AS_NODE=1`.
It reads only the known quoted installation-relative CLI path from `code.cmd`
as metadata, never executes that wrapper, and is not general batch compatibility.

The Windows `npm`/`npx` adapter in `platform::npm` launches native `node.exe`
with the selected installation's `node_modules/npm/bin/npm-cli.js` or
`npx-cli.js`. Discovery only checks known local paths; never execute or parse
wrapper scripts or run npm prefix helpers during discovery/completion. Prefer
PATH installations in order and adjacent Node, with a resolved native Node
fallback for a separate npm prefix. Prepend Node's directory to child PATH only.
Native npm/npx executables retain precedence; explicit script paths stay rejected.
Package-script execution belongs to npm and can use its normal Windows script
shell (usually CMD); K Shell still does not delegate its own input parsing.
Automated npm checks use fixture packages and offline mode, never install
packages, and keep cache/config/log files within test-owned temporary directories.

Hardware snapshots (`sysinfo`, alias `specs`) use read-only native APIs isolated
in `platform::system`. Keep typed snapshot data separate from command rendering.
Report unavailable sections/volumes individually; do not invent hardware values
or invoke PowerShell/WMIC. Additional `windows-sys` feature flags expose these
APIs without a new dependency. Storage describes local fixed drive-letter volumes,
not physical disks; RAM is usable physical memory, not installed DIMM capacity.
System snapshots also include typed OS edition/release/build data from the
CurrentVersion registry and the current logon name from `GetUserNameW`, never
OS/USERNAME environment guesses. Correct legacy Windows 10 client product labels
for Windows 11 builds >= 22000 without relabeling Server products. OS/user errors
remain independent; optional release/revision values are omitted when unavailable.

SSH uses the existing native executable runtime and inherited terminal streams.
After normal PATH lookup, bare `ssh`/`ssh.exe` may fall back to Windows' system
OpenSSH client through `platform::ssh`; explicit paths never use this fallback.
Leave authentication, host keys and config handling to OpenSSH. Automated tests
must not connect to servers; use `-V` or `-G -F none` for offline integration checks.

Curl uses the same native runtime and streams. `platform::curl` may resolve bare
`curl`/`curl.exe` from Windows' native system directory only after normal PATH
lookup; explicit paths never fall back. Keep requests/TLS/options/configuration
in curl, not a new built-in or wrapper. Completion uses cached local discovery.
Automated checks disable curl configuration with first-option `-q` and use only
test-owned loopback servers/files, with timeouts; never contact external services.

Nano uses the native executable runtime with inherited terminal streams and
normal PATH precedence. `platform::nano` provides a Windows-only discovery
fallback for bare `nano`/`nano.exe` to Git for Windows' packaged `usr/bin/nano.exe`.
Check known Git roots locally, never run discovery subprocesses or shell wrappers.
Explicit paths never fall back; no installation or global PATH changes occur.
Keep it an external editor; automated checks use native fixtures/`--version`.
The existing isolated-console/timeout job test may edit/save a test-owned file
with Nano's user configuration disabled, then verify return to K Shell. Never
open user files or an uncontrolled interactive editor.

Suggested structure; adapt incrementally rather than reorganizing working code unnecessarily:

```text
kshell/
├── AGENTS.md
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── error.rs
│   ├── language/
│   │   ├── mod.rs
│   │   ├── lexer.rs
│   │   ├── parser.rs
│   │   ├── ast.rs
│   │   └── expand.rs
│   ├── runtime/
│   │   ├── mod.rs
│   │   ├── shell.rs
│   │   ├── executor.rs
│   │   ├── pipeline.rs
│   │   └── redirect.rs
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── builtins.rs
│   │   └── utilities.rs
│   ├── interactive/
│   │   ├── mod.rs
│   │   ├── prompt.rs
│   │   ├── history.rs
│   │   └── completion.rs
│   └── platform/
│       ├── mod.rs
│       └── windows.rs
└── tests/
    ├── lexer.rs
    ├── parser.rs
    ├── expansion.rs
    ├── execution.rs
    └── windows_integration.rs
```

## Language specification: incremental target

### Initial grammar

```text
program     := list?
list        := and_or ((';' | NEWLINE) and_or)*
and_or      := pipeline (('&&' | '||') pipeline)*
pipeline    := command ('|' command)*
command     := word_or_assignment+ redirect*
redirect    := ('<' | '>' | '>>') word
```

This grammar is a planning sketch, not a complete Zsh grammar. The parser should support redirections interspersed with arguments (for example, `echo hi > out.txt extra`) as semantics are implemented. Define operator precedence explicitly: pipelines bind tighter than `&&`/`||`, which bind tighter than command separators. Treat `|` as byte-stream piping.

### Lexer and quoting rules
- Support single quotes (literal content), double quotes (variable expansion, with defined escaping), and unquoted words.
- Preserve empty quoted arguments (`echo ""` must receive an empty argument).
- Recognize operators without surrounding spaces (`echo hi|grep hi>out.txt`).
- Support Windows paths (`C:\\Program Files\\App`) without incorrectly stripping backslashes.
- Provide meaningful errors for unterminated quotes, incomplete pipes and missing redirection targets.
- Do not use `split_whitespace()` as a shell parser.
- Track source spans where practical for diagnostics.

### Expansion rules
- Implement `$NAME`, `${NAME}`, `$?`, and `~` first.
- Separate shell-local variables from exported environment variables; child processes inherit exported values.
- Expansion must honor quoting. Do not use naive string replacement on already-flattened tokens.
- Define word splitting and globbing before adding them; avoid accidental splitting of quoted values.
- Add command substitution (`$(...)`), arrays, and advanced Zsh-like globbing only after core behavior is tested.

## Execution requirements
- Use `std::process::Command` for normal external executable launching; isolate lower-level Windows APIs behind `platform` abstractions.
- Keep the shell's current directory in persistent state. `cd` changes the shell's directory; a child process cannot change its parent shell's directory.
- Resolve native executables using Windows-compatible search rules; explicitly design handling for `PATH`, `PATHEXT`, `.exe`, `.com`, and scripts.
- `.bat` and `.cmd` require explicit, carefully quoted compatibility behavior; do not claim they are native executables.
- Preserve distinct stdin, stdout and stderr streams. Implement `|` for stdout first, then add `2>`, `2>&1`, and `|&` deliberately.
- Apply redirections at the command level, with documented ordering and file truncation/append behavior.
- Launch all stages of a pipeline before waiting to avoid deadlock; close unused pipe handles promptly.
- Wait for/reap all child processes, handle partial spawn failures and return meaningful exit statuses. Initially use the last pipeline stage's status; `pipefail` may be added as an option later.
- Implement `&&`, `||`, and `;` according to exit status.
- For built-ins in pipelines, choose a consistent strategy that does not accidentally mutate parent shell state; document it.
- Interactive Windows sessions own a parent-only `ShellInterruptGuard` from
  `platform::interrupt`. It handles Ctrl+C/Ctrl+Break for K Shell without setting
  the inheritable ignore flag, so foreground children receive Windows' normal
  console broadcast. Keep the callback stateless: no allocation, I/O or locks.
  Remove it on session exit; pass close/logoff/shutdown events to Windows.
  Native `STATUS_CONTROL_C_EXIT` maps to status 130. Forced process-tree cleanup
  and production Windows Job Objects remain separate milestones.
- Native interrupt regression tests create a new hidden console and verify its
  initial PID list before generating a console-wide event. A test-only job owns
  the suspended helper and descendants for timeout cleanup. Never broadcast a
  synthetic Ctrl+C into the user/test runner's existing console. Additional
  `windows-sys` dev feature flags expose these test APIs without a new crate.
- Do not execute user input by passing it wholesale to `cmd /C` or `powershell -Command`.

## Linux-like command compatibility

Prioritize:

| Phase | Commands |
|---|---|
| Core | `pwd`, `cd`, `echo`, `exit`, `clear`, `export`, `unset` |
| Filesystem | `ls`, `mkdir`, `touch`, `cat`, `cp`, `mv`, `rm` |
| Search | `grep`, `find`, `which` |
| Usability | `history`, `alias`, `unalias` |

Guidelines:
- Match common Unix flags (`ls -la`, `mkdir -p`, `cp -r`, `rm -r`, `grep -in`) where explicitly implemented; reject unsupported flags with a useful error instead of silently ignoring them.
- Prefer `std::path::Path`/`PathBuf` and `OsStr`/`OsString` for Windows filesystem and process boundaries.
- Support `~`, `./`, `../`, `D:/Projects`, and native `D:\\Projects` paths.
- `/c/Users/...` -> `C:\\Users\\...` translation is an optional documented compatibility feature, not a Windows default.
- Windows hidden attributes, permissions, symlinks and executable bits differ from Unix; do not fabricate POSIX metadata.
- Avoid destructive defaults: `rm -rf` must be scoped to explicit targets; guard dangerous root/home/system-directory cases and test them.
- File operations live in `commands::files`: `delete` aliases `rm`, `copy` aliases
  `cp`, `move` aliases `mv`, and `update` aliases `rename` (same-parent leaf name).
  Reuse their pure parser for suggestion validation. Copy/move take one source
  and one destination, reject overwriting and require existing destination parents.
  Copy/remove preflight recursive entries and never descend into reparse points;
  copies reject them, removals unlink leaves. Destructive source guards protect
  roots, home, cwd/ancestors and system/program trees. Native no-replace renaming,
  no-follow copy-source opens, Win32 filename checks and leaf removal belong in
  `platform::filesystem`. Cross-volume moves finish copying before removing only
  the planned source entries. Failures may leave partial output/cleanup; operations
  do not promise transactions or protection from concurrent filesystem mutation.
- `create [--] file ...` uses that module's pure parser, path checks and exclusive
  file creation. Preflight all operands, require existing parents and reject
  existing entries; never truncate them or update their timestamps. `touch`
  retains its separate create-or-update-time behavior.
- Text commands live in `commands::text`: `write [--] file text ...` replaces
  contents, `append` appends; both create missing files and add a UTF-8 LF line.
  Parse only the filename as a path; trailing words (including flags/empty words)
  are text. Reuse the pure parser for hints and `files::resolve` for checked paths.
  `platform::filesystem::open_text_target` opens without truncation, with Windows
  no-follow flags, then verifies the handle before commands may truncate/write.
  Completion must stop offering paths after the filename. No redirection syntax
  or transactional-write guarantee is introduced by these commands.
- Internal utilities must write through provided streams, not directly to `println!`, so piping and redirection work consistently.
- Prefer shared command interfaces over launching a new shell process for each utility unless there is a measured reason.

## Interactive experience
- Current implementation uses `rustyline` with its `with-file-history` and
  `custom-bindings` features. The latter uses rustyline's radix-trie dependency
  to override Ctrl+L; do not introduce a second editor for screen handling.
  Keep interactive editing in `src/interactive`; non-interactive execution must
  bypass the editor. Completion uses the language lexer's incomplete-input mode,
  while execution always uses strict lexing. Do not add a second line editor.
- Interactive output passes explicit `OutputFormat` capabilities to commands;
  the default runtime API produces plain output. Screen commands return a
  `ShellAction` that the interactive host applies through `platform::console`,
  rather than writing control sequences into utility streams. `clear` erases
  viewport AND scrollback with VT `H`, `2J`, `3J`, re-enabling Windows VT mode
  after foreground children if needed. Fall back to the editor's native clear
  when VT is unavailable. Ctrl+L performs that terminal clear before returning
  the editor's `Cmd::ClearScreen` to preserve input/cursor and refresh its layout.
  `NO_COLOR` disables color, not screen-control sequences. Plain/batch execution
  must remain escape-free and must not clear the caller's terminal.
- Render the current-directory prompt in `interactive::prompt`, independently of
  execution. The interactive host owns a native console-title guard for K Shell;
  batch and `-c` modes must never change titles or emit prompts.
- Foreground colors are shared in `presentation::Style`. Keep the prompt's raw
  and styled text visually identical so multiline editing stays aligned. Respect
  `NO_COLOR` and keep redirected utility output free of ANSI sequences.
  Indicate prompt failure with the marker color only; never print numeric exit
  codes in the prompt. Preserve actual exit statuses in the runtime.
- Appearance settings live in `presentation::Appearance`, owned by `ShellState`
  and passed explicitly in `OutputFormat` by the host. `theme` changes session
  palette/icon choices; interactive startup reads `KSHELL_THEME`/`KSHELL_ICONS`.
  Standard Unicode icons are the default; Nerd Font glyphs are opt-in. Decorate
  terminal labels only, outside filename quoting, and include icons in Unicode
  cell-width calculations. Do not add icons to completion replacements, native
  command arguments or plain output. Keep prompt visible text identical with
  and without styling for every palette/icon mode. No font installation or
  settings-file persistence is performed by `theme`.
  Midnight is an opt-in palette coordinated with the Windows Terminal fragment
  installer in `contrib/windows-terminal`. The CLI initializes interactive
  appearance from startup environment settings, then applies explicit
  `--theme`/`--icons`/`--suggestions` flags before handing state to the host.
  Fonts/background/padding belong to Terminal profiles, never shell ANSI output.
  The installer adds only its own backed-up fragment, with no font installation,
  global default-profile change or general-purpose shell wrapper for K Shell.
- Implement persistent history and up/down navigation.
- `history [count]` renders a read-only snapshot of the editor's accepted entries,
  including saved history loaded at interactive startup. Refresh that bounded
  snapshot in `ShellState` after editor insertion; do not create another history
  store, reparse the history file or record ignored leading-space input. Keep its
  pure count parser shared with suggestion validation. Render through supplied
  streams with safe escaped controls and original retained-list numbering.
  Noninteractive execution never loads user history; no clear/replay action is
  implied by the listing command.
- Add Tab completion for commands, filesystem paths and environment variables.
- Keep completion providers independent and non-blocking where practical.
- Interactive suggestion highlighting belongs in `interactive::highlight`. Use the
  shared parser on typed text plus suggestion and preserve visible text;
  do not reparse with whitespace splitting or execute input during highlighting.
  Built-ins are matched exactly as dispatch does, and bare native/adapted names
  use completion's cached snapshot (startup/after cd), without per-key PATH scans
  or discovery subprocesses. Explicit paths use the native resolver. Known
  suggestions use `Style::Success` for supported valid syntax/remembered success,
  `Style::Error` for known invalid syntax/remembered failure and `Style::Muted`
  for unverified external arguments. Color the WHOLE suffix, including flags,
  rather than declaring an invalid Docker command green because docker exists.
  Typed letters retain their normal foreground. `interactive::validation` must
  remain side-effect-free: reuse utility/tree parsers; do not run built-ins or
  arbitrary commands to validate a preview. Docker's profile covers global
  options before the subcommand, not every plugin/subcommand schema. Record
  actual results after execution in a bounded (200), session-local cache keyed
  by cwd + parsed arguments; status 130 is not a new failure, and syntax errors
  override cached success. Never persist these results in the history file.
  Hinter captures suffix classification for highlight_hint
  in session-local state; clear it when a hint disappears or cwd changes, and
  verify the suffix matches before applying it. Never color a stale hint with
  another line's status. Host terminal/NO_COLOR capabilities control highlighting.
  Repaint on edits, deletions and final refresh. Local syntax/result checks do
  not guarantee runtime filesystem permissions, network reachability or server
  state. Full runtime validation remains the executor's responsibility.
- Suggestion appearance is `Appearance::suggestions` (`soft`, `bold`, `underline`),
  set through `theme suggestions` or `KSHELL_SUGGESTION_STYLE` at startup. Render
  status through `Appearance::paint_suggestion`; do not change validation to
  implement a visual preference. Soft uses explicit regular-weight preview RGB,
  bold preserves the original status palette, and underline uses soft colors.
  Leave completion's trailing whitespace outside the underline without removing
  it from accepted input. NO_COLOR disables every suggestion decoration.
- Add inline suggestions, syntax highlighting and Git-aware prompt after execution semantics are stable.
- Configuration should live in an appropriate per-user location (for example `%APPDATA%\\kshell\\config.toml`).
- Keep prompt rendering independent from command execution.
- Ensure redirected/non-interactive output is not polluted with ANSI color codes or prompt text.

## Error handling and security
- No panics or `unwrap()` on normal user input or recoverable I/O failures.
- Prefer structured errors with context and stable exit codes.
- Never concatenate untrusted arguments into a `cmd.exe` or PowerShell command line.
- Treat shell scripts, command substitution and plugins as code execution features; document their trust boundaries.
- Avoid logging secrets or exporting sensitive variables accidentally.
- Avoid following symlinks recursively without an explicit policy.
- `tree` uses `symlink_metadata` and treats symlinks and Windows reparse points
  (including junctions) as leaves; it must not descend through those entries.
- Preserve filenames that are not valid UTF-8 when interacting with Windows OS interfaces; avoid lossy conversion at process/filesystem boundaries.
- Never delete or overwrite user data during tests. Use temporary directories.

## Code quality
- Use `cargo fmt` and `cargo clippy --all-targets -- -D warnings` when feasible.
- Use small, cohesive modules and idiomatic Rust types.
- Prefer explicit enums for AST nodes, redirects and execution outcomes.
- Keep parser code deterministic and free of side effects.
- Avoid global mutable state; pass `ShellState` and stream handles explicitly.
- Write documentation for non-obvious Windows behavior and grammar decisions.
- Add tests with each new feature and update README examples to reflect reality.

## Testing matrix

### Lexer/parser
- `git status`
- `echo "hello world"`
- `echo ""`
- `echo 'literal $HOME'`
- `echo hi|grep hi>out.txt`
- `echo "unterminated` (error)
- `git status |` (error)
- `echo >` (error)
- Windows path containing spaces/backslashes
- Operator precedence: `a | b && c; d`

### Expansion
- `$NAME`, `${NAME}`, `$?`, `~`
- Single-quoted text stays literal
- Double-quoted expansion stays one argument
- Unset variable behavior is documented
- Exported values are inherited by children

### Execution
- Run a native `.exe` with arguments containing spaces
- `cd` persists between commands
- `echo hello > file.txt`, `echo world >> file.txt`
- Input redirection and multi-stage pipelines
- A pipeline with output larger than the OS pipe buffer
- Nonzero exit statuses with `&&` and `||`
- Missing executable, denied access, malformed redirection
- Cleanup after a process spawn failure
- Ctrl+C and interrupted pipelines (when supported)

### Windows
- Drive-letter paths and UNC paths
- Executable lookup through `PATH`/`PATHEXT`
- Unicode filenames
- Quoted arguments with spaces and backslashes
- `.cmd`/`.bat` compatibility policy
- Console control and child-process cleanup

## Agent workflow
1. Inspect repository structure, Cargo manifest, README and relevant code before proposing edits.
2. Summarize current behavior and identify the smallest coherent change.
3. Implement one feature end-to-end across lexer/parser/AST/runtime where necessary.
4. Add unit and integration tests, especially for edge cases and Windows behavior.
5. Run `cargo fmt --check`, `cargo test`, and `cargo clippy --all-targets -- -D warnings` where the environment permits.
6. Report exact commands run, results, and anything unverified (particularly Windows-specific behavior when working on Linux).
7. Update README, examples and this file when architectural decisions change.
8. Do not claim functionality is complete without tests or a reproducible demonstration.

## Development phases

### Phase 1 — Correct core
- Lexer, parser, AST, stateful built-ins, native executable execution.
- Quoting, basic variables, pipes, redirects, status codes, tests.

### Phase 2 — Linux command layer
- Filesystem commands and search utilities.
- Unix-style flags, path translation policy, safe deletion semantics.

### Phase 3 — Interactive UX
- History, completion, highlighting, inline suggestions, themes, Git prompt.

### Phase 4 — Shell language
- Aliases, functions, scripting, command substitution, globbing, control flow.

### Phase 5 — Windows hardening
- Ctrl+C, console control, Job Objects, batch-script compatibility, packaging.

## Definition of done for each feature
- Behavior documented with examples.
- Unit tests and, where appropriate, Windows integration tests pass.
- No regressions to existing syntax or execution behavior.
- Errors are actionable and do not crash the shell.
- Windows paths, quoting and streams have been considered.
- Implementation status is accurately reflected in README.

## First task for a coding agent
Inspect the current codebase and produce a concise gap analysis against this document. Then implement the highest-impact missing correctness feature in a small, tested change. Prioritize lexer/quote correctness and stream-safe pipeline execution before adding new UI features. Do not rewrite the entire repository or assert unverified compatibility.
