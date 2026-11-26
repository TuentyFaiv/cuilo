//! Update checks: at most once a day, an interactive run starts a detached `cuilo upgrade
//! --background`, which looks for a new release, installs it when the policy and the install allow,
//! and records what it found in `update.json` in the user cache folder for later runs to report.

use std::ffi::OsStr;
use std::io::{self, IsTerminal as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, fs};

use cuilo_core::workspace::{UpdatePolicy, user_cache_dir};
use cuilo_core::{UserConfig, brand};
use cuilo_packs::transport::{Fetch, HttpsFetch};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::cli::Cli;
use crate::upgrade::{self, Install};

/// What the last check found, in the user cache folder.
const STATE_FILE: &str = "update.json";

/// Held while a check runs.
const LOCK_FILE: &str = "update.lock";

/// How long a check's findings last, and how long a notice keeps quiet once printed.
const DAY: u64 = 24 * 60 * 60;

/// How old a lock must be before another check takes it over.
const STALE_LOCK: Duration = Duration::from_mins(10);

/// The update policy: `CUILO_UPDATES` when set, where an unknown value counts as `off`, else the
/// user config's; always `off` when `CI` is set.
fn policy(config: UpdatePolicy, env: Option<&OsStr>, ci: Option<&OsStr>) -> UpdatePolicy {
	if ci.is_some_and(|ci| !ci.is_empty()) {
		return UpdatePolicy::Off;
	}
	let Some(value) = env.filter(|value| !value.is_empty()) else {
		return config;
	};
	match value.to_str().map(|value| value.trim().to_ascii_lowercase()).as_deref() {
		Some("auto") => UpdatePolicy::Auto,
		Some("notify") => UpdatePolicy::Notify,
		_ => UpdatePolicy::Off,
	}
}

/// This process's update policy; `off` when the user config cannot be read and nothing overrides it.
fn current_policy() -> UpdatePolicy {
	let config = UserConfig::load().map_or(UpdatePolicy::Off, |user| user.updates);
	let env = env::var_os(format!("{}UPDATES", brand::ENV_PREFIX));
	policy(config, env.as_deref(), env::var_os("CI").as_deref())
}

/// Whether a run is interactive enough to check for updates: stderr is a terminal, the output is
/// not JSON, and the command is not one whose output other programs read.
fn interactive(cli: &Cli, terminal: bool) -> bool {
	terminal && !cli.json && cli.command.as_ref().is_none_or(crate::cli::Command::checks_for_updates)
}

/// The channel file a build follows: `latest-pre` for a pre-release, else `latest`.
fn channel(current: &Version) -> &'static str {
	if current.pre.is_empty() { "latest" } else { "latest-pre" }
}

/// Seconds since the Unix epoch.
fn seconds(time: SystemTime) -> u64 {
	time.duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// Whether a day has passed since `then`, or the clock went back past it.
fn a_day_since(then: u64, now: u64) -> bool {
	now < then || now - then >= DAY
}

/// `update.json`: what the last check found and what was already reported. Times are seconds since
/// the Unix epoch.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct State {
	/// When the last check started.
	checked_at: u64,
	/// The channel file it read, `latest` or `latest-pre`.
	channel: Option<String>,
	/// The newest release on that channel.
	latest: Option<Version>,
	/// When a notice was last printed.
	noticed_at: u64,
	/// The update the last check installed, until a run of that binary reports it.
	updated: Option<Updated>,
	/// Why the last check or update failed.
	error: Option<String>,
}

/// An update a background check installed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Updated {
	/// The version replaced.
	from: Version,
	/// The version installed.
	to: Version,
	/// The binary replaced.
	exe: String,
}

impl State {
	/// The state in `dir`, or a fresh one when it is missing or unreadable.
	fn load(dir: &Path) -> Self {
		fs::read(dir.join(STATE_FILE))
			.ok()
			.and_then(|bytes| serde_json::from_slice(&bytes).ok())
			.unwrap_or_default()
	}

