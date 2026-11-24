import type { ComponentProps } from "react";
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
export function ExampleAtom({ tone = "neutral", className, ...props }: ExampleAtomProps) {
	return <span {...props} data-tone={tone} className={[classes.root, className].filter(Boolean).join(" ")} />;
}
