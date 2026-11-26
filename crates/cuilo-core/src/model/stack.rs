//! Facts about a project's stack, used to pick variants and in conditions.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model::{Fact, Value, Values};

/// Detected or configured stack facts, e.g. `framework = {react, next}`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Stack(BTreeMap<Fact, BTreeSet<String>>);

impl Stack {
	/// Whether no fact is known.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		self.0.values().all(BTreeSet::is_empty)
	}

	/// Adds one fact value.
	pub fn insert(&mut self, fact: Fact, value: &str) {
		self.0.entry(fact).or_default().insert(value.to_owned());
	}

	/// Replaces every value of `fact`.
	pub fn set(&mut self, fact: Fact, values: impl IntoIterator<Item = String>) {
		self.0.insert(fact, values.into_iter().collect());
	}

	/// Whether `fact` includes `value`.
	#[must_use]
	pub fn has(&self, fact: Fact, value: &str) -> bool {
		self.0.get(&fact).is_some_and(|values| values.contains(value))
	}

	/// Values of `fact`.
	pub fn values(&self, fact: Fact) -> impl Iterator<Item = &str> {
		self.0.get(&fact).into_iter().flatten().map(String::as_str)
	}

	/// Whether every requirement holds.
	#[must_use]
	pub fn satisfies(&self, requirements: &BTreeMap<Fact, String>) -> bool {
		requirements.iter().all(|(fact, value)| self.has(*fact, value))
	}

	/// How relative imports are written; [`ImportStyle::Extensionless`] when no style is recorded.
	#[must_use]
	pub fn import_style(&self) -> ImportStyle {
		self
			.values(Fact::ImportStyle)
			.find_map(ImportStyle::from_value)
			.unwrap_or_default()
	}

	/// Records `style` as the only import style.
	pub fn set_import_style(&mut self, style: ImportStyle) {
		self.set(Fact::ImportStyle, [style.value().to_owned()]);
	}

	/// Condition values: `stack.<value>` is true for every known fact value.
	#[must_use]
	pub fn condition_values(&self) -> Values {
		self
			.0
			.values()
			.flatten()
			.map(|value| (format!("stack.{value}"), Value::Bool(true)))
			.collect()
	}
}

/// How generated JavaScript and TypeScript write relative imports, recorded as [`Fact::ImportStyle`].
///
/// Its values start with `imports-`, which no other fact's value does, so `stack.imports-js` and the
/// others never collide with a framework, tool or runtime name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImportStyle {
	/// `./foo`, as bundlers, `moduleResolution: "bundler"` and older resolutions read it.
	#[default]
	Extensionless,
	/// `./foo.js` for `foo.ts`: the file's name once compiled, as `NodeNext` and `Node16` need.
	RuntimeExtension,
	/// `./foo.ts`: the file's own name, as Deno, `allowImportingTsExtensions` and
	/// `rewriteRelativeImportExtensions` read it.
	SourceExtension,
}

impl ImportStyle {
	/// Every style, in the order [`Self::value`] lists them.
	pub const ALL: [Self; 3] = [Self::Extensionless, Self::RuntimeExtension, Self::SourceExtension];

	/// The fact value, e.g. `imports-js`.
	#[must_use]
	pub fn value(self) -> &'static str {
		match self {
			Self::Extensionless => "imports-extensionless",
			Self::RuntimeExtension => "imports-js",
			Self::SourceExtension => "imports-ts",
		}
	}

	/// The style a fact value names.
	#[must_use]
	pub fn from_value(value: &str) -> Option<Self> {
		Self::ALL.into_iter().find(|style| style.value() == value)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn satisfies_should_require_every_fact() {
		let mut stack = Stack::default();
		stack.insert(Fact::Framework, "react");

		let wants = BTreeMap::from([
			(Fact::Framework, "react".to_owned()),
			(Fact::Language, "typescript".to_owned()),
		]);

		assert!(!stack.satisfies(&wants));
	}

	#[test]
	fn condition_values_should_expose_stack_prefix() {
		let mut stack = Stack::default();
		stack.insert(Fact::Styling, "tailwind");

		assert_eq!(stack.condition_values().get("stack.tailwind"), Some(&Value::Bool(true)));
	}

	#[test]
	fn import_style_should_default_to_extensionless_and_round_trip_through_its_value() {
		let mut stack = Stack::default();
		assert_eq!(stack.import_style(), ImportStyle::Extensionless);

		stack.set_import_style(ImportStyle::RuntimeExtension);

		assert_eq!(stack.import_style(), ImportStyle::RuntimeExtension);
		assert_eq!(stack.values(Fact::ImportStyle).collect::<Vec<_>>(), ["imports-js"]);
		assert_eq!(
			stack.condition_values().get("stack.imports-js"),
			Some(&Value::Bool(true))
		);
		assert_eq!(
			ImportStyle::from_value("imports-ts"),
			Some(ImportStyle::SourceExtension)
		);
		assert_eq!(ImportStyle::from_value("js"), None);
	}
}