	/// Writes the state to `dir` through a temporary file, so readers never see half of it.
	fn save(&self, dir: &Path) -> io::Result<()> {
		let file = dir.join(STATE_FILE);
		let temp = dir.join(format!(".{STATE_FILE}.{}", std::process::id()));
		let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
		fs::write(&temp, format!("{text}\n"))?;
		fs::rename(&temp, &file).inspect_err(|_| {
			let _ = fs::remove_file(&temp);
		})
	}

	/// Whether a build on `channel` should check again at `now`.
	fn due(&self, channel: &str, now: u64) -> bool {
		self.channel.as_deref() != Some(channel) || a_day_since(self.checked_at, now)
	}
}

/// Changes the state in `dir` as it is now, so writes by another process in between are kept.
fn record(dir: &Path, change: impl FnOnce(&mut State)) -> io::Result<()> {
	let mut state = State::load(dir);
	change(&mut state);
	state.save(dir)
}

/// The lock a check holds, so two never run at once; released on drop.
#[derive(Debug)]
struct Lock(PathBuf);

impl Lock {
	/// Takes the lock in `dir`, unless a check that took it within ten minutes of `now` holds it.
	fn take(dir: &Path, now: SystemTime) -> Option<Self> {
		let path = dir.join(LOCK_FILE);
		for _ in 0..2 {
			match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
				Ok(_) => return Some(Self(path)),
				Err(error) if error.kind() == io::ErrorKind::AlreadyExists && stale(&path, now) => {
					let _ = fs::remove_file(&path);
				}
				Err(_) => return None,
			}
		}
		None
	}
}

impl Drop for Lock {
	fn drop(&mut self) {
		let _ = fs::remove_file(&self.0);
	}
}

/// Whether the lock at `path` was taken at least ten minutes away from `now`, either way since
/// clocks move back, or its age is unknown.
fn stale(path: &Path, now: SystemTime) -> bool {
	fs::metadata(path)
		.and_then(|metadata| metadata.modified())
		.ok()
		.map(|modified| now.duration_since(modified).unwrap_or_else(|later| later.duration()))
		.is_none_or(|age| age >= STALE_LOCK)
}

/// The running binary as the update checks see it.
#[derive(Debug)]
struct Running {
	/// The user cache folder, which holds the state and the lock.
	dir: PathBuf,
	/// The binary, with links resolved.
	exe: PathBuf,
	/// How it was installed.
	install: Install,
	/// Its version.
	version: Version,
	/// The update policy.
	policy: UpdatePolicy,
}

impl Running {
	/// This process, or `None` without a cache folder or a known binary.
	fn this(policy: UpdatePolicy) -> Option<Self> {
		let exe = upgrade::current_exe().ok()?;
		Some(Self {
			dir: user_cache_dir()?,
			install: Install::of(&exe),
			exe,
			version: Version::parse(env!("CARGO_PKG_VERSION")).ok()?,
			policy,
		})
	}

	/// Starts `cuilo upgrade --background`, detached and with no stdio, in the cache folder so it
	/// holds no project folder open. It starts the binary by the path this process was started
	/// with, since a resolved Windows path is a `\\?\` one.
	fn spawn(&self) -> io::Result<Child> {
		fs::create_dir_all(&self.dir)?;
		let mut command = Command::new(env::current_exe().unwrap_or_else(|_| self.exe.clone()));
		command
			.args(["upgrade", "--background"])
			.current_dir(&self.dir)
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null());
		detach(&mut command)
	}

	/// The line to print about updates, if any, noting in `state` that it was printed: once, that
	/// this binary was updated; otherwise, at most once a day, that a newer release is available.
	fn report(&self, state: &mut State, now: u64) -> Option<String> {
		let exe = self.exe.to_string_lossy();
		if let Some(updated) = state.updated.take_if(|updated| updated.exe == exe)
			&& updated.to == self.version
		{
			return Some(format!("{} updated {} → {}.", brand::NAME, updated.from, updated.to));
		}
		let notify = match self.policy {
			UpdatePolicy::Notify => true,
			UpdatePolicy::Auto => self.install != Install::Script || state.error.is_some(),
			UpdatePolicy::Off => false,
		};
		let latest = state.latest.as_ref().filter(|latest| **latest > self.version)?;
		if !notify || state.channel.as_deref() != Some(channel(&self.version)) || !a_day_since(state.noticed_at, now) {
			return None;
		}
		let line = format!(
			"{} {latest} is available (you have {}); {}.",
			brand::NAME,
			self.version,
			self.install.hint()
		);
		state.noticed_at = now;
		Some(line)
	}
}

