use hud_resources::session_monitor::{parse, Status};
const HEAD: &str = r#"{"type":"user","sessionId":"c"}"#;
const START: &str = r#"{"type":"assistant","sessionId":"c","message":{"content":[{"type":"tool_use","name":"Agent","id":"a","input":{"prompt":"PRIVATE"}}]}}"#;
fn tail(result: &str) -> String {
    format!("{START}\n{result}")
}
#[test]
fn sync_result_finishes() {
    let s=parse(HEAD,&tail(r#"{"type":"user","sessionId":"c","message":{"content":[{"type":"tool_result","tool_use_id":"a","content":"PRIVATE"}]}}"#)).unwrap();
    assert_eq!(s.subagents[0].status, Status::Completed);
    assert!(!format!("{s:?}").contains("PRIVATE"));
}
#[test]
fn async_launch_stays_active() {
    let s=parse(HEAD,&tail(r#"{"type":"user","sessionId":"c","toolUseResult":{"status":"async_launched"},"message":{"content":[{"type":"tool_result","tool_use_id":"a","content":"PRIVATE"}]}}"#)).unwrap();
    assert_eq!(s.subagents[0].status, Status::Active);
}
#[test]
fn async_final_notification_finishes() {
    let data = format!(
        "{}\n{}",
        tail(
            r#"{"type":"user","sessionId":"c","toolUseResult":{"status":"async_launched"},"message":{"content":[{"type":"tool_result","tool_use_id":"a"}]}}"#
        ),
        r#"{"type":"queue-operation","sessionId":"c","content":"<task-notification><tool-use-id>a</tool-use-id><status>completed</status><summary>PRIVATE</summary></task-notification>"}"#
    );
    let s = parse(HEAD, &data).unwrap();
    assert_eq!(s.subagents[0].status, Status::Completed);
    assert!(!format!("{s:?}").contains("PRIVATE"));
}
#[test]
fn unknown_malformed_wrong_session_remain_active() {
    for text in ["<task-notification><tool-use-id>a</tool-use-id><status>pending</status></task-notification>","<task-notification><tool-use-id>a</tool-use-id><status>mystery</status></task-notification>","<task-notification><tool-use-id>a</tool-use-id><status>completed</status>","<task-notification><tool-use-id>other</tool-use-id><status>completed</status></task-notification>"]{let record=serde_json::json!({"type":"queue-operation","sessionId":"c","content":text});let s=parse(HEAD,&tail(&record.to_string())).unwrap();assert_eq!(s.subagents[0].status,Status::Active);}
}
#[test]
fn error_notification_aborts() {
    let record = serde_json::json!({"type":"queue-operation","sessionId":"c","content":"<task-notification><tool-use-id>a</tool-use-id><status>failed</status></task-notification>"});
    assert_eq!(
        parse(HEAD, &tail(&record.to_string())).unwrap().subagents[0].status,
        Status::Aborted
    );
}
#[test]
fn notification_other_session_does_not_finish() {
    let record = serde_json::json!({"type":"queue-operation","sessionId":"other","content":"<task-notification><tool-use-id>a</tool-use-id><status>completed</status></task-notification>"});
    assert_eq!(
        parse(HEAD, &tail(&record.to_string())).unwrap().subagents[0].status,
        Status::Active
    );
}

#[test]
fn quoted_notifications_do_not_finish() {
    let text="<task-notification><tool-use-id>a</tool-use-id><status>completed</status></task-notification>";
    let records = [
        serde_json::json!({"type":"user","sessionId":"c","message":{"content":text}}),
        serde_json::json!({"type":"assistant","sessionId":"c","message":{"content":[{"type":"text","text":text}]}}),
        serde_json::json!({"type":"user","sessionId":"c","toolUseResult":{"status":"async_launched"},"message":{"content":[{"type":"tool_result","tool_use_id":"a","content":text}]}}),
        serde_json::json!({"type":"tool","sessionId":"c","content":text}),
        serde_json::json!({"type":"queue-operation","sessionId":"c","content":format!("quoted: {text}")}),
    ];
    for record in records {
        assert_eq!(
            parse(HEAD, &tail(&record.to_string())).unwrap().subagents[0].status,
            Status::Active
        );
    }
}
