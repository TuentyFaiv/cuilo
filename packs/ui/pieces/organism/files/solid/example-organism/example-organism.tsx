import type { ComponentProps, JSX } from "@solidjs/web";
import { Show, createUniqueId, omit } from "solid-js";
// @cuilo if !stack.tailwind

import styles from "./example-organism.module.css";
// @cuilo end

export type ExampleOrganismProps = Omit<ComponentProps<"section">, "aria-labelledby"> & {
	/** Heading that names the section. */
	heading: JSX.Element;
	/** Controls shown next to the heading. */
	actions?: JSX.Element;
};

// @cuilo if stack.tailwind
const classes = {
	root: "grid gap-4 rounded-xl border border-zinc-200 p-6",
	header: "flex flex-wrap items-center justify-between gap-3",
	title: "text-lg font-semibold",
	actions: "flex gap-2",
};
// @cuilo else
const classes = styles;
// @cuilo end

/** A distinct, labelled region of the page, built from molecules. */
export function ExampleOrganism(props: ExampleOrganismProps) {
	const rest = omit(props, "heading", "actions", "class", "children");
	const headingId = createUniqueId();

	return (
		<section {...rest} aria-labelledby={headingId} class={[classes.root, props.class]}>
			<header class={classes.header}>
				<h2 id={headingId} class={classes.title}>
					{props.heading}
				</h2>
				<Show when={props.actions}>
					<div class={classes.actions}>{props.actions}</div>
				</Show>
			</header>
			{props.children}
		</section>
	);
}
