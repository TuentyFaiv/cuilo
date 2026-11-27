//! The stacks the official packs are verified in, each a fixture project in `fixtures/stacks/<name>`.
//!
//! Every variant of every official piece must be covered by a stack here or listed in [`UNCOVERED`].

use super::follow::Follow;

/// A command line: the program, then its arguments.
pub(crate) type Cmd = &'static [&'static str];

/// How a stack's toolchain installs and updates dependencies; each group of them is one CI job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Ecosystem {
	/// TypeScript with Bun as the package manager and Node.js running the tools.
	Bun,
	/// Rust with Cargo.
	Cargo,
	/// Go modules.
	Go,
	/// Python with uv.
	Uv,
	/// Dart with Flutter's `flutter pub`.
	Flutter,
	/// Kotlin on the JVM, built by the project's Gradle wrapper.
	Gradle,
	/// Kotlin for Android, built by the project's Gradle wrapper with the Android SDK.
	Android,
	/// Swift apps built by an Xcode project.
	Xcode,
	/// Swift packages built by the Swift Package Manager, with the SDKs of Xcode.
	SwiftPm,
}

/// What shows that a toolchain is installed.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Probe {
	/// The command succeeds.
	Runs(Cmd),
	/// One of these environment variables names a folder, as `ANDROID_HOME` does the Android SDK.
	Folder(&'static [&'static str]),
}

impl Ecosystem {
	/// Every ecosystem, in the order `--list` shows them; ecosystems of one group are next to each other.
	pub(crate) const ALL: [Self; 9] = [
		Self::Bun,
		Self::Cargo,
		Self::Go,
		Self::Uv,
		Self::Flutter,
		Self::Gradle,
		Self::Android,
		Self::Xcode,
		Self::SwiftPm,
	];

	/// The group `--group` selects and the CI job is named after.
	pub(crate) fn group(self) -> &'static str {
		match self {
			Self::Bun => "typescript",
			Self::Cargo => "rust",
			Self::Go => "go",
			Self::Uv => "python",
			Self::Flutter => "flutter",
			Self::Gradle | Self::Android => "kotlin",
			Self::Xcode | Self::SwiftPm => "swift",
		}
	}

	/// What holds when the toolchain is installed; `--latest` also needs the tools that look up new releases.
	pub(crate) fn probes(self, latest: bool) -> Vec<Probe> {
		const JAVA: Probe = Probe::Runs(&["java", "-version"]);
		const CURL: Probe = Probe::Runs(&["curl", "--version"]);
		const XCODE: Probe = Probe::Runs(&["xcodebuild", "-version"]);
		const ANDROID_SDK: Probe = Probe::Folder(&["ANDROID_HOME", "ANDROID_SDK_ROOT"]);
		let mut probes = match self {
			Self::Bun => vec![Probe::Runs(&["bun", "--version"]), Probe::Runs(&["node", "--version"])],
			Self::Cargo if latest => vec![
				Probe::Runs(&["cargo", "clippy", "--version"]),
				Probe::Runs(&["cargo", "upgrade", "--version"]),
			],
			Self::Cargo => vec![Probe::Runs(&["cargo", "clippy", "--version"])],
			Self::Go => vec![Probe::Runs(&["go", "version"])],
			Self::Uv => vec![Probe::Runs(&["uv", "--version"])],
			Self::Flutter => vec![Probe::Runs(&["flutter", "--version"])],
			Self::Gradle => vec![JAVA],
			Self::Android => vec![JAVA, ANDROID_SDK],
			Self::Xcode => vec![XCODE],
			Self::SwiftPm => vec![Probe::Runs(&["swift", "--version"]), XCODE],
		};
		if latest && matches!(self, Self::Gradle | Self::Android) {
			probes.push(CURL);
		}
		probes
	}

	/// Installs the fixture's locked dependencies.
	pub(crate) fn install(self) -> Cmd {
		match self {
			Self::Bun => &["bun", "install", "--frozen-lockfile"],
			Self::Cargo => &["cargo", "fetch", "--locked"],
			Self::Go => &["go", "mod", "download"],
			Self::Uv => &["uv", "sync", "--locked"],
			Self::Flutter => &["flutter", "pub", "get", "--enforce-lockfile"],
			Self::Gradle | Self::Android => GRADLE_SETUP,
			Self::Xcode => &[
				"xcodebuild",
				"-resolvePackageDependencies",
				"-scheme",
				XCODE_SCHEME,
				"-derivedDataPath",
				XCODE_DERIVED_DATA,
			],
			Self::SwiftPm => &["swift", "package", "resolve"],
		}
	}

	/// Runs after `cuilo add` installed what it asked for, so the project builds as its toolchain expects.
	pub(crate) fn settle(self) -> &'static [Cmd] {
		match self {
			Self::Go => &[&["go", "mod", "tidy"]],
			Self::Bun
			| Self::Cargo
			| Self::Uv
			| Self::Flutter
			| Self::Gradle
			| Self::Android
			| Self::Xcode
			| Self::SwiftPm => &[],
		}
	}
}

