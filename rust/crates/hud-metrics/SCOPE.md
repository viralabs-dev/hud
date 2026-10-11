# Linux metrics slice / Fatia Linux

Source contract: hud/metrics_linux.py and hud/metrics.py, tests/test_metrics.py.
CPU excludes guest counters already included in user/nice, idle includes iowait.
Memory and swap use MemAvailable/SwapFree and 1024 bytes per kB.
Network excludes lo, veth*, docker*, br-*, virbr*. Rates are bytes/second.
Temperature selects preferred hwmon driver order and highest valid reading,
converting millidegrees to degrees. Battery preserves kernel status strings.
Missing, malformed, overflowed, counter resets and initial delta => None.
This deliberately strengthens Python's misleading first-sample numeric zeros.
Histories preserve unavailable gaps (Option), capacity capped at 3600 samples.
Injected proc/sys roots and monotonic timestamp support deterministic fixtures.
Each text read is capped at 128 KiB; hwmon/power scans cap at 128 entries.
Proc/sys roots are caller trusted, sysfs device links intentionally followed.

Eight tests passed on Linux, including read-only live Linux memory/uptime smoke,
fixtures, delta/cached network, reset/unavailable, bounded reads, disk capacity/dedup and final symlink/file refusal. cargo fmt,
cargo clippy --all-targets -- -D warnings passed. No other platform validation.

Linux native disk capacity uses existing rustix =1.1.5 fs safe wrappers,
open DIRECTORY/NOFOLLOW/NONBLOCK and fstat/fstatvfs on the same descriptor.
Total and used match Python (user-available includes reserved blocks in used).
Device ID dedup preserves root/home/extra order; failed capacity still marks
volume observed like Python. Paths capped at 64. No mounttable enumeration:
Python only uses root, home and explicit extras, so adding a mount scanner would
change scope. Default backend samples / only; root can explicitly add home/extras.
Final disk symlinks rejected; ancestor links resolve metadata only. Trusted
proc/sys paths intentionally follow sysfs class device links; injected fixture
roots are test inputs, not a hardened interface for arbitrary attacker roots.

Remaining AT-074 gaps: macOS/Windows adapters, native platform tests, scheduler
integration, hostname and complete system panel formatting. No shell commands,
system packages, Python edits or active config. Dependency reused from project.

Integration: copy src/lib.rs to crates/hud-metrics/src/lib.rs; adapt Cargo.toml
package fields to workspace conventions, remove local [workspace], add member
and hud-tui path dependency. Construct LinuxBackend only on cfg Linux; do not
represent absence of /proc on other platforms as a successful native adapter.