/// Update checks and notices for one interactive run.
#[derive(Debug)]
pub(crate) struct Watch(Running);

impl Watch {
	/// Starts a background check when one is due; `None` when this run neither checks nor reports.
	pub(crate) fn start(cli: &Cli) -> Option<Self> {
		if !interactive(cli, io::stderr().is_terminal()) {
			return None;
		}
		let policy = current_policy();
		if policy == UpdatePolicy::Off {
			return None;
		}
		let running = Running::this(policy)?;
		if State::load(&running.dir).due(channel(&running.version), seconds(SystemTime::now())) {
			let _ = running.spawn();
		}
		Some(Self(running))
	}

	/// Prints on stderr, after the command's own output, what the background checks found.
	pub(crate) fn finish(self) {
		let mut state = State::load(&self.0.dir);
		let before = state.clone();
		if let Some(line) = self.0.report(&mut state, seconds(SystemTime::now())) {
			eprintln!("{line}");
		}
		if state != before {
			let reported = state.updated != before.updated;
			let _ = record(&self.0.dir, |now| {
				now.noticed_at = state.noticed_at;
				if reported {
					now.updated = state.updated;
				}
			});
		}
	}
}

/// `cuilo upgrade --background`: checks for a new release and, when allowed, installs it.
pub(crate) fn background() {
	if let Some(running) = Running::this(current_policy()) {
		let _ = fs::create_dir_all(&running.dir);
		check(
			&HttpsFetch::default(),
			upgrade::RELEASE_KEY,
			&running,
			SystemTime::now(),
		);
	}
}

/// One check, unless another one runs or ran less than a day ago; it records what it found, and
/// failures, in the state.
fn check(fetch: &dyn Fetch, key: &str, running: &Running, now: SystemTime) {
	if running.policy == UpdatePolicy::Off {
		return;
	}
	let Some(_lock) = Lock::take(&running.dir, now) else {
		return;
	};
	let started = seconds(now);
	let channel = channel(&running.version);
	if !State::load(&running.dir).due(channel, started) {
		return;
	}
	let claimed = record(&running.dir, |state| {
		if state.channel.as_deref() != Some(channel) {
			state.latest = None;
		}
		state.checked_at = started;
		state.channel = Some(channel.to_owned());
	});
	if claimed.is_err() {
		return;
	}
	let mut latest = None;
	let mut updated = None;
	let result = update(fetch, key, running, &mut latest, &mut updated);
	let _ = record(&running.dir, |state| {
		state.latest = latest.or(state.latest.take());
		if updated.is_some() {
			state.updated = updated;
		}
		state.error = result.err().map(|error| format!("{error:#}"));
	});
}

/// Reads the newest release into `latest` and, with policy `auto` for a script install, installs
/// it when newer, after checking its signature and digest, noting it in `updated`.
fn update(
	fetch: &dyn Fetch,
	key: &str,
	running: &Running,
	latest: &mut Option<Version>,
	updated: &mut Option<Updated>,
) -> anyhow::Result<()> {
	let newest = upgrade::latest(fetch, !running.version.pre.is_empty())?;
	*latest = Some(newest.clone());
	if running.policy != UpdatePolicy::Auto || running.install != Install::Script || newest <= running.version {
		return Ok(());
	}
	let release = upgrade::release(fetch, key, &newest)?;
	let bytes = upgrade::download(fetch, &release)?;
	upgrade::replace(&running.exe, &bytes, &release.version)?;
	*updated = Some(Updated {
		from: running.version.clone(),
		to: release.version,
		exe: running.exe.to_string_lossy().into_owned(),
	});
	Ok(())
}

