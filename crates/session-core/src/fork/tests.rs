use super::*;
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("asv-fork-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn transcript(&self, rows: &[Value]) -> PathBuf {
        let path = self.0.join("original.jsonl");
        fs::write(&path, jsonl(rows)).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn records(rows: Vec<Value>) -> Vec<Record> {
    rows.into_iter()
        .enumerate()
        .map(|(line, value)| Record { line, value })
        .collect()
}

#[test]
fn claude_fork_keeps_complete_tool_round_and_registers_new_identity() {
    let fixture = Fixture::new();
    let rows = vec![
        json!({"type":"user","uuid":"u1","parentUuid":null,"sessionId":"old","cwd":"/project","message":{"role":"user","content":"question"}}),
        json!({"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"old","message":{"role":"assistant","content":[{"type":"tool_use","id":"call","name":"Read","input":{}}]}}),
        json!({"type":"user","uuid":"tool","parentUuid":"a1","sessionId":"old","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call","content":"result"}]}}),
        json!({"type":"assistant","uuid":"a2","parentUuid":"tool","sessionId":"old","message":{"role":"assistant","content":"final answer"}}),
        json!({"type":"user","uuid":"u2","parentUuid":"a2","sessionId":"old","message":{"role":"user","content":"future question"}}),
    ];
    let path = fixture.transcript(&rows);
    let before = fs::read(&path).unwrap();
    fs::write(
        fixture.0.join("sessions-index.json"),
        json!({"version":1,"entries":[{"sessionId":"old"}],"customField":"preserve"}).to_string(),
    )
    .unwrap();
    let parsed = read_records(&path).unwrap();
    assert!(target_index(SessionSourceKind::Claude, &parsed, "tool").is_err());
    let fork = fork_files(SessionSourceKind::Claude, &path, &parsed, 0).unwrap();
    assert_ne!(fork.new_session_id, "old");
    assert_eq!(fs::read(&path).unwrap(), before);
    let history = read_records(Path::new(&fork.new_file_path)).unwrap();
    assert_eq!(history.len(), 4);
    assert_eq!(history[3].value["uuid"], "a2");
    assert!(history
        .iter()
        .all(|r| string(&r.value, "sessionId") == Some(&fork.new_session_id)));
    let index: Value =
        serde_json::from_slice(&fs::read(fixture.0.join("sessions-index.json")).unwrap()).unwrap();
    assert_eq!(index["customField"], "preserve");
    assert_eq!(index["entries"].as_array().unwrap().len(), 2);
    assert_eq!(index["entries"][1]["sessionId"], fork.new_session_id);
}

#[test]
fn omp_fork_selects_ancestry_and_keeps_profile_directory_and_artifacts() {
    let fixture = Fixture::new();
    let rows = vec![
        json!({"type":"session","id":"old","version":3,"cwd":"/project","timestamp":"2026-09-12T00:00:00Z"}),
        json!({"type":"model_change","id":"model","parentId":null,"modelId":"chosen"}),
        json!({"type":"message","id":"root","parentId":"model","message":{"role":"user","content":"root question"}}),
        json!({"type":"message","id":"root-reply","parentId":"root","message":{"role":"assistant","content":"root answer"}}),
        json!({"type":"message","id":"sibling","parentId":"root-reply","message":{"role":"user","content":"other branch"}}),
        json!({"type":"message","id":"sibling-reply","parentId":"sibling","message":{"role":"assistant","content":"not inherited"}}),
        json!({"type":"message","id":"selected","parentId":"root-reply","message":{"role":"user","content":"selected branch"}}),
        json!({"type":"message","id":"reply","parentId":"selected","message":{"role":"assistant","content":"branch answer"}}),
        json!({"type":"message","id":"future","parentId":"reply","message":{"role":"user","content":"future"}}),
    ];
    let path = fixture.transcript(&rows);
    let artifact_dir = path.with_extension("");
    fs::create_dir(&artifact_dir).unwrap();
    fs::write(artifact_dir.join("result.txt"), "tool artifact").unwrap();
    let fork = fork_files(
        SessionSourceKind::Omp,
        &path,
        &read_records(&path).unwrap(),
        6,
    )
    .unwrap();
    let new_path = Path::new(&fork.new_file_path);
    assert_eq!(new_path.parent(), path.parent());
    assert_eq!(
        fs::read_to_string(new_path.with_extension("").join("result.txt")).unwrap(),
        "tool artifact"
    );
    let history = read_records(new_path).unwrap();
    let ids: Vec<_> = history
        .iter()
        .filter_map(|r| string(&r.value, "id"))
        .collect();
    assert_eq!(
        &ids[1..],
        &["model", "root", "root-reply", "selected", "reply"]
    );
    assert_eq!(history[0].value["parentSession"], "old");
    let parsed = omp::parse_all_messages(new_path).unwrap();
    assert_eq!(parsed.last().unwrap().uuid.as_deref(), Some("reply"));
    assert_eq!(fs::read(&path).unwrap(), jsonl(&rows));
}