/// The scheme of the Xcode fixture, which Xcode names after the app.
const XCODE_SCHEME: &str = "CuiloFixture";
/// Where Xcode builds, inside the fixture's copy instead of the user's `~/Library`.
const XCODE_DERIVED_DATA: &str = "DerivedData";

/// Downloads the wrapper's Gradle and the build's plugins by configuring the project; Gradle fetches the rest
/// as tasks need it.
pub(crate) const GRADLE_SETUP: Cmd = &["./gradlew", "--console=plain", "help"];

/// A command whose success, and for [`Check::quiet`] whose empty output, the verification requires.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Check {
	/// The command.
	pub(crate) cmd: Cmd,
	/// Whether it must also print nothing, as `gofmt -l` does when every file is formatted.
	pub(crate) quiet: bool,
}

impl Check {
	const fn run(cmd: Cmd) -> Self {
		Self { cmd, quiet: false }
	}

	const fn quiet(cmd: Cmd) -> Self {
		Self { cmd, quiet: true }
	}
}

/// Pieces of a pack, in one variant, that a stack verifies.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cover {
	/// The pack.
	pub(crate) pack: &'static str,
	/// The variant `cuilo add --variant` gets.
	pub(crate) variant: &'static str,
	/// The pieces; empty for every piece that has the variant.
	pub(crate) pieces: &'static [&'static str],
	/// Pieces of the same pack, with their variants, added first because these pieces build on them.
	pub(crate) first: &'static [(&'static str, &'static str)],
}

impl Cover {
	const fn all(pack: &'static str, variant: &'static str) -> Self {
		Self {
			pack,
			variant,
			pieces: &[],
			first: &[],
		}
	}

	const fn pieces(pack: &'static str, variant: &'static str, pieces: &'static [&'static str]) -> Self {
		Self {
			pack,
			variant,
			pieces,
			first: &[],
		}
	}

	const fn after(mut self, first: &'static [(&'static str, &'static str)]) -> Self {
		self.first = first;
		self
	}
}

/// A fixture project and how to verify pieces in it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Stack {
	/// The name `--stack` selects; the fixture is `fixtures/stacks/<name>`.
	pub(crate) name: &'static str,
	/// The toolchain.
	pub(crate) ecosystem: Ecosystem,
	/// Stack facts `cuilo detect` must report, as `(fact, value)`.
	pub(crate) detects: &'static [(&'static str, &'static str)],
	/// What it verifies.
	pub(crate) covers: &'static [Cover],
	/// Folders its build compiles, one of which every file a piece creates must be in so the checks see it; empty
	/// when the checks see the whole project.
	pub(crate) sources: &'static [&'static str],
	/// Packages `--latest` keeps at the fixture's version, because the pack or the stack's tools do not support their
	/// newest release yet; for Gradle, keys of the version catalog's `[versions]`, or `gradle` for the wrapper.
	pub(crate) hold: &'static [&'static str],
	/// Commands run after installing, before cuilo adds anything.
	pub(crate) prepare: &'static [Cmd],
	/// Whether its tests run in Playwright's Chromium, installed before cuilo adds anything, with its system
	/// libraries on Linux in CI.
	pub(crate) browser: bool,
	/// The ecosystem's checks: type checkers, linters and compilers.
	pub(crate) checks: &'static [Check],
	/// The test runner, which runs the tests the pieces wrote.
	pub(crate) tests: &'static [Check],
	/// The next steps of the pieces that verification follows by hand, as a user would.
	pub(crate) follow: Follow,
}

/// Variants no stack verifies yet, as `(pack, variants, reason)`.
pub(crate) const UNCOVERED: &[(&str, &[&str], &str)] = &[];

const TSC: &[Check] = &[Check::run(&["bun", "run", "tsc", "--noEmit"])];
const VITEST: &[Check] = &[Check::run(&["bun", "run", "vitest", "run"])];
const CLIPPY: &[Check] = &[Check::run(&[
	"cargo",
	"clippy",
	"--all-targets",
	"--",
	"-D",
	"warnings",
])];
const CARGO_TEST: &[Check] = &[Check::run(&["cargo", "test"])];
const GO_CHECKS: &[Check] = &[Check::run(&["go", "vet", "./..."]), Check::quiet(&["gofmt", "-l", "."])];
const GO_TEST: &[Check] = &[Check::run(&["go", "test", "./..."])];
const MYPY: &[Check] = &[Check::run(&["uv", "run", "mypy", "."])];
const PYTEST: &[Check] = &[Check::run(&["uv", "run", "pytest"])];

