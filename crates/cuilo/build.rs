//! Records the target triple, which names the release binary `upgrade` downloads.

fn main() {
	if let Ok(target) = std::env::var("TARGET") {
		println!("cargo::rustc-env=CUILO_TARGET={target}");
	}
}