/// Starts `command` in its own process group, so neither Ctrl-C in the terminal nor closing it
/// stops it.
#[cfg(unix)]
fn detach(command: &mut Command) -> io::Result<Child> {
	use std::os::unix::process::CommandExt as _;
	command.process_group(0).spawn()
}

/// Starts `command` without a console, so no window opens, in its own process group, and out of
/// this process's job when the job allows it, so closing the terminal does not end it.
#[cfg(windows)]
fn detach(command: &mut Command) -> io::Result<Child> {
	use std::os::windows::process::CommandExt as _;
	const DETACHED_PROCESS: u32 = 0x0000_0008;
	const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
	const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
	let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
	command.creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB);
	match command.spawn() {
		Ok(child) => Ok(child),
		Err(_) => {
			command.creation_flags(flags);
			command.spawn()
		}
	}
}

#[cfg(not(any(unix, windows)))]
fn detach(command: &mut Command) -> io::Result<Child> {
	command.spawn()
}

#[cfg(test)]
mod tests {
	use clap::Parser as _;

	use super::*;
	use crate::upgrade::tests::{BINARY, KEY, Served, release_server, version};

	const NOW: u64 = 1_800_000_000;

	fn running(dir: &Path, exe: PathBuf, policy: UpdatePolicy, current: &str) -> Running {
		Running {
			dir: dir.to_path_buf(),
			install: Install::of(&exe),
			exe,
			version: version(current),
			policy,
		}
	}

	/// A script install of 2.0.0 in `dir/bin`, with its cache in `dir/cache`.
	fn script_install(dir: &Path, policy: UpdatePolicy) -> anyhow::Result<Running> {
		let exe = dir.join("bin/cuilo");
		fs::create_dir_all(dir.join("bin"))?;
		fs::create_dir_all(dir.join("cache"))?;
		fs::write(&exe, "old")?;
		fs::write(exe.with_file_name(brand::INSTALL_RECEIPT), "{}")?;
		Ok(running(&dir.join("cache"), exe, policy, "2.0.0"))
	}

	/// Release 99.0.0 on the stable channel, with `binary` as this target's binary.
	fn newer_release(binary: &[u8]) -> Served {
		let mut served = release_server(binary);
		served.bodies.push(("/latest".to_owned(), b"99.0.0\n".to_vec()));
		served
	}

	fn asked(fetch: &Served) -> Vec<String> {
		fetch.asked.lock().map(|asked| asked.clone()).unwrap_or_default()
	}

	fn at(seconds: u64) -> SystemTime {
		UNIX_EPOCH + Duration::from_secs(seconds)
	}

	#[test]
	fn policy_should_take_the_environment_over_the_config_and_turn_off_in_ci() {
		let os = |value: &'static str| Some(OsStr::new(value));

		assert_eq!(policy(UpdatePolicy::Auto, None, None), UpdatePolicy::Auto);
		assert_eq!(policy(UpdatePolicy::Off, None, None), UpdatePolicy::Off);
		assert_eq!(policy(UpdatePolicy::Auto, os("notify"), None), UpdatePolicy::Notify);
		assert_eq!(policy(UpdatePolicy::Off, os(" AUTO "), None), UpdatePolicy::Auto);
		assert_eq!(policy(UpdatePolicy::Notify, os("off"), None), UpdatePolicy::Off);
		assert_eq!(policy(UpdatePolicy::Auto, os("sometimes"), None), UpdatePolicy::Off);
		assert_eq!(policy(UpdatePolicy::Notify, os(""), None), UpdatePolicy::Notify);
		assert_eq!(policy(UpdatePolicy::Auto, os("auto"), os("true")), UpdatePolicy::Off);
		assert_eq!(policy(UpdatePolicy::Notify, None, os("1")), UpdatePolicy::Off);
		assert_eq!(policy(UpdatePolicy::Auto, None, os("")), UpdatePolicy::Auto);
	}

