import { type PropsOf, Slot, component$, useId, useStylesScoped$ } from "@builder.io/qwik"; // @cuilo if !stack.tailwind
import { type PropsOf, Slot, component$, useId } from "@builder.io/qwik"; // @cuilo if stack.tailwind
// @cuilo if !stack.tailwind

import styles from "./example-organism.css?inline";
// @cuilo end

export type ExampleOrganismProps = Omit<PropsOf<"section">, "aria-labelledby"> & {
	/** Heading that names the section. */
	heading: string;
};

// @cuilo if stack.tailwind
const classes = {
	root: "grid gap-4 rounded-xl border border-zinc-200 p-6",
	header: "flex flex-wrap items-center justify-between gap-3",
	title: "text-lg font-semibold",
	actions: "flex gap-2",
};
// @cuilo else
const classes = { root: "root", header: "header", title: "title", actions: "actions" };
// @cuilo end

/** A distinct, labelled region of the page; put controls in the `actions` slot. */
export const ExampleOrganism = component$<ExampleOrganismProps>(({ heading, class: className, ...props }) => {
	// @cuilo if !stack.tailwind
	useStylesScoped$(styles);
	// @cuilo end
	const headingId = useId();

	return (
		<section {...props} aria-labelledby={headingId} class={[classes.root, className]}>
			<header class={classes.header}>
				<h2 id={headingId} class={classes.title}>
					{heading}
				</h2>
				<div class={classes.actions}>
					<Slot name="actions" />
				</div>
			</header>
			<Slot />
		</section>
	);
});