#[test]
fn grok_fork_keeps_raw_tools_context_and_rewrites_summary() {
    let fixture = Fixture::new();
    let original = fixture.0.join("old");
    fs::create_dir(&original).unwrap();
    let path = original.join("chat_history.jsonl");
    let rows = vec![
        json!({"type":"system","content":"system context"}),
        json!({"type":"user","content":"question"}),
        json!({"type":"reasoning","summary":[{"text":"reasoning"}]}),
        json!({"type":"tool_call","id":"call","arguments":{"path":"file"}}),
        json!({"type":"tool_result","id":"call","output":"raw result"}),
        json!({"type":"user","synthetic_reason":"system_reminder","content":"context reminder"}),
        json!({"type":"assistant","content":"final answer"}),
        json!({"type":"user","content":"future"}),
    ];
    fs::write(&path, jsonl(&rows)).unwrap();
    let summary = json!({"info":{"id":"old","cwd":"/project"},"session_summary":"future summary","request_id":"old-request","next_trace_turn":100,"chat_format_version":1,"customField":true});
    fs::write(original.join("summary.json"), summary.to_string()).unwrap();
    fs::write(original.join("prompt_context.json"), "{\"version\":1}").unwrap();
    fs::write(original.join("system_prompt.txt"), "system prompt").unwrap();
    let parsed = read_records(&path).unwrap();
    let id = line_message_id(1, &rows[1]);
    let index = target_index(SessionSourceKind::Grok, &parsed, &id).unwrap();
    let fork = fork_files(SessionSourceKind::Grok, &path, &parsed, index).unwrap();
    let new_path = Path::new(&fork.new_file_path);
    let new_parent = new_path.parent().unwrap();
    assert_eq!(fs::read(new_path).unwrap(), jsonl(&rows[..7]));
    assert_eq!(fs::read(&path).unwrap(), jsonl(&rows));
    assert_eq!(
        fs::read_to_string(new_parent.join("system_prompt.txt")).unwrap(),
        "system prompt"
    );
    let new_summary: Value =
        serde_json::from_slice(&fs::read(new_parent.join("summary.json")).unwrap()).unwrap();
    assert_eq!(new_summary["info"]["id"], fork.new_session_id);
    assert_eq!(new_summary["session_summary"], "");
    assert_ne!(new_summary["request_id"], "old-request");
    assert_eq!(new_summary["customField"], true);
    assert_eq!(
        grok::extract_session_meta(new_path).unwrap().id,
        fork.new_session_id
    );
    assert_eq!(
        grok::parse_all_messages(new_path)
            .unwrap()
            .last()
            .unwrap()
            .role,
        "assistant"
    );
    // A non-file context cannot leave a half-created, discoverable session.
    fs::remove_file(original.join("prompt_context.json")).unwrap();
    fs::create_dir(original.join("prompt_context.json")).unwrap();
    let count = fs::read_dir(&fixture.0).unwrap().count();
    assert!(fork_files(SessionSourceKind::Grok, &path, &parsed, index).is_err());
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), count);
}

#[test]
fn stale_selection_and_missing_ancestry_fail_without_creating_files() {
    let row = json!({"type":"user","content":"before"});
    let old_id = line_message_id(2, &row);
    let changed = vec![Record {
        line: 2,
        value: json!({"type":"user","content":"after"}),
    }];
    assert!(target_index(SessionSourceKind::Grok, &changed, &old_id).is_err());
    let missing = records(vec![
        json!({"type":"message","id":"u","parentId":"missing","message":{"role":"user","content":"question"}}),
    ]);
    assert!(history_through_round(SessionSourceKind::Omp, &missing, 0).is_err());
    let fixture = Fixture::new();
    let path = fixture.transcript(&[row]);
    assert!(validate_session_file("grok", path.to_str().unwrap()).is_err());
}

#[test]
fn codex_native_turn_ids_disambiguate_repeated_and_rolled_back_prompts() {
    let user = json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"again"}]}});
    let rows = records(vec![
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"removed"}}),
        user.clone(),
        json!({"type":"event_msg","payload":{"type":"thread_rolled_back","num_turns":1}}),
        json!({"type":"turn_context","payload":{"turn_id":"current"}}),
        user,
    ]);
    let turns = vec![
        json!({"id":"current","items":[{"type":"userMessage","content":[{"type":"text","text":"again"}]}]}),
    ];
    assert_eq!(codex_turn_index(&rows, 4, &turns).unwrap(), 0);
    assert!(codex_turn_index(&rows, 1, &turns).is_err());
}

#[test]
fn codex_pagination_has_stable_targets_and_hides_rolled_back_turns() {
    let fixture = Fixture::new();
    let mut rows = Vec::new();
    for number in 0..3 {
        rows.push(json!({"type":"event_msg","payload":{"type":"task_started","turn_id":format!("t{number}")}}));
        rows.push(json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":format!("question {number}")}]}}));
        rows.push(json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":format!("answer {number}")}]}}));
    }
    let path = fixture.transcript(&rows);
    // Tail is requested first, so it cannot borrow a previously full cache.
    let tail = codex::parse_session_messages(&path, 0, 2, true).unwrap();
    let full = codex::parse_all_messages(&path).unwrap();
    assert_eq!(tail.messages[0].uuid, full[4].uuid);
    assert_eq!(
        target_index(
            SessionSourceKind::Codex,
            &read_records(&path).unwrap(),
            full[4].uuid.as_deref().unwrap()
        )
        .unwrap(),
        7
    );
    let range = codex::parse_messages_range(&path, 4, 6).unwrap();
    assert_eq!(range.messages[0].uuid, tail.messages[0].uuid);
    rows.push(json!({"type":"event_msg","payload":{"type":"thread_rolled_back","num_turns":2}}));
    let rolled = fixture.0.join("rolled.jsonl");
    fs::write(&rolled, jsonl(&rows)).unwrap();
    let tail = codex::parse_session_messages(&rolled, 0, 2, true).unwrap();
    assert_eq!(tail.total, 2);
    assert_eq!(tail.messages[0].uuid, full[0].uuid);
    assert_eq!(codex::parse_all_messages(&rolled).unwrap().len(), 2);
}
