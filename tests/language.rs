use kshell::language::{
    ast::Quote,
    lexer::{TokenKind, lex},
    parser::parse,
};

fn words(input: &str) -> Vec<String> {
    parse(input)
        .unwrap()
        .unwrap()
        .words
        .iter()
        .map(|word| word.literal())
        .collect()
}

#[test]
fn quotes_preserve_spaces_empty_arguments_and_adjacent_parts() {
    assert_eq!(
        words("echo \"hello world\" \"\" '' pre\"mid dle\"post"),
        ["echo", "hello world", "", "", "premid dlepost"]
    );
    assert_eq!(words("echo 'literal $HOME'"), ["echo", "literal $HOME"]);
    assert_eq!(words("echo 'a\"b' \"a'b\""), ["echo", "a\"b", "a'b"]);
}

#[test]
fn quote_context_is_retained_for_future_expansion() {
    let command = parse("echo pre'$HOME'\"$USER\"").unwrap().unwrap();
    let parts = &command.words[1].parts;
    assert_eq!(
        parts.iter().map(|part| part.quote).collect::<Vec<_>>(),
        [Quote::Unquoted, Quote::Single, Quote::Double]
    );
}

#[test]
fn native_windows_backslashes_and_unicode_are_literal() {
    assert_eq!(
        words(r#"echo C:\Projects\App "C:\Program Files\App\" '\\server\share\' ឯកសារ"#),
        [
            "echo",
            r"C:\Projects\App",
            "C:\\Program Files\\App\\",
            r"\\server\share\",
            "ឯកសារ"
        ]
    );
}

#[test]
fn adjacent_operators_have_exact_spans() {
    let tokens = lex("echo hi|grep hi>out.txt").unwrap();
    assert_eq!(tokens[2].kind, TokenKind::Operator("|".into()));
    assert_eq!(tokens[2].span, 7..8);
    assert_eq!(tokens[5].kind, TokenKind::Operator(">".into()));
    assert_eq!(tokens[5].span, 15..16);
    assert_eq!(
        &"echo hi|grep hi>out.txt"[tokens[6].span.clone()],
        "out.txt"
    );
}

#[test]
fn all_unimplemented_operators_are_rejected_before_execution() {
    for input in [
        "echo hi|grep hi",
        "git status |",
        "echo >",
        "echo >>out",
        "a&&b",
        "a||b",
        "a;b",
        "a&b",
        "a<b",
        "a\nb",
        "a\rb",
    ] {
        let error = parse(input).unwrap_err();
        assert_eq!(error.status, 2, "{input}");
        assert!(error.message.contains("not supported yet"), "{input}");
        assert!(error.span.is_some());
    }
    assert_eq!(words("echo '|' \"&&\" ';'"), ["echo", "|", "&&", ";"]);
}

#[test]
fn unterminated_quote_reports_its_utf8_byte_span() {
    let error = lex("echo ឯក 'unfinished").unwrap_err();
    let input = "echo ឯក 'unfinished";
    assert_eq!(error.span, Some(input.find('\'').unwrap()..input.len()));
    assert_eq!(error.status, 2);
    assert!(error.message.contains("unterminated quote"));
    assert!(lex("echo \"unterminated").is_err());
}

#[test]
fn blank_input_is_no_command() {
    assert!(parse(" \t ").unwrap().is_none());
}