const TYPESCRIPT: (&str, &str) = ("language", "typescript");
/// svelte-check, `astro check`, `vue-tsc` and the Angular compiler use TypeScript's JavaScript API, which TypeScript 7
/// does not have.
const TYPESCRIPT_6: &[&str] = &["typescript"];
/// Prisma's `latest` tag is the 8.0 release candidate, which has no client to generate; the pack targets Prisma 7.
const PRISMA: &[&str] = &["prisma", "@prisma/client", "@prisma/adapter-pg"];

/// The simulator the Xcode tests run on, which the iOS runtimes of Xcode 27 include.
const IOS_SIMULATOR: &str = "platform=iOS Simulator,name=iPhone 17";

/// Every stack, in the order they run: Svelte, Astro and Rust first, as cuilo leads with them.
pub(crate) const STACKS: &[Stack] = &[
	Stack {
		name: "svelte",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "svelte"), TYPESCRIPT],
		covers: &[Cover::all("ui", "svelte")],
		sources: &[],
		hold: TYPESCRIPT_6,
		prepare: &[],
		browser: true,
		checks: &[
			Check::run(&["bun", "run", "svelte-kit", "sync"]),
			Check::run(&[
				"bun",
				"run",
				"svelte-check",
				"--tsconfig",
				"./tsconfig.json",
				"--fail-on-warnings",
			]),
		],
		tests: VITEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "astro",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "astro"), TYPESCRIPT],
		covers: &[Cover::all("ui", "astro")],
		sources: &[],
		hold: TYPESCRIPT_6,
		prepare: &[],
		browser: false,
		checks: &[Check::run(&[
			"bun",
			"run",
			"astro",
			"check",
			"--minimumFailingSeverity",
			"warning",
		])],
		tests: VITEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "leptos",
		ecosystem: Ecosystem::Cargo,
		detects: &[("framework", "leptos")],
		covers: &[Cover::all("ui", "leptos")],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: CLIPPY,
		tests: CARGO_TEST,
		follow: Follow::RustUi,
	},
	Stack {
		name: "dioxus",
		ecosystem: Ecosystem::Cargo,
		detects: &[("framework", "dioxus")],
		covers: &[Cover::all("ui", "dioxus")],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: CLIPPY,
		tests: CARGO_TEST,
		follow: Follow::RustUi,
	},
	Stack {
		name: "yew",
		ecosystem: Ecosystem::Cargo,
		detects: &[("framework", "yew")],
		covers: &[Cover::all("ui", "yew")],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: CLIPPY,
		tests: CARGO_TEST,
		follow: Follow::RustUi,
	},
	Stack {
		name: "axum",
		ecosystem: Ecosystem::Cargo,
		detects: &[("framework", "axum")],
		covers: &[Cover::pieces("backend", "axum", &["error", "middleware", "resource"])],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: CLIPPY,
		tests: CARGO_TEST,
		follow: Follow::Axum,
	},
	Stack {
		name: "axum-sqlx",
		ecosystem: Ecosystem::Cargo,
		detects: &[("framework", "axum"), ("database", "sqlx")],
		covers: &[
			Cover::pieces("backend", "axum", &["database"]),
			Cover::pieces("backend", "axum", &["resource"]).after(&[("database", "axum")]),
		],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: CLIPPY,
		tests: CARGO_TEST,
		follow: Follow::Axum,
	},
	Stack {
		name: "angular",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "angular"), TYPESCRIPT],
		covers: &[Cover::all("ui", "angular")],
		sources: &[],
		hold: TYPESCRIPT_6,
		prepare: &[],
		browser: false,
		checks: &[Check::run(&["bun", "run", "ng", "build"])],
		tests: &[Check::run(&["bun", "run", "ng", "test", "--watch=false"])],
		follow: Follow::Nothing,
	},
	Stack {
		name: "qwik",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "qwik"), TYPESCRIPT],
		covers: &[Cover::all("ui", "qwik")],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: TSC,
		tests: VITEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "solid",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "solid"), ("framework", "solid-v2"), TYPESCRIPT],
		covers: &[Cover::all("ui", "solid")],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: TSC,
		tests: VITEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "vue",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "vue"), TYPESCRIPT],
		covers: &[Cover::all("ui", "vue")],
		sources: &[],
		hold: TYPESCRIPT_6,
		prepare: &[],
		browser: false,
		checks: &[Check::run(&["bun", "run", "vue-tsc", "--build"])],
		tests: VITEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "react",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "react"), TYPESCRIPT],
		covers: &[Cover::all("ui", "react")],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: &[Check::run(&["bun", "run", "tsc", "-b"])],
		tests: VITEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "lit",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "lit"), TYPESCRIPT],
		covers: &[Cover::all("ui", "lit")],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: TSC,
		tests: VITEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "hono",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "hono"), TYPESCRIPT],
		covers: &[Cover::pieces("backend", "hono", &["error", "middleware", "resource"])],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: TSC,
		tests: VITEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "hono-drizzle",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "hono"), TYPESCRIPT, ("database", "drizzle")],
		covers: &[
			Cover::all("backend", "hono-drizzle"),
			Cover::pieces("backend", "hono", &["resource"]).after(&[("database", "hono-drizzle")]),
		],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: TSC,
		tests: VITEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "hono-prisma",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "hono"), TYPESCRIPT, ("database", "prisma")],
		covers: &[
			Cover::all("backend", "hono-prisma"),
			Cover::pieces("backend", "hono", &["resource"]).after(&[("database", "hono-prisma")]),
		],
		sources: &[],
		hold: PRISMA,
		prepare: &[],
		browser: false,
		checks: TSC,
		tests: VITEST,
		follow: Follow::Prisma,
	},
	Stack {
		name: "express",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "express"), TYPESCRIPT],
		covers: &[Cover::pieces(
			"backend",
			"express",
			&["error", "middleware", "resource"],
		)],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: TSC,
		tests: VITEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "express-drizzle",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "express"), TYPESCRIPT, ("database", "drizzle")],
		covers: &[
			Cover::all("backend", "express-drizzle"),
			Cover::pieces("backend", "express", &["resource"]).after(&[("database", "express-drizzle")]),
		],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: TSC,
		tests: VITEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "express-prisma",
		ecosystem: Ecosystem::Bun,
		detects: &[("framework", "express"), TYPESCRIPT, ("database", "prisma")],
		covers: &[
			Cover::all("backend", "express-prisma"),
			Cover::pieces("backend", "express", &["resource"]).after(&[("database", "express-prisma")]),
		],
		sources: &[],
		hold: PRISMA,
		prepare: &[],
		browser: false,
		checks: TSC,
		tests: VITEST,
		follow: Follow::Prisma,
	},
	Stack {
		name: "fastapi",
		ecosystem: Ecosystem::Uv,
		detects: &[("framework", "fastapi"), ("language", "python")],
		covers: &[Cover::pieces(
			"backend",
			"fastapi",
			&["error", "middleware", "resource"],
		)],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: MYPY,
		tests: PYTEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "fastapi-sqlalchemy",
		ecosystem: Ecosystem::Uv,
		detects: &[("framework", "fastapi"), ("database", "sqlalchemy")],
		covers: &[
			Cover::pieces("backend", "fastapi", &["database"]),
			Cover::pieces("backend", "fastapi", &["resource"]).after(&[("database", "fastapi")]),
		],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: MYPY,
		tests: PYTEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "chi",
		ecosystem: Ecosystem::Go,
		detects: &[("framework", "chi"), ("language", "go")],
		covers: &[Cover::pieces("backend", "chi", &["error", "middleware", "resource"])],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: GO_CHECKS,
		tests: GO_TEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "chi-gorm",
		ecosystem: Ecosystem::Go,
		detects: &[("framework", "chi"), ("database", "gorm")],
		covers: &[Cover::pieces("backend", "chi", &["database", "resource"])],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: GO_CHECKS,
		tests: GO_TEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "echo",
		ecosystem: Ecosystem::Go,
		detects: &[("framework", "echo"), ("framework", "echo-v4"), ("language", "go")],
		covers: &[Cover::pieces("backend", "echo", &["error", "middleware", "resource"])],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: GO_CHECKS,
		tests: GO_TEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "echo-gorm",
		ecosystem: Ecosystem::Go,
		detects: &[("framework", "echo-v4"), ("database", "gorm")],
		covers: &[Cover::pieces("backend", "echo", &["database", "resource"])],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: GO_CHECKS,
		tests: GO_TEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "echo-v5",
		ecosystem: Ecosystem::Go,
		detects: &[("framework", "echo"), ("framework", "echo-v5"), ("language", "go")],
		covers: &[Cover::pieces(
			"backend",
			"echo-v5",
			&["error", "middleware", "resource"],
		)],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: GO_CHECKS,
		tests: GO_TEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "echo-v5-gorm",
		ecosystem: Ecosystem::Go,
		detects: &[("framework", "echo-v5"), ("database", "gorm")],
		covers: &[Cover::pieces("backend", "echo-v5", &["database", "resource"])],
		sources: &[],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: GO_CHECKS,
		tests: GO_TEST,
		follow: Follow::Nothing,
	},
	Stack {
		name: "flutter",
		ecosystem: Ecosystem::Flutter,
		detects: &[("framework", "flutter"), ("language", "dart")],
		covers: &[Cover::all("ui", "flutter")],
		sources: &["lib/", "test/"],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: &[Check::run(&["flutter", "analyze", "--fatal-infos", "--fatal-warnings"])],
		tests: &[Check::run(&["flutter", "test"])],
		follow: Follow::Nothing,
	},
	Stack {
		name: "compose-multiplatform",
		ecosystem: Ecosystem::Gradle,
		detects: &[("framework", "compose-multiplatform"), ("language", "kotlin")],
		covers: &[Cover::all("ui", "compose-multiplatform")],
		sources: &["shared/src/commonMain/kotlin/", "shared/src/jvmTest/kotlin/"],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: &[Check::run(&[
			"./gradlew",
			"--console=plain",
			":shared:compileKotlinJvm",
		])],
		tests: &[Check::run(&["./gradlew", "--console=plain", ":shared:jvmTest"])],
		follow: Follow::ComposeUiTest("shared"),
	},
	Stack {
		name: "compose-multiplatform-legacy",
		ecosystem: Ecosystem::Gradle,
		detects: &[("framework", "compose-multiplatform"), ("language", "kotlin")],
		covers: &[Cover::all("ui", "compose-multiplatform")],
		sources: &["composeApp/src/commonMain/kotlin/", "composeApp/src/jvmTest/kotlin/"],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: &[Check::run(&[
			"./gradlew",
			"--console=plain",
			":composeApp:compileKotlinJvm",
		])],
		tests: &[Check::run(&["./gradlew", "--console=plain", ":composeApp:jvmTest"])],
		follow: Follow::ComposeUiTest("composeApp"),
	},
	Stack {
		name: "compose",
		ecosystem: Ecosystem::Android,
		detects: &[
			("framework", "compose"),
			("framework", "android"),
			("language", "kotlin"),
		],
		covers: &[Cover::all("ui", "compose")],
		sources: &["app/src/main/java/", "app/src/test/java/"],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: &[Check::run(&[
			"./gradlew",
			"--console=plain",
			":app:compileDebugKotlin",
			":app:lintDebug",
		])],
		tests: &[Check::run(&["./gradlew", "--console=plain", ":app:testDebugUnitTest"])],
		follow: Follow::Robolectric,
	},
	Stack {
		name: "swiftui",
		ecosystem: Ecosystem::Xcode,
		detects: &[("language", "swift")],
		covers: &[Cover::all("ui", "swiftui")],
		sources: &["CuiloFixture/", "CuiloFixtureTests/"],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: &[Check::run(&[
			"xcodebuild",
			"build",
			"-scheme",
			XCODE_SCHEME,
			"-destination",
			"generic/platform=iOS Simulator",
			"-derivedDataPath",
			XCODE_DERIVED_DATA,
			"SWIFT_TREAT_WARNINGS_AS_ERRORS=YES",
			"GCC_TREAT_WARNINGS_AS_ERRORS=YES",
		])],
		tests: &[Check::run(&[
			"xcodebuild",
			"test",
			"-scheme",
			XCODE_SCHEME,
			"-destination",
			IOS_SIMULATOR,
			"-derivedDataPath",
			XCODE_DERIVED_DATA,
			"SWIFT_TREAT_WARNINGS_AS_ERRORS=YES",
			"GCC_TREAT_WARNINGS_AS_ERRORS=YES",
		])],
		follow: Follow::Nothing,
	},
	Stack {
		name: "swiftui-package",
		ecosystem: Ecosystem::SwiftPm,
		detects: &[("language", "swift"), ("package_manager", "swiftpm")],
		covers: &[Cover::all("ui", "swiftui-package")],
		sources: &["Sources/", "Tests/"],
		hold: &[],
		prepare: &[],
		browser: false,
		checks: &[Check::run(&["swift", "build", "-Xswiftc", "-warnings-as-errors"])],
		tests: &[Check::run(&["swift", "test", "-Xswiftc", "-warnings-as-errors"])],
		follow: Follow::Nothing,
	},
];
