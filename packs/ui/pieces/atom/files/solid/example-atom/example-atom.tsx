import type { ComponentProps } from "@solidjs/web";
import { omit } from "solid-js";
// @cuilo if !stack.tailwind

import styles from "./example-atom.module.css";
// @cuilo end

export type ExampleAtomTone = "neutral" | "accent";

export type ExampleAtomProps = ComponentProps<"span"> & {
	/** Visual emphasis. */
	tone?: ExampleAtomTone;
};

// @cuilo if stack.tailwind
const classes = {
	root: "inline-flex items-center gap-1 rounded-full bg-zinc-100 px-2 py-0.5 text-xs font-semibold text-zinc-700 data-[tone=accent]:bg-indigo-600 data-[tone=accent]:text-white",
};
// @cuilo else
const classes = styles;
// @cuilo end

/** The smallest UI building block: an inline, label-sized element. */
export function ExampleAtom(props: ExampleAtomProps) {
	const rest = omit(props, "tone", "class");

	return <span {...rest} data-tone={props.tone ?? "neutral"} class={[classes.root, props.class]} />;
}
