//! Native libc timezone tests isolated in child processes; no parent TZ changes.
#[cfg(unix)]
#[test]
fn isolated_new_york_dst_native_midnights() {
    let executable = std::env::current_exe().unwrap();
    let result = std::process::Command::new(executable)
        .arg("--exact")
        .arg("dst_child_probe")
        .arg("--nocapture")
        .env("TZ", "America/New_York")
        .env("HUD_DST_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
#[cfg(unix)]
#[test]
fn dst_child_probe() {
    if std::env::var("HUD_DST_CHILD").as_deref() != Ok("1") {
        return;
    }
    // UTC noons safely inside the requested local days. New York transitions:
    // March 8 2026 jumps UTC-5→UTC-4; November 1 falls UTC-4→UTC-5.
    use hud_data::local_moment;
    let spring = local_moment(1_772_971_200).unwrap(); // 2026-03-08 12:00 UTC
    let next = local_moment(1_773_057_600).unwrap();
    assert_eq!(spring.today.iso(), "2026-03-08");
    assert_eq!(next.today.iso(), "2026-03-09");
    assert_eq!(next.midnight_unix - spring.midnight_unix, 23 * 3600);
    let fall = local_moment(1_793_534_400).unwrap(); // 2026-11-01 12:00 UTC
    let next = local_moment(1_793_620_800).unwrap();
    assert_eq!(fall.today.iso(), "2026-11-01");
    assert_eq!(next.today.iso(), "2026-11-02");
    assert_eq!(next.midnight_unix - fall.midnight_unix, 25 * 3600);
}
