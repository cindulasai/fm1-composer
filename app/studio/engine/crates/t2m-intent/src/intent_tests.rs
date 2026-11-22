use super::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

struct Scripted {
    replies: RefCell<Vec<Result<String, IntentError>>>,
    seen: RefCell<Vec<String>>,
}

impl Scripted {
    fn new(replies: Vec<Result<String, IntentError>>) -> Scripted {
        Scripted {
            replies: RefCell::new(replies),
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl Complete for Scripted {
    fn complete(&self, _system: &str, user: &str) -> Result<String, IntentError> {
        self.seen.borrow_mut().push(user.to_string());
        let mut replies = self.replies.borrow_mut();
        if replies.is_empty() {
            return Err(IntentError::Network("no more scripted replies".into()));
        }
        replies.remove(0)
    }
}

fn kb() -> Knowledge {
    Knowledge::load().unwrap()
}

fn good_json() -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../suite/briefs/hiphop-lofi-1.json"),
    )
    .unwrap()
}

#[test]
fn t38_1_extract_json_finds_the_object_among_chatter_fences_and_thinking() {
    assert_eq!(extract_json("{\"a\":1}"), Some("{\"a\":1}"));
    assert_eq!(
        extract_json("Sure! ```json\n{\"a\":{\"b\":2}}\n``` hope it helps"),
        Some("{\"a\":{\"b\":2}}")
    );
    assert_eq!(
        extract_json("<think>maybe {not} this</think>{\"ok\":true}"),
        Some("{\"ok\":true}")
    );
    assert_eq!(
        extract_json("{\"s\":\"a } brace \\\" quote\"} trailing"),
        Some("{\"s\":\"a } brace \\\" quote\"}")
    );
    for none in ["", "no braces", "{unclosed", "}{", "<think>{\"a\":1}"] {
        let got = extract_json(none);
        assert!(
            got.is_none() || none.starts_with("<think>"),
            "{none:?} -> {got:?}"
        );
    }
}

#[test]
fn t38_2_a_reply_is_read_and_service_errors_are_reported() {
    let ok = r#"{"choices":[{"message":{"content":"hello"}}]}"#;
    assert_eq!(parse_response(ok).unwrap(), "hello");
    for bad in [
        "",
        "not json",
        "{}",
        r#"{"choices":[]}"#,
        r#"{"choices":[{"message":{"content":"  "}}]}"#,
        r#"{"choices":[{"message":{"content":5}}]}"#,
    ] {
        assert!(
            matches!(parse_response(bad), Err(IntentError::BadAnswer(_))),
            "{bad:?}"
        );
    }
    let e = parse_response(r#"{"error":{"message":"invalid api key"}}"#).unwrap_err();
    assert_eq!(e, IntentError::Service("invalid api key".into()));
    let e = parse_response(
        r#"{"base_resp":{"status_code":1004,"status_msg":"auth failed"},"choices":[]}"#,
    )
    .unwrap_err();
    assert_eq!(e, IntentError::Service("1004: auth failed".into()));
    let long = format!(r#"{{"error":{{"message":"{}"}}}}"#, "x".repeat(1_000));
    match parse_response(&long).unwrap_err() {
        IntentError::Service(m) => assert_eq!(m.len(), 300),
        other => panic!("{other:?}"),
    }
}

#[test]
fn t38_3_settings_come_from_the_environment_then_the_private_file_and_stay_hidden() {
    let home = Path::new("/home/x");
    let cwd = Path::new("/work");
    let no_env = |_: &str| None;
    let file = |p: &Path| {
        (p == Path::new("/home/x/.config/text2midi/minimax.key"))
            .then(|| " sk-file \nignored".to_string())
    };
    let c = Config::load(&no_env, &file, home, cwd).unwrap();
    assert_eq!(
        (c.key.as_str(), c.base_url.as_str(), c.model.as_str()),
        ("sk-file", DEFAULT_BASE_URL, DEFAULT_MODEL)
    );
    assert_eq!(c.chat_url(), "https://api.minimax.io/v1/chat/completions");
    // The environment wins over the file; overrides apply and a trailing slash is dropped.
    let env = |k: &str| match k {
        "MINIMAX_API_KEY" => Some("sk-env".to_string()),
        "MINIMAX_BASE_URL" => Some("https://api.minimaxi.com/v1/".to_string()),
        "MINIMAX_MODEL" => Some("MiniMax-M3".to_string()),
        _ => None,
    };
    let c = Config::load(&env, &file, home, cwd).unwrap();
    assert_eq!(
        (c.key.as_str(), c.base_url.as_str(), c.model.as_str()),
        ("sk-env", "https://api.minimaxi.com/v1", "MiniMax-M3")
    );
    assert!(
        !format!("{c:?}").contains("sk-env"),
        "the key leaked into Debug output"
    );
    // Nothing usable: no key, blank, spaces inside, control characters, huge, plain http.
    let none_file = |_: &Path| None;
    assert!(Config::load(&no_env, &none_file, home, cwd).is_none());
    for key in ["", "   ", "a b", "a\u{0}b", &"k".repeat(513), "\n"] {
        let env = |k: &str| (k == "MINIMAX_API_KEY").then(|| key.to_string());
        assert!(
            Config::load(&env, &none_file, home, cwd).is_none(),
            "{key:?}"
        );
    }
    let http = |k: &str| match k {
        "MINIMAX_API_KEY" => Some("sk".to_string()),
        "MINIMAX_BASE_URL" => Some("http://evil.example/v1".to_string()),
        _ => None,
    };
    assert!(
        Config::load(&http, &none_file, home, cwd).is_none(),
        "a key must never be sent over plain http"
    );
}

#[test]
fn t38_4_the_request_names_the_model_and_the_prompt_lists_what_the_model_may_use() {
    let body = build_request("m-1", "SYS", "USER");
    assert_eq!(body["model"], "m-1");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], "SYS");
    assert_eq!(body["messages"][1]["content"], "USER");
    let prompt = system_prompt(&kb());
    for id in ["hiphop.lofi", "jazz.swing", "pop.generic", "groove."] {
        assert!(prompt.contains(id), "{id} missing from the prompt");
    }
    assert!(prompt.contains("\"version\""), "the schema is missing");
    assert!(prompt.contains('\n'), "line breaks must be real");
    assert!(
        !prompt.contains("\\n"),
        "line breaks must not be literal backslash-n"
    );
}

#[test]
fn t38_5_a_good_answer_becomes_a_checked_brief_and_a_bad_one_gets_one_more_try() {
    let kb = kb();
    let fenced = format!(
        "<think>hmm</think>Here you go:\n```json\n{}\n```",
        good_json()
    );
    let model = Scripted::new(vec![Ok(fenced)]);
    let brief = brief_from_prompt(&model, &kb, "  lofi for studying  ").unwrap();
    assert_eq!(brief.identity.style, "hiphop.lofi");
    assert_eq!(model.seen.borrow().as_slice(), ["lofi for studying"]);
    // First answer unusable, second good: the model is told what was wrong.
    let model = Scripted::new(vec![Ok("I cannot do that".into()), Ok(good_json())]);
    assert!(brief_from_prompt(&model, &kb, "lofi").is_ok());
    let seen = model.seen.borrow();
    assert_eq!(seen.len(), 2);
    assert!(seen[1].contains("could not be used") && seen[1].starts_with("lofi"));
    drop(seen);
    // Two bad answers: a clear error, and no third call.
    let model = Scripted::new(vec![
        Ok("{\"nonsense\":1}".into()),
        Ok("still no".into()),
        Ok(good_json()),
    ]);
    assert!(matches!(
        brief_from_prompt(&model, &kb, "lofi"),
        Err(IntentError::BadAnswer(_))
    ));
    assert_eq!(model.seen.borrow().len(), 2);
}

#[test]
fn t38_6_bad_input_and_failures_are_clean_errors_that_never_call_or_retry_wrongly() {
    let kb = kb();
    let never = Scripted::new(vec![]);
    for blank in ["", "  ", "\n\t"] {
        assert_eq!(
            brief_from_prompt(&never, &kb, blank).unwrap_err(),
            IntentError::Empty
        );
    }
    let long = "x".repeat(MAX_TEXT_CHARS + 1);
    assert_eq!(
        brief_from_prompt(&never, &kb, &long).unwrap_err(),
        IntentError::TooLong
    );
    assert!(
        never.seen.borrow().is_empty(),
        "a refused description must not be sent"
    );
    // Exactly at the limit is fine to send.
    let at_limit = "x".repeat(MAX_TEXT_CHARS);
    let model = Scripted::new(vec![Ok(good_json())]);
    assert!(brief_from_prompt(&model, &kb, &at_limit).is_ok());
    // A network or service failure is passed on at once, without a second attempt.
    for failure in [
        IntentError::Network("down".into()),
        IntentError::Service("quota".into()),
    ] {
        let model = Scripted::new(vec![Err(failure.clone()), Ok(good_json())]);
        assert_eq!(brief_from_prompt(&model, &kb, "lofi").unwrap_err(), failure);
        assert_eq!(model.seen.borrow().len(), 1);
    }
    // A JSON object that is not a brief at all (wrong types) is refused after the retry.
    let model = Scripted::new(vec![
        Ok("{\"version\":\"one\"}".into()),
        Ok("{\"version\":[]}".into()),
    ]);
    assert!(matches!(
        brief_from_prompt(&model, &kb, "lofi"),
        Err(IntentError::BadAnswer(_))
    ));
}

#[test]
fn t38_7_dotenv_files_are_read_with_the_right_precedence_and_comments_are_ignored() {
    let home = Path::new("/home/x");
    let cwd = Path::new("/work");
    let no_env = |_: &str| None;
    let files = |cwd_env: Option<&'static str>,
                 home_env: Option<&'static str>,
                 key_file: Option<&'static str>| {
        move |p: &Path| {
            if p == Path::new("/work/.env") {
                cwd_env.map(String::from)
            } else if p == Path::new("/home/x/.env") {
                home_env.map(String::from)
            } else if p == Path::new("/home/x/.config/text2midi/minimax.key") {
                key_file.map(String::from)
            } else {
                None
            }
        }
    };
    let text = "# MINIMAX_API_KEY=commented\n\nOTHER=1\nexport MINIMAX_API_KEY=\"sk-quoted\"\nMINIMAX_BASE_URL='https://api.minimaxi.com/v1'\nMINIMAX_MODEL=MiniMax-M3\n";
    let c = Config::load(&no_env, &files(Some(text), None, None), home, cwd).unwrap();
    assert_eq!(
        (c.key.as_str(), c.base_url.as_str(), c.model.as_str()),
        ("sk-quoted", "https://api.minimaxi.com/v1", "MiniMax-M3")
    );
    // Only commented or other names: no key.
    assert!(
        Config::load(
            &no_env,
            &files(
                Some("# MINIMAX_API_KEY=x\nOTHER=1\nMINIMAX_API_KEY_X=y"),
                None,
                None
            ),
            home,
            cwd
        )
        .is_none()
    );
    // The project's .env beats the home .env; the private key file beats both; the environment beats all.
    let both = files(
        Some("MINIMAX_API_KEY=sk-cwd"),
        Some("MINIMAX_API_KEY=sk-home"),
        None,
    );
    assert_eq!(
        Config::load(&no_env, &both, home, cwd).unwrap().key,
        "sk-cwd"
    );
    let only_home = files(None, Some("MINIMAX_API_KEY=sk-home"), None);
    assert_eq!(
        Config::load(&no_env, &only_home, home, cwd).unwrap().key,
        "sk-home"
    );
    let with_file = files(Some("MINIMAX_API_KEY=sk-cwd"), None, Some("sk-file"));
    assert_eq!(
        Config::load(&no_env, &with_file, home, cwd).unwrap().key,
        "sk-file"
    );
    let env = |k: &str| (k == "MINIMAX_API_KEY").then(|| "sk-env".to_string());
    assert_eq!(
        Config::load(&env, &with_file, home, cwd).unwrap().key,
        "sk-env"
    );
    // A plain-http address in a .env file is refused like any other; a key with a space too.
    let http = files(
        Some("MINIMAX_API_KEY=sk\nMINIMAX_BASE_URL=http://evil/v1"),
        None,
        None,
    );
    assert!(Config::load(&no_env, &http, home, cwd).is_none());
    let spaced = files(Some("MINIMAX_API_KEY=a b"), None, None);
    assert!(Config::load(&no_env, &spaced, home, cwd).is_none());
}

