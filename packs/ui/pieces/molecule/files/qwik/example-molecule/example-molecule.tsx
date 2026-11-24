import { type PropsOf, Slot, component$, useId, useStylesScoped$ } from "@builder.io/qwik"; // @cuilo if !stack.tailwind
import { type PropsOf, Slot, component$, useId } from "@builder.io/qwik"; // @cuilo if stack.tailwind
// @cuilo if !stack.tailwind

import styles from "./example-molecule.css?inline";
// @cuilo end

export type ExampleMoleculeProps = Omit<PropsOf<"fieldset">, "aria-describedby" | "aria-labelledby" | "role"> & {
	/** Names the group for assistive technology. */
	label: string;
	/** Extra guidance shown under the label. */
	description?: string;
};

// @cuilo if stack.tailwind
const classes = {
	root: "grid min-w-0 gap-2",
	label: "text-sm font-semibold",
	description: "text-sm text-zinc-500",
	content: "flex flex-wrap items-center gap-2",
};
// @cuilo else
const classes = { root: "root", label: "label", description: "description", content: "content" };
// @cuilo end

/** Groups atoms under one accessible label, such as a field with its hint. */
export const ExampleMolecule = component$<ExampleMoleculeProps>(
	({ label, description, class: className, ...props }) => {
		// @cuilo if !stack.tailwind
		useStylesScoped$(styles);
		// @cuilo end
		const id = useId();
		const labelId = `${id}-label`;
		const descriptionId = `${id}-description`;

		return (
			<fieldset
				{...props}
				aria-labelledby={labelId}
				aria-describedby={description ? descriptionId : undefined}
				class={[classes.root, className]}
			>
				<span id={labelId} class={classes.label}>
					{label}
				</span>
				{description && (
					<p id={descriptionId} class={classes.description}>
						{description}
					</p>
				)}
				<div class={classes.content}>
					<Slot />
				</div>
			</fieldset>
		);
	},
);
