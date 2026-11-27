import type { ComponentProps, JSX } from "@solidjs/web";
import { Show, createUniqueId, omit } from "solid-js";
// @cuilo if !stack.tailwind

import styles from "./example-molecule.module.css";
// @cuilo end

export type ExampleMoleculeProps = Omit<ComponentProps<"fieldset">, "aria-describedby" | "aria-labelledby" | "role"> & {
	/** Names the group for assistive technology. */
	label: JSX.Element;
	/** Extra guidance shown under the label. */
	description?: JSX.Element;
};

// @cuilo if stack.tailwind
const classes = {
	root: "grid min-w-0 gap-2",
	label: "text-sm font-semibold",
	description: "text-sm text-zinc-500",
	content: "flex flex-wrap items-center gap-2",
};
// @cuilo else
const classes = styles;
// @cuilo end

/** Groups atoms under one accessible label, such as a field with its hint. */
export function ExampleMolecule(props: ExampleMoleculeProps) {
	const rest = omit(props, "label", "description", "class", "children");
	const id = createUniqueId();
	const labelId = `${id}-label`;
	const descriptionId = `${id}-description`;

	return (
		<fieldset
			{...rest}
			aria-labelledby={labelId}
			aria-describedby={props.description ? descriptionId : undefined}
			class={[classes.root, props.class]}
		>
			<span id={labelId} class={classes.label}>
				{props.label}
			</span>
			<Show when={props.description}>
				<p id={descriptionId} class={classes.description}>
					{props.description}
				</p>
			</Show>
			<div class={classes.content}>{props.children}</div>
		</fieldset>
	);
}