#[test]
fn t42_1_the_nearest_dotenv_above_the_working_folder_is_used_and_never_one_outside_home() {
    let home = Path::new("/home/x");
    let no_env = |_: &str| None;
    // Compared as text with `/`, since `Path::join` writes `\` on Windows.
    let files = |p: &Path| match p.to_str().unwrap().replace('\\', "/").as_str() {
        "/home/x/proj/.env" => Some("MINIMAX_API_KEY=sk-proj".to_string()),
        "/home/x/.env" => Some("MINIMAX_API_KEY=sk-home".to_string()),
        "/.env" | "/home/.env" => Some("MINIMAX_API_KEY=sk-outside".to_string()),
        _ => None,
    };
    let deep = Path::new("/home/x/proj/app/src-tauri");
    assert_eq!(
        Config::load(&no_env, &files, home, deep).unwrap().key,
        "sk-proj"
    );
    let elsewhere = Path::new("/home/x/other/deeper");
    assert_eq!(
        Config::load(&no_env, &files, home, elsewhere).unwrap().key,
        "sk-home"
    );
    // A working folder outside home must not pick up `.env` files from the root or `/home`.
    let outside = Path::new("/srv/app");
    assert_eq!(
        Config::load(&no_env, &files, home, outside).unwrap().key,
        "sk-home"
    );
    let nothing =
        |p: &Path| (p == Path::new("/.env")).then(|| "MINIMAX_API_KEY=sk-outside".to_string());
    assert!(Config::load(&no_env, &nothing, home, outside).is_none());
}