	#[test]
	fn interactive_should_need_a_terminal_and_leave_out_json_and_machine_commands() -> anyhow::Result<()> {
		let cli = |args: &[&str]| Cli::try_parse_from([brand::BINARY_NAME].iter().chain(args));

		for args in [&[][..], &["status"], &["log"], &["pack", "list"]] {
			assert!(interactive(&cli(args)?, true), "{args:?}");
			assert!(!interactive(&cli(args)?, false), "{args:?}");
		}
		for args in [
			&["--json", "status"][..],
			&["mcp"],
			&["rpc"],
			&["completions", "zsh"],
			&["man"],
			&["upgrade"],
			&["upgrade", "--check"],
			&["upgrade", "--background"],
		] {
			assert!(!interactive(&cli(args)?, true), "{args:?}");
		}
		Ok(())
	}

	#[test]
	fn state_should_be_due_a_day_after_a_check_on_the_same_channel() {
		let state = State {
			checked_at: NOW,
			channel: Some("latest".to_owned()),
			..State::default()
		};

		assert!(State::default().due("latest", NOW));
		assert!(!state.due("latest", NOW + DAY - 1));
		assert!(state.due("latest", NOW + DAY));
		assert!(
			state.due("latest-pre", NOW + 1),
			"another channel's findings do not count"
		);
		assert!(state.due("latest", NOW - 60), "a clock that went back counts as stale");
	}

	#[test]
	fn state_should_round_trip_and_read_a_broken_file_as_empty() -> anyhow::Result<()> {
		let dir = tempfile::tempdir()?;
		let state = State {
			checked_at: NOW,
			channel: Some("latest".to_owned()),
			latest: Some(version("2.1.0")),
			error: Some("offline".to_owned()),
			..State::default()
		};

		assert_eq!(State::load(dir.path()), State::default());
		state.save(dir.path())?;
		assert_eq!(State::load(dir.path()), state);
		let text = fs::read_to_string(dir.path().join(STATE_FILE))?;
		assert!(
			text.contains("\"checked_at\": 1800000000") && text.contains("\"latest\": \"2.1.0\""),
			"{text}"
		);
		assert_eq!(
			fs::read_dir(dir.path())?.count(),
			1,
			"the temporary file was left behind"
		);
		fs::write(dir.path().join(STATE_FILE), "{ not json")?;
		assert_eq!(State::load(dir.path()), State::default());
		Ok(())
	}

	#[test]
	fn lock_should_keep_a_second_check_out_until_released_or_stale() -> anyhow::Result<()> {
		let dir = tempfile::tempdir()?;
		let now = SystemTime::now();

		let held = Lock::take(dir.path(), now);
		assert!(held.is_some());
		assert!(Lock::take(dir.path(), now).is_none());
		drop(held);
		assert!(!dir.path().join(LOCK_FILE).exists(), "the lock outlived its check");
		assert!(Lock::take(dir.path(), now).is_some());

		fs::write(dir.path().join(LOCK_FILE), "")?;
		assert!(Lock::take(dir.path(), now + Duration::from_mins(9)).is_none());
		assert!(
			Lock::take(dir.path(), now - Duration::from_mins(11)).is_some(),
			"a clock that went back keeps the lock for good"
		);
		fs::write(dir.path().join(LOCK_FILE), "")?;
		assert!(
			Lock::take(dir.path(), now + Duration::from_mins(11)).is_some(),
			"a lock left by a check that died blocks forever"
		);
		Ok(())
	}

