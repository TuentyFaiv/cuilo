use leptos::prelude::*;

// @cuilo if stack.tailwind
const ROOT: &str = "grid min-w-0 gap-2";
const LABEL: &str = "text-sm font-semibold";
const DESCRIPTION: &str = "text-sm text-zinc-500";
const CONTENT: &str = "flex flex-wrap items-center gap-2";
// @cuilo else
const ROOT: &str = "example-molecule";
const LABEL: &str = "example-molecule__label";
const DESCRIPTION: &str = "example-molecule__description";
const CONTENT: &str = "example-molecule__content";
// @cuilo end

/// Groups atoms under one accessible label, such as a field with its hint.
#[component]
pub fn ExampleMolecule(
	/// Unique id; the label and description ids derive from it.
	#[prop(into)]
	id: String,
	/// Names the group for assistive technology.
	#[prop(into)]
	label: String,
	/// Extra guidance shown under the label.
	#[prop(optional, into)]
	description: Option<String>,
	children: Children,
) -> impl IntoView {
	let label_id = format!("{id}-label");
	let labelled_by = label_id.clone();
	let description_id = description.as_ref().map(|_| format!("{id}-description"));
	let described_by = description_id.clone();

	view! {
		<fieldset
			id=id
			aria-labelledby=labelled_by
			aria-describedby=described_by
			class=ROOT
		>
			<span id=label_id class=LABEL>
				{label}
			</span>
			{description.map(|description| view! { <p id=description_id class=DESCRIPTION>{description}</p> })}
			<div class=CONTENT>{children()}</div>
		</fieldset>
	}
}
