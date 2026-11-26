import { type ComponentProps, type JSX, Show, createUniqueId, splitProps } from "solid-js";
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
	const [local, rest] = splitProps(props, ["label", "description", "class", "children"]);
	const id = createUniqueId();
	const labelId = `${id}-label`;
	const descriptionId = `${id}-description`;

	return (
		<fieldset
			{...rest}
			aria-labelledby={labelId}
			aria-describedby={local.description ? descriptionId : undefined}
			class={[classes.root, local.class].filter(Boolean).join(" ")}
		>
			<span id={labelId} class={classes.label}>
				{local.label}
			</span>
			<Show when={local.description}>
				<p id={descriptionId} class={classes.description}>
					{local.description}
				</p>
			</Show>
			<div class={classes.content}>{local.children}</div>
		</fieldset>
	);
}