	#[test]
	fn report_should_say_once_that_this_binary_was_updated() {
		let dir = Path::new("/cache");
		let exe = PathBuf::from("/home/me/.cuilo/bin/cuilo");
		let now = running(dir, exe.clone(), UpdatePolicy::Auto, "2.1.0");
		let updated = |exe: &Path, to: &str| State {
			updated: Some(Updated {
				from: version("2.0.0"),
				to: version(to),
				exe: exe.to_string_lossy().into_owned(),
			}),
			..State::default()
		};

		let mut state = updated(&exe, "2.1.0");
		assert_eq!(
			now.report(&mut state, NOW).as_deref(),
			Some("cuilo updated 2.0.0 → 2.1.0.")
		);
		assert_eq!(state.updated, None);
		assert_eq!(now.report(&mut state, NOW), None);

		let mut other = updated(Path::new("/opt/cuilo/bin/cuilo"), "2.1.0");
		assert_eq!(now.report(&mut other, NOW), None);
		assert!(other.updated.is_some(), "another copy's update was reported here");

		let mut reinstalled = updated(&exe, "2.0.5");
		assert_eq!(now.report(&mut reinstalled, NOW), None);
		assert_eq!(reinstalled.updated, None);
	}

	#[test]
	fn report_should_print_the_notice_at_most_once_a_day() {
		let exe = PathBuf::from("/usr/local/lib/node_modules/@cuilo/cli-linux-x64/bin/cuilo");
		let npm = running(Path::new("/cache"), exe, UpdatePolicy::Auto, "2.0.0");
		let mut state = State {
			checked_at: NOW,
			channel: Some("latest".to_owned()),
			latest: Some(version("2.1.0")),
			..State::default()
		};

		assert_eq!(
			npm.report(&mut state, NOW).as_deref(),
			Some("cuilo 2.1.0 is available (you have 2.0.0); run `cuilo upgrade`.")
		);
		assert_eq!(state.noticed_at, NOW);
		assert_eq!(npm.report(&mut state, NOW + 3600), None);
		assert!(npm.report(&mut state, NOW + DAY).is_some());
	}

	#[test]
	fn report_should_notify_by_policy_and_install() -> anyhow::Result<()> {
		let dir = tempfile::tempdir()?;
		let script = |policy| script_install(dir.path(), policy);
		let elsewhere = |policy| running(dir.path(), PathBuf::from("/usr/local/bin/cuilo"), policy, "2.0.0");
		let found = |latest: &str, channel: &str, error: Option<&str>| State {
			checked_at: NOW,
			channel: Some(channel.to_owned()),
			latest: Some(version(latest)),
			error: error.map(str::to_owned),
			..State::default()
		};
		let notice = |running: &Running, mut state: State| running.report(&mut state, NOW);

		assert_eq!(
			notice(&script(UpdatePolicy::Auto)?, found("2.1.0", "latest", None)),
			None,
			"auto-update covers it"
		);
		assert!(notice(&script(UpdatePolicy::Auto)?, found("2.1.0", "latest", Some("offline"))).is_some());
		assert!(notice(&script(UpdatePolicy::Notify)?, found("2.1.0", "latest", None)).is_some());
		assert_eq!(
			notice(&elsewhere(UpdatePolicy::Off), found("2.1.0", "latest", None)),
			None
		);
		assert_eq!(
			notice(&elsewhere(UpdatePolicy::Auto), found("2.1.0", "latest", None)).as_deref(),
			Some(
				"cuilo 2.1.0 is available (you have 2.0.0); cuilo did not install this copy; reinstall with the install script at https://dl.cuilo.dev/install.sh."
			)
		);
		assert_eq!(
			notice(&elsewhere(UpdatePolicy::Notify), found("2.0.0", "latest", None)),
			None
		);
		assert_eq!(
			notice(&elsewhere(UpdatePolicy::Notify), found("1.9.0", "latest", None)),
			None
		);
		assert_eq!(
			notice(
				&elsewhere(UpdatePolicy::Notify),
				found("2.1.0-beta.1", "latest-pre", None)
			),
			None,
			"a stable build reported a pre-release"
		);
		Ok(())
	}

	#[test]
	fn check_should_install_a_newer_release_for_a_script_install_and_record_it() -> anyhow::Result<()> {
		let dir = tempfile::tempdir()?;
		let old = script_install(dir.path(), UpdatePolicy::Auto)?;
		let fetch = newer_release(BINARY);

		check(&fetch, KEY, &old, at(NOW));

		assert_eq!(fs::read(&old.exe)?, BINARY);
		let state = State::load(&old.dir);
		assert_eq!(state.checked_at, NOW);
		assert_eq!(state.latest, Some(version("99.0.0")));
		assert_eq!(state.error, None);
		assert!(!old.dir.join(LOCK_FILE).exists());
		let mut state = state;
		let new = running(&old.dir, old.exe.clone(), UpdatePolicy::Auto, "99.0.0");
		assert_eq!(
			new.report(&mut state, NOW + 60).as_deref(),
			Some("cuilo updated 2.0.0 → 99.0.0.")
		);
		Ok(())
	}

	#[test]
	fn check_should_only_record_the_release_for_other_installs_and_policies() -> anyhow::Result<()> {
		let dir = tempfile::tempdir()?;
		let exe = dir.path().join("node_modules/@cuilo/cli-darwin-arm64/bin/cuilo");
		fs::create_dir_all(dir.path().join("node_modules/@cuilo/cli-darwin-arm64/bin"))?;
		fs::write(&exe, "old")?;
		let npm = running(dir.path(), exe.clone(), UpdatePolicy::Auto, "2.0.0");
		let notify = script_install(&dir.path().join("script"), UpdatePolicy::Notify)?;

		for running in [&npm, &notify] {
			let fetch = newer_release(BINARY);

			check(&fetch, KEY, running, at(NOW));

			assert_eq!(fs::read(&running.exe)?, b"old");
			assert_eq!(asked(&fetch).len(), 1, "it downloaded the release");
			let state = State::load(&running.dir);
			assert_eq!((state.latest, state.updated), (Some(version("99.0.0")), None));
		}
		Ok(())
	}

	#[test]
	fn check_should_refuse_a_tampered_release_and_retry_the_next_day() -> anyhow::Result<()> {
		let dir = tempfile::tempdir()?;
		let script = script_install(dir.path(), UpdatePolicy::Auto)?;
		let fetch = newer_release(b"evil");

		check(&fetch, KEY, &script, at(NOW));
		assert_eq!(fs::read(&script.exe)?, b"old");
		let state = State::load(&script.dir);
		assert!(
			state
				.error
				.as_deref()
				.is_some_and(|error| error.contains("does not match its signed checksum")),
			"{state:?}"
		);
		assert_eq!(state.updated, None);
		let tries = asked(&fetch).len();

		check(&fetch, KEY, &script, at(NOW + 3600));
		assert_eq!(asked(&fetch).len(), tries, "it checked again within a day");
		check(&fetch, KEY, &script, at(NOW + DAY));
		assert!(asked(&fetch).len() > tries, "it did not retry the next day");
		Ok(())
	}

	#[test]
	fn check_should_wait_for_a_running_check_and_never_run_when_off() -> anyhow::Result<()> {
		let dir = tempfile::tempdir()?;
		let script = script_install(dir.path(), UpdatePolicy::Auto)?;
		let off = script_install(&dir.path().join("off"), UpdatePolicy::Off)?;
		let fetch = newer_release(BINARY);
		fs::write(script.dir.join(LOCK_FILE), "")?;

		check(&fetch, KEY, &script, SystemTime::now());
		check(&fetch, KEY, &off, SystemTime::now());

		assert_eq!(
			asked(&fetch).len(),
			0,
			"it downloaded while another check ran, or when off"
		);
		assert_eq!(State::load(&script.dir), State::default());
		assert_eq!(fs::read(&script.exe)?, b"old");
		Ok(())
	}
}
